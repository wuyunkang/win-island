//! 逐会话的只读回执；缺失或过期的回执不能代替已读确认。
use super::Source;
use std::path::Path;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ReadReceipt {
    pub revision: String,
    pub first_unread_turn: Option<String>,
    pub last_published_turn: Option<String>,
    pub viewed_at_ms: Option<i64>,
    pub ui_unread: Option<bool>,
    pub ui_context: String,
}
impl ReadReceipt {
    pub fn covers(&self, turn: &str, activity_ms: i64, previous: Option<&Self>) -> bool {
        if self.ui_unread == Some(true) {
            return false;
        }
        if self.ui_unread == Some(false)
            && !self.ui_context.is_empty()
            && previous
                .is_some_and(|old| old.ui_context == self.ui_context && old.ui_unread == Some(true))
        {
            return true;
        }
        if let Some(viewed_at) = self.viewed_at_ms {
            return activity_ms > 0 && viewed_at >= activity_ms;
        }
        if self.revision.is_empty()
            || self.first_unread_turn.is_some()
            || self
                .last_published_turn
                .as_deref()
                .is_none_or(str::is_empty)
        {
            return false;
        }
        (!turn.is_empty() && self.last_published_turn.as_deref() == Some(turn))
            || previous.is_some_and(|old| !old.revision.is_empty() && old.revision != self.revision)
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionReadReceipt {
    pub source: Source,
    pub id: String,
    pub receipt: ReadReceipt,
}
#[cfg(test)]
pub(super) fn load(root: &Path) -> Vec<SessionReadReceipt> {
    load_with_ids(root, &[])
}

pub(super) fn load_with_ids(root: &Path, ids: &[String]) -> Vec<SessionReadReceipt> {
    let mut rows = {
        #[cfg(windows)]
        {
            native::load(&root.join("state_5.sqlite"))
        }
        #[cfg(not(windows))]
        {
            let _ = root;
            Vec::new()
        }
    };
    if let Some((context, unread)) = desktop_unread(root) {
        for id in ids {
            if !rows.iter().any(|r| r.id == *id) {
                rows.push(SessionReadReceipt {
                    source: Source::Codex,
                    id: id.clone(),
                    receipt: ReadReceipt::default(),
                });
            }
        }
        for row in &mut rows {
            row.receipt.ui_unread = Some(unread.contains(&row.id));
            row.receipt.ui_context = context.clone();
        }
    }
    rows
}

fn desktop_unread(root: &Path) -> Option<(String, std::collections::HashSet<String>)> {
    use serde::Deserialize;
    use std::{collections::HashMap, io::Read};
    #[derive(Deserialize)]
    struct State {
        #[serde(rename = "electron-thread-read-state-v1")]
        reads: Option<DesktopReads>,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct DesktopReads {
        version: u32,
        unread_by_identity: HashMap<String, HashMap<String, Vec<String>>>,
    }
    let file = std::fs::File::open(root.join(".codex-global-state.json")).ok()?;
    if file.metadata().ok()?.len() > 4 * 1024 * 1024 {
        return None;
    }
    let mut bytes = Vec::new();
    file.take(4 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    let state: State = serde_json::from_slice(&bytes).ok()?;
    let reads = state.reads?;
    if reads.version != 1 {
        return None;
    }
    let mut local = reads
        .unread_by_identity
        .into_iter()
        .flat_map(|(owner, hosts)| {
            hosts
                .into_iter()
                .filter(|(host, _)| host.starts_with("local:"))
                .map(move |(host, ids)| (format!("{owner}/{host}"), ids))
        });
    let (context, ids) = local.next()?;
    // 多账号或多本机 Host 时不猜当前身份；仍可使用逐会话后端回执。
    if local.next().is_some() || context.len() > 512 || ids.len() > 16384 {
        return None;
    }
    Some((
        context,
        ids.into_iter()
            .filter(|id| !id.is_empty() && id.len() <= 256)
            .collect(),
    ))
}

#[cfg(windows)]
mod native {
    use super::*;
    use std::{
        ffi::{c_char, c_int, c_void, CStr, CString},
        ptr,
    };
    #[link(name = "winsqlite3")]
    extern "C" {
        fn sqlite3_open_v2(
            name: *const c_char,
            db: *mut *mut c_void,
            flags: c_int,
            vfs: *const c_char,
        ) -> c_int;
        fn sqlite3_exec(
            db: *mut c_void,
            sql: *const c_char,
            callback: Option<
                unsafe extern "C" fn(
                    *mut c_void,
                    c_int,
                    *mut *mut c_char,
                    *mut *mut c_char,
                ) -> c_int,
            >,
            data: *mut c_void,
            error: *mut *mut c_char,
        ) -> c_int;
        fn sqlite3_busy_timeout(db: *mut c_void, ms: c_int) -> c_int;
        fn sqlite3_close(db: *mut c_void) -> c_int;
    }
    pub(super) fn load(path: &Path) -> Vec<SessionReadReceipt> {
        let Ok(name) = CString::new(path.to_string_lossy().as_bytes()) else {
            return Vec::new();
        };
        unsafe extern "C" fn row(
            data: *mut c_void,
            count: c_int,
            values: *mut *mut c_char,
            _: *mut *mut c_char,
        ) -> c_int {
            if count != 4 || values.is_null() {
                return 0;
            }
            let text = |index: usize| -> Option<String> {
                let value = *values.add(index);
                if value.is_null() {
                    return None;
                }
                let text = CStr::from_ptr(value).to_str().ok()?;
                (text.len() <= 256).then(|| text.to_owned())
            };
            let (Some(id), Some(revision)) = (text(0), text(2)) else {
                return 0;
            };
            if id.is_empty() || revision.is_empty() {
                return 0;
            }
            // 非空但无法解析的未读字段必须保留为「未读」，不能被当成 NULL。
            let first_unread_turn = if (*values.add(1)).is_null() {
                None
            } else {
                Some(text(1).unwrap_or_else(|| "unknown".into()))
            };
            let rows = &mut *data.cast::<Vec<SessionReadReceipt>>();
            rows.push(SessionReadReceipt {
                source: Source::Codex,
                id,
                receipt: ReadReceipt {
                    revision,
                    first_unread_turn,
                    last_published_turn: text(3),
                    viewed_at_ms: None,
                    ..Default::default()
                },
            });
            0
        }
        let mut db = ptr::null_mut();
        let mut rows = Vec::new();
        unsafe {
            // 只读打开；锁冲突只在工作线程等候 10ms，失败时保留提醒。
            let opened = sqlite3_open_v2(name.as_ptr(), &mut db, 1, ptr::null()) == 0;
            let mut ok = false;
            if opened {
                sqlite3_busy_timeout(db, 10);
                let sql=c"SELECT thread_id,first_unread_turn,revision,last_published_turn FROM thread_read_receipts ORDER BY rowid DESC LIMIT 4096";
                ok = sqlite3_exec(
                    db,
                    sql.as_ptr(),
                    Some(row),
                    (&mut rows as *mut Vec<SessionReadReceipt>).cast(),
                    ptr::null_mut(),
                ) == 0;
            }
            if !db.is_null() {
                sqlite3_close(db);
            }
            if !ok {
                rows.clear();
            }
        }
        rows
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn codex_read_receipt_requires_the_current_turn_or_a_new_acknowledgement() {
        let old = ReadReceipt {
            revision: "old".into(),
            last_published_turn: Some("old-turn".into()),
            ..Default::default()
        };
        assert!(
            !old.covers("new-turn", 1000, Some(&old)),
            "旧已读不能清掉新一轮通知"
        );
        let current = ReadReceipt {
            revision: "current".into(),
            last_published_turn: Some("new-turn".into()),
            ..Default::default()
        };
        assert!(current.covers("new-turn", 1000, Some(&old)));
        let unread = ReadReceipt {
            first_unread_turn: Some("new-turn".into()),
            ..current.clone()
        };
        assert!(!unread.covers("new-turn", 1000, Some(&old)));
        let acknowledged = ReadReceipt {
            revision: "ack".into(),
            ..old.clone()
        };
        assert!(
            acknowledged.covers("new-turn", 1000, Some(&old)),
            "同一会话的新已读回执可确认运行中的询问"
        );
        assert!(!acknowledged.covers("new-turn", 1000, None));
        assert!(!ReadReceipt::default().covers("", 0, None));
    }
    #[test]
    fn dsh_view_receipt_must_be_at_least_as_new_as_the_notification() {
        assert!(!ReadReceipt {
            viewed_at_ms: Some(999),
            ..Default::default()
        }
        .covers("turn", 1000, None));
        assert!(ReadReceipt {
            viewed_at_ms: Some(1000),
            ..Default::default()
        }
        .covers("turn", 1000, None));
        assert!(!ReadReceipt {
            viewed_at_ms: Some(1000),
            ..Default::default()
        }
        .covers("turn", 0, None));
    }
    #[test]
    fn codex_ui_read_transition_acknowledges_questions_without_a_new_completed_turn() {
        let before = ReadReceipt {
            ui_unread: Some(true),
            ui_context: "owner-local".into(),
            revision: "unchanged".into(),
            first_unread_turn: Some("older".into()),
            last_published_turn: Some("older".into()),
            ..Default::default()
        };
        let read = ReadReceipt {
            ui_unread: Some(false),
            ..before.clone()
        };
        assert!(
            read.covers("question-in-progress", 1000, Some(&before)),
            "界面已读变化应清除当前询问"
        );
        let other = ReadReceipt {
            ui_context: "other-account".into(),
            ..read.clone()
        };
        assert!(!other.covers("question-in-progress", 1000, Some(&before)));
        let still_unread = ReadReceipt {
            first_unread_turn: None,
            last_published_turn: Some("question-in-progress".into()),
            ..before.clone()
        };
        assert!(
            !still_unread.covers("question-in-progress", 1000, Some(&before)),
            "原界面仍未读时不能被后端回执覆盖"
        );
    }
    #[test]
    fn codex_desktop_unread_state_tracks_pending_questions_without_a_database_row() {
        let root = std::env::temp_dir().join(format!(
            "island-ui-read-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join(".codex-global-state.json");
        let state = |unread: Vec<&str>| {
            serde_json::json!({"electron-thread-read-state-v1":{"version":1,"unreadByIdentity":{"owner":{"local:machine":unread}}}}).to_string()
        };
        std::fs::write(&path, state(vec!["pending"])).unwrap();
        let before = load_with_ids(&root, &["pending".into()])
            .pop()
            .unwrap()
            .receipt;
        assert_eq!(before.ui_unread, Some(true));
        std::fs::write(&path, state(vec![])).unwrap();
        let after = load_with_ids(&root, &["pending".into()])
            .pop()
            .unwrap()
            .receipt;
        assert!(after.covers("question-turn", 1000, Some(&before)));
        std::fs::write(&path, "{broken").unwrap();
        assert!(load_with_ids(&root, &["pending".into()]).is_empty());
        std::fs::remove_file(path).unwrap();
        std::fs::remove_dir(root).unwrap();
    }
    #[cfg(windows)]
    #[test]
    fn codex_receipt_database_is_read_only_and_refreshes_without_session_events() {
        use std::{
            ffi::{c_char, c_int, c_void, CString},
            ptr,
        };
        #[link(name = "winsqlite3")]
        extern "C" {
            fn sqlite3_open_v2(
                name: *const c_char,
                db: *mut *mut c_void,
                flags: c_int,
                vfs: *const c_char,
            ) -> c_int;
            fn sqlite3_exec(
                db: *mut c_void,
                sql: *const c_char,
                callback: Option<
                    unsafe extern "C" fn(
                        *mut c_void,
                        c_int,
                        *mut *mut c_char,
                        *mut *mut c_char,
                    ) -> c_int,
                >,
                data: *mut c_void,
                error: *mut *mut c_char,
            ) -> c_int;
            fn sqlite3_close(db: *mut c_void) -> c_int;
        }
        let root = std::env::temp_dir().join(format!(
            "island-read-中文-'{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("state_5.sqlite");
        assert!(load(&root).is_empty());
        assert!(!path.exists(), "查询缺失数据库不能创建文件");
        let name = CString::new(path.to_string_lossy().as_bytes()).unwrap();
        let mut db = ptr::null_mut();
        unsafe {
            assert_eq!(sqlite3_open_v2(name.as_ptr(), &mut db, 6, ptr::null()), 0);
            let sql=CString::new("CREATE TABLE thread_read_receipts(thread_id TEXT PRIMARY KEY,first_unread_turn TEXT,revision TEXT,last_published_turn TEXT);INSERT INTO thread_read_receipts VALUES('one',NULL,'r1','turn-one');INSERT INTO thread_read_receipts VALUES('two','turn-two','r2','turn-two');").unwrap();
            assert_eq!(
                sqlite3_exec(db, sql.as_ptr(), None, ptr::null_mut(), ptr::null_mut()),
                0
            );
            sqlite3_close(db);
        }
        let before = std::fs::metadata(&path).unwrap().modified().unwrap();
        let rows = load(&root);
        assert_eq!(rows.len(), 2, "应读取逐会话的真实回执");
        let one = rows.iter().find(|r| r.id == "one").unwrap();
        assert!(one.receipt.covers("turn-one", 1, None));
        assert!(!rows
            .iter()
            .find(|r| r.id == "two")
            .unwrap()
            .receipt
            .covers("turn-two", 1, None));
        assert_eq!(
            before,
            std::fs::metadata(&path).unwrap().modified().unwrap()
        );
        unsafe {
            assert_eq!(sqlite3_open_v2(name.as_ptr(), &mut db, 2, ptr::null()), 0);
            let sql=CString::new("UPDATE thread_read_receipts SET first_unread_turn=NULL,revision='read-two' WHERE thread_id='two'").unwrap();
            assert_eq!(
                sqlite3_exec(db, sql.as_ptr(), None, ptr::null_mut(), ptr::null_mut()),
                0
            );
            sqlite3_close(db);
        }
        assert!(load(&root)
            .iter()
            .find(|r| r.id == "two")
            .unwrap()
            .receipt
            .covers("turn-two", 1, None));
        std::fs::remove_file(path).unwrap();
        std::fs::remove_dir(root).unwrap();
    }
}
