//! 性能数据与有界历史。平台采样只在后台进行，未取得的读数保留为 None。
use eframe::egui::{self, Color32};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, VecDeque},
    time::{Duration, Instant},
};

#[cfg(windows)]
pub mod native;

const DEFAULT_COLORS: [&str; 6] = [
    "#68B9FF", "#B69AFF", "#67DDB0", "#63CFE8", "#FFBD78", "#F38BA8",
];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Palette {
    #[default]
    Classic,
    Theme,
    Ocean,
    Sunset,
    Custom,
}
impl Palette {
    pub const ALL: [Self; 5] = [
        Self::Classic,
        Self::Theme,
        Self::Ocean,
        Self::Sunset,
        Self::Custom,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Classic => "经典彩色",
            Self::Theme => "跟随主题",
            Self::Ocean => "海风",
            Self::Sunset => "落日",
            Self::Custom => "自定义",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    #[serde(skip)]
    pub(crate) compact: bool,
    pub interval: u32,
    pub show_processes: bool,
    pub show_storage: bool,
    pub sort_memory: bool,
    pub palette: Palette,
    pub custom_colors: [String; 6],
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            compact: false,
            interval: 1,
            show_processes: true,
            show_storage: true,
            sort_memory: false,
            palette: Palette::Classic,
            custom_colors: DEFAULT_COLORS.map(str::to_owned),
        }
    }
}
impl Settings {
    pub fn normalize(&mut self) {
        self.interval = self.interval.clamp(1, 5);
        for (color, default) in self.custom_colors.iter_mut().zip(DEFAULT_COLORS) {
            if crate::transfer::color(color).is_none() {
                *color = default.into();
            }
        }
    }
    pub fn colors(&self, appearance: &crate::appearance::Appearance) -> [Color32; 6] {
        let colors = match self.palette {
            Palette::Classic => DEFAULT_COLORS.map(str::to_owned),
            Palette::Ocean => [
                "#55BFFF", "#7BDCD4", "#8DAAFF", "#64D6F0", "#87E2B0", "#A5B9FF",
            ]
            .map(str::to_owned),
            Palette::Sunset => [
                "#FFB078", "#EF9ED3", "#FFDA85", "#C9A4FF", "#FF8E9A", "#F4C890",
            ]
            .map(str::to_owned),
            Palette::Custom => self.custom_colors.clone(),
            Palette::Theme => {
                let (_, _, accent) = appearance.colors();
                return std::array::from_fn(|i| {
                    crate::paint::mix(
                        Color32::from_rgb(accent[0], accent[1], accent[2]),
                        Color32::WHITE,
                        i as f32 * 0.1,
                    )
                });
            }
        };
        colors.map(|c| {
            let [r, g, b] = crate::transfer::color(&c).unwrap_or([160; 3]);
            Color32::from_rgb(r, g, b)
        })
    }
}

#[derive(Clone, Debug, Default)]
pub struct Drive {
    pub name: String,
    pub used: u64,
    pub total: u64,
}
#[derive(Clone, Debug, Default)]
pub struct Process {
    pub name: String,
    pub cpu: Option<f64>,
    pub memory: u64,
    pub count: usize,
}
#[derive(Clone, Debug, Default)]
pub struct Sample {
    pub cpu: Option<f64>,
    pub gpu: Option<f64>,
    pub memory_used: Option<u64>,
    pub memory_total: Option<u64>,
    pub download: Option<f64>,
    pub upload: Option<f64>,
    pub drives: Vec<Drive>,
    pub processes: Vec<Process>,
}
impl Sample {
    pub fn memory_percent(&self) -> Option<f64> {
        let total = self.memory_total.filter(|v| *v > 0)?;
        Some((self.memory_used? as f64 / total as f64 * 100.).clamp(0., 100.))
    }
    pub fn sort_processes(&mut self, memory: bool) {
        self.processes.sort_by(|a, b| {
            let order = if memory {
                b.memory.cmp(&a.memory)
            } else {
                b.cpu.unwrap_or(-1.).total_cmp(&a.cpu.unwrap_or(-1.))
            };
            order.then_with(|| a.name.cmp(&b.name))
        });
    }
}

/// GetSystemTimes 的 kernel 已包含 idle，不能再把 idle 加到总量。
pub fn cpu_percent(old: [u64; 3], new: [u64; 3]) -> Option<f64> {
    let idle = new[0].checked_sub(old[0])?;
    let total = new[1]
        .checked_sub(old[1])?
        .checked_add(new[2].checked_sub(old[2])?)?;
    if total == 0 || idle > total {
        return None;
    }
    Some((total - idle) as f64 / total as f64 * 100.)
}
pub fn counter_rate(old: u64, new: u64, seconds: f64) -> Option<f64> {
    if !seconds.is_finite() || seconds <= 0. {
        return None;
    }
    Some(new.checked_sub(old)? as f64 / seconds)
}
/// 多个进程可能共享同一 GPU 引擎。先逐引擎相加，再取最忙引擎。
pub fn gpu_percent<'a>(rows: impl IntoIterator<Item = (&'a str, f64)>) -> Option<f64> {
    let mut engines = HashMap::<&str, f64>::new();
    for (name, value) in rows {
        if !value.is_finite() || value < 0. {
            continue;
        }
        let Some((_, engine)) = name.split_once("_luid_") else {
            continue;
        };
        *engines.entry(engine).or_default() += value;
    }
    engines
        .values()
        .copied()
        .reduce(f64::max)
        .map(|v| v.clamp(0., 100.))
}

