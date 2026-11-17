//! 有界尾读、增量游标和按需工作线程。
use super::{clip, projects::ProjectNames, Parser, Snapshot};
use std::{
    collections::{HashMap, HashSet},
    fs::{self, File},
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    sync::{mpsc, Arc, Mutex},
    thread::JoinHandle,
    time::{Duration, Instant},
};

const TAIL: u64 = 2 * 1024 * 1024;
const LINE: usize = 256 * 1024;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Request {
    pub root: PathBuf,
    pub full: bool,
    pub pinned: Option<PathBuf>,
    pub hold: Option<PathBuf>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Update {
    pub selected: Option<Snapshot>,
    pub recent: Vec<Snapshot>,
    pub latest: Option<PathBuf>,
    pub error: Option<String>,
    pub dsh_status: String,
    pub ready: bool,
}

struct Cursor {
    parser: Parser,
    offset: u64,
    boundary: Vec<u8>,
    skip: bool,
    booted: bool,
}
impl Cursor {
    fn new(path: &Path, full: bool) -> Self {
        let mut parser = Parser::new(full);
        parser.set_path(path.to_owned());
        Self {
            parser,
            offset: 0,
            boundary: Vec::new(),
            skip: false,
            booted: false,
        }
    }
    fn read(&mut self, path: &Path) -> std::io::Result<()> {
        let mut f = File::open(path)?;
        let len = f.metadata()?.len();
        if self.booted {
            let mut boundary = vec![0; self.boundary.len()];
            let matches = len >= self.offset
                && f.seek(SeekFrom::Start(
                    self.offset.saturating_sub(boundary.len() as u64),
                ))
                .is_ok()
                && f.read_exact(&mut boundary).is_ok()
                && boundary == self.boundary;
            if !matches {
                let full = self.parser.full;
                *self = Self::new(path, full);
            }
        }
        if !self.booted {
            // 首行元信息单独读取；大文件从尾部引导，禁止回放全部历史。
            let mut head = Vec::new();
            f.seek(SeekFrom::Start(0))?;
            (&mut f).take(LINE as u64).read_to_end(&mut head)?;
            if let Some(end) = head.iter().position(|b| *b == b'\n') {
                if let Ok(v) = serde_json::from_slice(&head[..end]) {
                    self.parser.ingest(&v);
                }
            }
            self.offset = len.saturating_sub(TAIL);
            self.skip = self.offset > 0;
            self.booted = true;
            if self.skip {
                self.parser.mark_truncated();
            }
        }
        if self.offset == len {
            return Ok(());
        }
        f.seek(SeekFrom::Start(self.offset))?;
        let mut bytes = Vec::new();
        (&mut f).take(TAIL).read_to_end(&mut bytes)?;
        let mut start = 0;
        for (end, b) in bytes.iter().enumerate() {
            if *b != b'\n' {
                continue;
            }
            if self.skip {
                self.skip = false;
            } else if end - start > LINE {
                self.parser.mark_truncated();
            } else if let Ok(value) = serde_json::from_slice(&bytes[start..end]) {
                self.parser.ingest(&value);
            }
            start = end + 1;
        }
        // 未完成的正常行留在文件中，下次补读；超长行直接跳过并标注。
        if bytes.len() - start > LINE {
            start = bytes.len();
            self.skip = true;
            self.parser.mark_truncated();
        }
        self.offset += start as u64;
        let n = self.offset.min(32) as usize;
        self.boundary.resize(n, 0);
        f.seek(SeekFrom::Start(self.offset - n as u64))?;
        f.read_exact(&mut self.boundary)?;
        Ok(())
    }
}

pub struct Reader {
    root: PathBuf,
    cursors: HashMap<PathBuf, Cursor>,
    body: Option<(PathBuf, Cursor)>,
    titles: HashMap<String, String>,
    projects: ProjectNames,
    index_len: u64,
    observed: Vec<PathBuf>,
    ignored: HashSet<PathBuf>,
}
impl Reader {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            cursors: HashMap::new(),
            body: None,
            titles: HashMap::new(),
            projects: ProjectNames::default(),
            index_len: u64::MAX,
            observed: Vec::new(),
            ignored: HashSet::new(),
        }
    }
    pub(super) fn observe(&mut self, path: PathBuf) {
        if path.starts_with(self.root.join("sessions"))
            && path.extension().is_some_and(|s| s == "jsonl")
        {
            self.ignored.remove(&path);
            if !self.observed.contains(&path) {
                if self.observed.len() >= 32 {
                    self.observed.remove(0);
                }
                self.observed.push(path);
            }
        }
    }
    pub fn retained_body_bytes(&self) -> usize {
        self.body.as_ref().map_or(0, |(_, c)| {
            c.parser.data.messages.iter().map(|m| m.text.len()).sum()
        })
    }
    fn titles(&mut self) {
        let path = self.root.join("session_index.jsonl");
        let Ok(mut f) = File::open(path) else { return };
        let Ok(meta) = f.metadata() else { return };
        if self.index_len == meta.len() {
            return;
        }
        self.index_len = meta.len();
        self.titles.clear();
        let offset = meta.len().saturating_sub(256 * 1024);
        if f.seek(SeekFrom::Start(offset)).is_err() {
            return;
        }
        let mut bytes = Vec::new();
        if f.take(256 * 1024).read_to_end(&mut bytes).is_err() {
            return;
        }
        for line in bytes.split(|b| *b == b'\n').rev().take(256) {
            if let Ok(v) = serde_json::from_slice::<serde_json::Value>(line) {
                if let (Some(id), Some(title)) = (v["id"].as_str(), v["thread_name"].as_str()) {
                    self.titles
                        .entry(id.into())
                        .or_insert_with(|| clip(title, 240));
                }
            }
        }
    }
    pub fn poll(&mut self, request: &Request) -> Update {
        self.titles();
        self.projects.refresh(&self.root);
        let sessions = self.root.join("sessions");
        if !sessions.is_dir() {
            self.cursors.clear();
            self.body = None;
            return Update {
                error: Some("未找到 Codex 会话目录，请检查来源路径".into()),
                ..Default::default()
            };
        }
        let mut paths = Vec::new();
        discover(&sessions, 0, &mut paths);
        for path in self.cursors.keys().chain(self.observed.iter()) {
            if !paths.contains(path) {
                paths.push(path.clone());
            }
        }
        self.observed.clear();
        for path in [request.pinned.as_ref(), request.hold.as_ref()]
            .into_iter()
            .flatten()
        {
            self.ignored.remove(path);
            if path.starts_with(&sessions) && !paths.contains(path) {
                paths.push(path.clone());
            }
        }
        let keep: HashSet<_> = paths.iter().cloned().collect();
        self.cursors.retain(|p, _| keep.contains(p));
        let mut failures = HashSet::new();
        for path in &paths {
            if self.ignored.contains(path) {
                continue;
            }
            let c = self
                .cursors
                .entry(path.clone())
                .or_insert_with(|| Cursor::new(path, false));
            if c.read(path).is_err() {
                failures.insert(path.clone());
            }
        }
        let mut recent: Vec<_> = self
            .cursors
            .values()
            .map(|c| c.parser.data.summary())
            .filter(|s| !s.subagent && s.updated_at > 0 && !failures.contains(&s.path))
            .collect();
        recent.sort_by(|a, b| {
            b.activity_ms
                .cmp(&a.activity_ms)
                .then_with(|| b.path.cmp(&a.path))
        });
        recent.truncate(32);
        let latest = recent.first().map(|s| s.path.clone());
        let wanted = request
            .pinned
            .as_ref()
            .or(request.hold.as_ref())
            .cloned()
            .or_else(|| latest.clone());
        let mut error = None;
        let mut selected = wanted
            .as_ref()
            .and_then(|path| self.cursors.get(path))
            .map(|c| c.parser.data.summary())
            .filter(|s| !s.subagent && s.updated_at > 0);
        if let Some(path) = wanted.as_ref() {
            if !path.starts_with(&sessions) || failures.contains(path) || selected.is_none() {
                error = Some("会话暂不可读或已被删除，请重试或选择其他会话".into());
            }
            if request.full && path.starts_with(&sessions) && selected.is_some() {
                if self.body.as_ref().is_none_or(|(p, _)| p != path) {
                    self.body = Some((path.clone(), Cursor::new(path, true)));
                }
                if let Some((_, cursor)) = &mut self.body {
                    match cursor.read(path) {
                        Ok(()) => {
                            let s = cursor.parser.snapshot();
                            if !s.subagent && s.updated_at > 0 {
                                selected = Some(s);
                            }
                        }
                        Err(_) => error = Some("读取中断，显示上次记录".into()),
                    }
                }
            }
        }
        if !request.full || selected.is_none() {
            self.body = None;
        }
        for s in recent.iter_mut().chain(selected.iter_mut()) {
            if let Some(title) = self.titles.get(&s.id) {
                s.title = title.clone();
            }
            if let Some(cursor) = self.cursors.get(&s.path) {
                if let Some(name) = self.projects.name(&s.id, &cursor.parser.cwd) {
                    s.project = name.to_owned();
                }
            }
        }
        let keep: HashSet<_> = recent
            .iter()
            .map(|s| s.path.clone())
            .chain(wanted)
            .collect();
        if self.ignored.len() > 128 {
            self.ignored.clear();
        }
        self.cursors.retain(|path, _| {
            if keep.contains(path) {
                true
            } else {
                self.ignored.insert(path.clone());
                false
            }
        });
        Update {
            selected,
            recent,
            latest,
            error,
            dsh_status: String::new(),
            ready: true,
        }
    }
}
// 仅枚举最近的日期目录及最多 32 个候选文件，不读取全部历史内容。
fn discover(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    if depth > 3 || out.len() >= 32 {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = entries.flatten().collect();
    entries.sort_by_key(|e| std::cmp::Reverse(e.file_name()));
    for entry in entries {
        if out.len() >= 32 {
            break;
        }
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        let path = entry.path();
        if kind.is_dir() && !kind.is_symlink() {
            discover(&path, depth + 1, out);
        } else if kind.is_file() && path.extension().is_some_and(|s| s == "jsonl") {
            out.push(path);
        }
    }
}

pub struct Service {
    tx: mpsc::Sender<Option<(Request, super::Sources)>>,
    output: Arc<Mutex<Option<Update>>>,
    thread: Option<JoinHandle<()>>,
    request: Request,
    sources: super::Sources,
}
impl Service {
    pub fn start(request: Request, ctx: eframe::egui::Context) -> Option<Self> {
        Self::start_sources(request, super::Sources::default(), ctx)
    }
    pub fn start_sources(
        request: Request,
        sources: super::Sources,
        ctx: eframe::egui::Context,
    ) -> Option<Self> {
        let (tx, rx) = mpsc::channel::<Option<(Request, super::Sources)>>();
        let output = Arc::new(Mutex::new(None));
        let slot = output.clone();
        let initial = request.clone();
        let initial_sources = sources.clone();
        let thread = crate::resources::AGENT_SESSION.spawn(move || {
            let mut request = initial;
            let mut sources = initial_sources;
            let mut reader = super::sources::ReaderSet::new(request.root.clone(), sources.clone());
            let mut watcher = sources.codex.then(|| Watcher::new(&request.root));
            let mut last = None;
            let mut last_poll = Instant::now() - Duration::from_secs(3);
            let mut dirty = None;
            loop {
                if last_poll.elapsed() >= Duration::from_secs(2)
                    || dirty.is_some_and(|at: Instant| at.elapsed() >= Duration::from_millis(250))
                {
                    let update = reader.poll(&request);
                    if last.as_ref() != Some(&update) {
                        last = Some(update.clone());
                        *slot.lock().unwrap_or_else(|e| e.into_inner()) = Some(update);
                        ctx.request_repaint();
                    }
                    last_poll = Instant::now();
                    dirty = None;
                }
                match rx.recv_timeout(Duration::from_millis(100)) {
                    Ok(Some((next, next_sources))) => {
                        if next.root != request.root || next_sources != sources {
                            reader = super::sources::ReaderSet::new(
                                next.root.clone(),
                                next_sources.clone(),
                            );
                            watcher = next_sources.codex.then(|| Watcher::new(&next.root));
                            last = None;
                        }
                        request = next;
                        sources = next_sources;
                        last_poll = Instant::now() - Duration::from_secs(3);
                    }
                    Ok(None) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                }
                let changed = watcher.as_mut().map(|w| w.changes()).unwrap_or_default();
                if !changed.is_empty() {
                    for path in changed {
                        reader.observe(path);
                    }
                    dirty.get_or_insert_with(Instant::now);
                }
            }
        })?;
        Some(Self {
            tx,
            output,
            thread: Some(thread),
            request,
            sources,
        })
    }
    pub fn configure(&mut self, request: Request) {
        self.configure_sources(request, self.sources.clone());
    }
    pub fn configure_sources(&mut self, request: Request, sources: super::Sources) {
        if self.request != request || self.sources != sources {
            self.request = request.clone();
            self.sources = sources.clone();
            let _ = self.tx.send(Some((request, sources)));
        }
    }
    pub fn poll(&self) -> Option<Update> {
        self.output.lock().unwrap_or_else(|e| e.into_inner()).take()
    }
}
impl Drop for Service {
    fn drop(&mut self) {
        let _ = self.tx.send(None);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[cfg(windows)]
#[repr(C)]
#[derive(Default)]
struct Overlapped {
    internal: usize,
    internal_high: usize,
    offset: u32,
    offset_high: u32,
    event: isize,
}
#[cfg(windows)]
struct Watcher {
    root: PathBuf,
    handle: isize,
    overlapped: Box<Overlapped>,
    buffer: Vec<u8>,
    pending: bool,
}
#[cfg(windows)]
impl Watcher {
    fn new(root: &Path) -> Self {
        use std::os::windows::ffi::OsStrExt;
        let path = if root.join("sessions").is_dir() {
            root.join("sessions")
        } else {
            root.to_owned()
        };
        let wide: Vec<_> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        let handle = unsafe {
            CreateFileW(
                wide.as_ptr(),
                1,
                7,
                std::ptr::null(),
                3,
                0x02000000 | 0x40000000,
                0,
            )
        };
        let event = unsafe { CreateEventW(std::ptr::null(), 1, 0, std::ptr::null()) };
        let mut watcher = Self {
            root: path,
            handle,
            overlapped: Box::new(Overlapped {
                event,
                ..Default::default()
            }),
            buffer: vec![0; 64 * 1024],
            pending: false,
        };
        watcher.arm();
        watcher
    }
    fn arm(&mut self) {
        if self.handle == 0 || self.handle == -1 || self.overlapped.event == 0 {
            return;
        }
        unsafe {
            ResetEvent(self.overlapped.event);
            self.pending = ReadDirectoryChangesW(
                self.handle,
                self.buffer.as_mut_ptr().cast(),
                self.buffer.len() as u32,
                1,
                0x1 | 0x2 | 0x8 | 0x10,
                std::ptr::null_mut(),
                &mut *self.overlapped,
                std::ptr::null(),
            ) != 0;
        }
    }
    fn changes(&mut self) -> Vec<PathBuf> {
        let mut out = Vec::new();
        if !self.pending || unsafe { WaitForSingleObject(self.overlapped.event, 0) } != 0 {
            return out;
        }
        let mut transferred = 0;
        let ok =
            unsafe { GetOverlappedResult(self.handle, &mut *self.overlapped, &mut transferred, 0) }
                != 0;
        self.pending = false;
        if ok {
            let end = (transferred as usize).min(self.buffer.len());
            let mut at = 0;
            while at + 12 <= end {
                let next = u32::from_le_bytes(self.buffer[at..at + 4].try_into().unwrap()) as usize;
                let len =
                    u32::from_le_bytes(self.buffer[at + 8..at + 12].try_into().unwrap()) as usize;
                if len % 2 != 0 || at + 12 + len > end {
                    break;
                }
                let name: Vec<u16> = self.buffer[at + 12..at + 12 + len]
                    .chunks_exact(2)
                    .map(|b| u16::from_le_bytes([b[0], b[1]]))
                    .collect();
                let path = self.root.join(String::from_utf16_lossy(&name));
                if path.extension().is_some_and(|s| s == "jsonl") {
                    out.push(path);
                }
                if next < 12 || at + next > end {
                    break;
                }
                at += next;
            }
        }
        self.arm();
        out
    }
}
#[cfg(windows)]
impl Drop for Watcher {
    fn drop(&mut self) {
        unsafe {
            if self.handle != 0 && self.handle != -1 {
                if self.pending {
                    CancelIoEx(self.handle, &mut *self.overlapped);
                    let mut done = 0;
                    GetOverlappedResult(self.handle, &mut *self.overlapped, &mut done, 1);
                }
                CloseHandle(self.handle as *mut _);
            }
            if self.overlapped.event != 0 {
                CloseHandle(self.overlapped.event as *mut _);
            }
        }
    }
}
#[cfg(windows)]
#[link(name = "kernel32")]
extern "system" {
    fn CreateFileW(
        path: *const u16,
        access: u32,
        share: u32,
        security: *const std::ffi::c_void,
        creation: u32,
        flags: u32,
        template: isize,
    ) -> isize;
    fn CreateEventW(
        security: *const std::ffi::c_void,
        manual: i32,
        initial: i32,
        name: *const u16,
    ) -> isize;
    fn ReadDirectoryChangesW(
        handle: isize,
        buffer: *mut std::ffi::c_void,
        len: u32,
        subtree: i32,
        filter: u32,
        returned: *mut u32,
        overlapped: *mut Overlapped,
        completion: *const std::ffi::c_void,
    ) -> i32;
    fn GetOverlappedResult(
        handle: isize,
        overlapped: *mut Overlapped,
        transferred: *mut u32,
        wait: i32,
    ) -> i32;
    fn CancelIoEx(handle: isize, overlapped: *mut Overlapped) -> i32;
    fn WaitForSingleObject(handle: isize, millis: u32) -> u32;
    fn ResetEvent(handle: isize) -> i32;
    fn CloseHandle(handle: *mut std::ffi::c_void) -> i32;
}
#[cfg(not(windows))]
struct Watcher;
#[cfg(not(windows))]
impl Watcher {
    fn new(_: &Path) -> Self {
        Self
    }
    fn changes(&mut self) -> Vec<PathBuf> {
        Vec::new()
    }
}
