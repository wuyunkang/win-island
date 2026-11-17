//! 只读 Windows API 采样；不安装驱动、不要求管理员权限。
use super::*;
use windows::{
    core::{w, PCWSTR},
    Win32::{
        Foundation::{CloseHandle, FILETIME},
        NetworkManagement::{IpHelper::*, Ndis::IfOperStatusUp},
        Storage::FileSystem::*,
        System::{
            Diagnostics::ToolHelp::*, Performance::*, ProcessStatus::*, SystemInformation::*,
            Threading::*,
        },
    },
};

#[derive(Default)]
pub struct Sampler {
    cpu: Option<[u64; 3]>,
    network: HashMap<u64, (u64, u64)>,
    at: Option<Instant>,
    processes: HashMap<(u32, u64), u64>,
    gpu: Option<Gpu>,
    gpu_tried: bool,
    drives: Vec<Drive>,
    drives_at: Option<Instant>,
}
fn ticks(time: FILETIME) -> u64 {
    (time.dwHighDateTime as u64) << 32 | time.dwLowDateTime as u64
}

#[cfg(test)]
mod switch_tests {
    use super::*;
    #[test]
    fn native_monitor_releases_disabled_sampling_channels() {
        let mut sampler = Sampler::default();
        let settings = Settings {
            show_gpu: false,
            show_processes: false,
            show_storage: false,
            ..Default::default()
        };
        let _ = sampler.sample(&settings);
        let disabled = sampler.sample(&settings.sampling_for_views(false, false));
        assert!(
            disabled.cpu.is_none() && disabled.memory_total.is_none() && disabled.gpu.is_none()
        );
        assert!(disabled.download.is_none() && disabled.upload.is_none());
        assert!(disabled.drives.is_empty() && disabled.processes.is_empty());
        assert!(
            sampler.cpu.is_none() && sampler.network.is_empty() && sampler.processes.is_empty()
        );
        assert!(sampler.gpu.is_none() && !sampler.gpu_tried && sampler.drives_at.is_none());
    }
}
impl Sampler {
    pub fn sample(&mut self, settings: &Settings) -> Sample {
        let now = Instant::now();
        let elapsed = self.at.map(|at| now.duration_since(at).as_secs_f64());
        let resumed = elapsed.is_some_and(|s| s > settings.interval.clamp(1, 5) as f64 * 3. + 1.);
        if resumed {
            self.cpu = None;
            self.network.clear();
            self.processes.clear();
            self.gpu = None;
            self.gpu_tried = false;
        }
        let elapsed = if resumed { None } else { elapsed };
        let mut sample = Sample::default();
        unsafe {
            let (mut idle, mut kernel, mut user) = (
                FILETIME::default(),
                FILETIME::default(),
                FILETIME::default(),
            );
            if settings.show_cpu
                && GetSystemTimes(Some(&mut idle), Some(&mut kernel), Some(&mut user)).is_ok()
            {
                let current = [ticks(idle), ticks(kernel), ticks(user)];
                sample.cpu = self.cpu.and_then(|old| cpu_percent(old, current));
                self.cpu = Some(current);
            } else {
                self.cpu = None;
            }
            let mut memory = MEMORYSTATUSEX {
                dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32,
                ..Default::default()
            };
            if settings.show_memory && GlobalMemoryStatusEx(&mut memory).is_ok() {
                sample.memory_total = Some(memory.ullTotalPhys);
                sample.memory_used = Some(memory.ullTotalPhys.saturating_sub(memory.ullAvailPhys));
            }
            if settings.show_download || settings.show_upload {
                self.network(&mut sample, elapsed);
                if !settings.show_download {
                    sample.download = None;
                }
                if !settings.show_upload {
                    sample.upload = None;
                }
            } else {
                self.network = HashMap::new();
            }
            if settings.show_gpu {
                if !self.gpu_tried {
                    self.gpu = Gpu::open();
                    self.gpu_tried = true;
                }
                sample.gpu = self.gpu.as_mut().and_then(|g| g.read());
            } else {
                self.gpu = None;
                self.gpu_tried = false;
            }
            if settings.show_storage {
                if self
                    .drives_at
                    .is_none_or(|at| at.elapsed() >= Duration::from_secs(15))
                {
                    self.drives = drives();
                    self.drives_at = Some(now);
                }
                sample.drives = self.drives.clone();
            } else {
                self.drives = Vec::new();
                self.drives_at = None;
            }
            if settings.show_processes {
                sample.processes = self.processes(elapsed);
                sample.sort_processes(settings.sort_memory);
            } else {
                self.processes = HashMap::new();
            }
        }
        self.at = Some(now);
        sample
    }
    unsafe fn network(&mut self, sample: &mut Sample, seconds: Option<f64>) {
        let mut table = std::ptr::null_mut();
        if GetIfTable2(&mut table).0 != 0 || table.is_null() {
            self.network.clear();
            return;
        }
        let rows =
            std::slice::from_raw_parts((*table).Table.as_ptr(), (*table).NumEntries as usize);
        let mut next = HashMap::new();
        let mut rates = None;
        for row in rows {
            // 实体以太网/Wi-Fi；排除回环、隧道及虚拟接口的重复计数。
            if row.OperStatus != IfOperStatusUp
                || !matches!(row.Type, 6 | 71)
                || row.InterfaceAndOperStatusFlags._bitfield & 1 == 0
            {
                continue;
            }
            let id = row.InterfaceLuid.Value;
            if let Some((old_in, old_out)) = self.network.get(&id) {
                if let Some((down, up)) = seconds.and_then(|s| {
                    Some((
                        counter_rate(*old_in, row.InOctets, s)?,
                        counter_rate(*old_out, row.OutOctets, s)?,
                    ))
                }) {
                    let (d, u) = rates.get_or_insert((0., 0.));
                    *d += down;
                    *u += up;
                }
            }
            next.insert(id, (row.InOctets, row.OutOctets));
        }
        FreeMibTable(table.cast());
        self.network = next;
        if let Some((down, up)) = rates {
            sample.download = Some(down);
            sample.upload = Some(up);
        }
    }
    unsafe fn processes(&mut self, elapsed: Option<f64>) -> Vec<Process> {
        let Ok(handle) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else {
            return vec![];
        };
        let mut entry = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        let mut next = HashMap::new();
        let mut grouped = HashMap::<String, Process>::new();
        let cores = std::thread::available_parallelism().map_or(1, |v| v.get()) as f64;
        let mut ok = Process32FirstW(handle, &mut entry).is_ok();
        while ok {
            if entry.th32ProcessID != 0 {
                if let Ok(process) = OpenProcess(
                    PROCESS_QUERY_INFORMATION | PROCESS_VM_READ,
                    false,
                    entry.th32ProcessID,
                ) {
                    let (mut start, mut end, mut kernel, mut user) = (
                        FILETIME::default(),
                        FILETIME::default(),
                        FILETIME::default(),
                        FILETIME::default(),
                    );
                    let mut memory = PROCESS_MEMORY_COUNTERS::default();
                    memory.cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
                    let read_memory = GetProcessMemoryInfo(
                        process,
                        &mut memory,
                        std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32,
                    )
                    .is_ok();
                    if GetProcessTimes(process, &mut start, &mut end, &mut kernel, &mut user)
                        .is_ok()
                        && read_memory
                    {
                        let current = ticks(kernel).saturating_add(ticks(user));
                        let key = (entry.th32ProcessID, ticks(start));
                        let cpu = self
                            .processes
                            .get(&key)
                            .and_then(|old| counter_rate(*old, current, elapsed?))
                            .map(|v| (v / 100_000. / cores).clamp(0., 100.));
                        next.insert(key, current);
                        let length = entry
                            .szExeFile
                            .iter()
                            .position(|v| *v == 0)
                            .unwrap_or(entry.szExeFile.len());
                        let name = String::from_utf16_lossy(&entry.szExeFile[..length]);
                        let group = grouped
                            .entry(name.to_lowercase())
                            .or_insert_with(|| Process {
                                name,
                                ..Default::default()
                            });
                        group.memory = group.memory.saturating_add(memory.WorkingSetSize as u64);
                        group.count += 1;
                        if let Some(cpu) = cpu {
                            group.cpu = Some(group.cpu.unwrap_or(0.) + cpu);
                        }
                    }
                    let _ = CloseHandle(process);
                }
            }
            ok = Process32NextW(handle, &mut entry).is_ok();
        }
        let _ = CloseHandle(handle);
        self.processes = next;
        grouped.into_values().collect()
    }
}
unsafe fn drives() -> Vec<Drive> {
    let mask = GetLogicalDrives();
    let mut out = vec![];
    for n in 0..26 {
        if mask & (1 << n) == 0 {
            continue;
        }
        let root = [b'A' as u16 + n as u16, b':' as u16, b'\\' as u16, 0];
        if GetDriveTypeW(PCWSTR(root.as_ptr())) != 3 {
            continue;
        }
        let (mut total, mut free) = (0, 0);
        if GetDiskFreeSpaceExW(
            PCWSTR(root.as_ptr()),
            None,
            Some(&mut total),
            Some(&mut free),
        )
        .is_ok()
            && total > 0
        {
            out.push(Drive {
                name: format!("{}:", (b'A' + n as u8) as char),
                used: total.saturating_sub(free),
                total,
            });
        }
    }
    out
}