#[derive(Default)]
pub struct History {
    pub cpu: VecDeque<Option<f64>>,
    pub memory: VecDeque<Option<f64>>,
    pub gpu: VecDeque<Option<f64>>,
}
impl History {
    pub fn push(&mut self, sample: &Sample) {
        for (series, value) in [
            (&mut self.cpu, sample.cpu),
            (&mut self.memory, sample.memory_percent()),
            (&mut self.gpu, sample.gpu),
        ] {
            if series.len() >= 60 {
                series.pop_front();
            }
            series.push_back(value);
        }
    }
}

#[derive(Default)]
pub(crate) struct State {
    pub sample: Sample,
    pub history: History,
    pub received: Option<Instant>,
    #[cfg(windows)]
    worker: Option<Worker>,
    requested: Option<Instant>,
    pending: bool,
}
#[cfg(windows)]
struct Worker {
    tx: std::sync::mpsc::SyncSender<Settings>,
    rx: std::sync::mpsc::Receiver<Sample>,
}
impl State {
    pub fn update(&mut self, ctx: &egui::Context, needed: bool, settings: &Settings) {
        if !needed {
            // 断开请求和结果通道；后台完成在途采样后销毁 PDH 查询与缓存。
            *self = Self::default();
            return;
        }
        #[cfg(windows)]
        {
            if needed && self.worker.is_none() {
                let (tx, requests) = std::sync::mpsc::sync_channel::<Settings>(1);
                let (results, rx) = std::sync::mpsc::sync_channel(1);
                let context = ctx.clone();
                if crate::resources::SYSTEM_MONITOR
                    .spawn(move || {
                        let mut sampler = native::Sampler::default();
                        while let Ok(settings) = requests.recv() {
                            if results.send(sampler.sample(&settings)).is_err() {
                                break;
                            }
                            context.request_repaint_of(egui::ViewportId::ROOT);
                            context.request_repaint_of(egui::ViewportId::from_hash_of(
                                "island-settings",
                            ));
                        }
                    })
                    .is_some()
                {
                    self.worker = Some(Worker { tx, rx });
                }
            }
            if self.worker.is_none() {
                ctx.request_repaint_after(Duration::from_millis(100));
            }
            if let Some(worker) = &self.worker {
                while let Ok(sample) = worker.rx.try_recv() {
                    self.history.push(&sample);
                    self.sample = sample;
                    self.received = Some(Instant::now());
                    self.pending = false;
                }
                let interval = Duration::from_secs(settings.interval.clamp(1, 5) as u64);
                if needed
                    && !self.pending
                    && self.requested.is_none_or(|at| at.elapsed() >= interval)
                {
                    if worker.tx.try_send(settings.clone()).is_ok() {
                        self.pending = true;
                        self.requested = Some(Instant::now());
                    }
                }
                if needed {
                    ctx.request_repaint_after(if self.pending {
                        Duration::from_millis(100)
                    } else {
                        interval.saturating_sub(
                            self.requested.map_or(Duration::ZERO, |at| at.elapsed()),
                        )
                    });
                }
            }
        }
        #[cfg(not(windows))]
        let _ = (ctx, needed, settings);
    }
}

pub fn percent(value: Option<f64>) -> String {
    value
        .filter(|v| v.is_finite())
        .map_or_else(|| "—".into(), |v| format!("{:.0}%", v.clamp(0., 100.)))
}
pub fn bytes(value: u64) -> String {
    if value >= 1 << 30 {
        format!("{:.1} GB", value as f64 / (1u64 << 30) as f64)
    } else if value >= 1 << 20 {
        format!("{:.0} MB", value as f64 / (1u64 << 20) as f64)
    } else {
        format!("{:.0} KB", value as f64 / 1024.)
    }
}
pub fn rate(value: Option<f64>) -> String {
    value.map_or_else(
        || "—".into(),
        |v| {
            if v >= 1_048_576. {
                format!("{:.1} MB/s", v / 1_048_576.)
            } else {
                format!("{:.0} KB/s", v / 1024.)
            }
        },
    )
}

#[cfg(test)]
mod lifecycle_tests {
    use super::*;
    #[test]
    fn inactive_monitor_releases_samples_history_and_worker_channels() {
        let mut state = State::default();
        state.sample.cpu = Some(50.);
        state.sample.processes.push(Process {
            name: "sample".into(),
            ..Default::default()
        });
        state.history.push(&state.sample);
        state.received = Some(Instant::now());
        state.pending = true;
        #[cfg(windows)]
        let requests = {
            let (tx, requests) = std::sync::mpsc::sync_channel(1);
            let (_results, rx) = std::sync::mpsc::sync_channel(1);
            state.worker = Some(Worker { tx, rx });
            requests
        };
        state.update(&egui::Context::default(), false, &Settings::default());
        assert!(state.sample.cpu.is_none() && state.sample.processes.is_empty());
        assert_eq!(state.history.cpu.capacity(), 0);
        assert!(state.received.is_none() && !state.pending);
        #[cfg(windows)]
        {
            assert!(state.worker.is_none());
            assert!(matches!(
                requests.try_recv(),
                Err(std::sync::mpsc::TryRecvError::Disconnected)
            ));
        }
    }
}