struct Gpu {
    query: isize,
    counter: isize,
}
impl Gpu {
    unsafe fn open() -> Option<Self> {
        let mut query = 0;
        if PdhOpenQueryW(PCWSTR::null(), 0, &mut query) != 0 {
            return None;
        }
        let mut counter = 0;
        if PdhAddEnglishCounterW(
            query,
            w!("\\GPU Engine(*)\\Utilization Percentage"),
            0,
            &mut counter,
        ) != 0
        {
            PdhCloseQuery(query);
            return None;
        }
        Some(Self { query, counter })
    }
    unsafe fn read(&mut self) -> Option<f64> {
        if PdhCollectQueryData(self.query) != 0 {
            return None;
        }
        let (mut size, mut count) = (0, 0);
        if PdhGetFormattedCounterArrayW(self.counter, PDH_FMT_DOUBLE, &mut size, &mut count, None)
            != PDH_MORE_DATA
            || size == 0
            || size > 16 * 1024 * 1024
        {
            return None;
        }
        // u64 缓冲区提供足够的结构体对齐；指针和字符串在解析完成前保持有效。
        let mut buffer = vec![0u64; (size as usize + 7) / 8];
        let ptr = buffer.as_mut_ptr().cast::<PDH_FMT_COUNTERVALUE_ITEM_W>();
        if PdhGetFormattedCounterArrayW(
            self.counter,
            PDH_FMT_DOUBLE,
            &mut size,
            &mut count,
            Some(ptr),
        ) != 0
        {
            return None;
        }
        if count as usize > buffer.len() * 8 / std::mem::size_of::<PDH_FMT_COUNTERVALUE_ITEM_W>() {
            return None;
        }
        let rows = std::slice::from_raw_parts(ptr, count as usize)
            .iter()
            .filter_map(|item| {
                if item.FmtValue.CStatus > 1 {
                    return None;
                }
                Some((
                    item.szName.to_string().ok()?,
                    item.FmtValue.Anonymous.doubleValue,
                ))
            })
            .collect::<Vec<_>>();
        gpu_percent(rows.iter().map(|(name, value)| (name.as_str(), *value)))
    }
}
impl Drop for Gpu {
    fn drop(&mut self) {
        unsafe {
            PdhCloseQuery(self.query);
        }
    }
}
