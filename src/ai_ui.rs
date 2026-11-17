//! AI 会话与额度页面：胶囊只需要元数据，正文仅在可见时保留。
use crate::{
    agent_sessions::{self as agent, AgentState, MessageKind, Snapshot, Source},
    app::{App, Cmd, CONTENT_W},
    paint::{self, Card, H},
    theme::{self, W},
};
use eframe::egui::{self, Color32, Pos2, Vec2};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

const AGENT_ICON_WIDTH: f32 = 27.;
const AGENT_SOURCE_GAP: f32 = 8.;
const AGENT_PROJECT_INSET: f32 = 31.;
const AGENT_PROJECT_PADDING: f32 = AGENT_PROJECT_INSET + 4.;

struct Block {
    kind: MessageKind,
    top: f32,
    galley: Arc<egui::Galley>,
}
#[derive(Default)]
struct Body {
    blocks: Vec<Block>,
    height: f32,
}
struct Finished {
    snapshot: Snapshot,
    until: f64,
}
impl Finished {
    fn matches(&self, s: &Snapshot) -> bool {
        self.snapshot.source == s.source
            && self.snapshot.id == s.id
            && self.snapshot.turn_id == s.turn_id
    }
}
pub(crate) struct State {
    pub config: agent::Config,
    pub snapshot: Option<Snapshot>,
    pub recent: Vec<Snapshot>,
    pub latest: Option<std::path::PathBuf>,
    pub error: Option<String>,
    dsh_status: String,
    service: Option<agent::Service>,
    hold: Option<std::path::PathBuf>,
    finished: Vec<Finished>,
    notifications: Vec<Snapshot>,
    waiting_notices: Vec<(Source, String, String)>,
    read_receipts: Vec<agent::SessionReadReceipt>,
    observing: bool,
    body: Body,
    scroll: f32,
    follow_bottom: bool,
    new_messages: bool,
    picker: bool,
    picker_first: usize,
    desktop: bool,
    desktop_at: Option<Instant>,
    pub(crate) preview: usize,
    pub(crate) preview_source: Source,
}
impl Default for State {
    fn default() -> Self {
        Self {
            config: Default::default(),
            snapshot: None,
            recent: Vec::new(),
            latest: None,
            error: None,
            dsh_status: String::new(),
            service: None,
            hold: None,
            finished: Vec::new(),
            notifications: Vec::new(),
            waiting_notices: Vec::new(),
            read_receipts: Vec::new(),
            observing: false,
            body: Body::default(),
            scroll: 0.,
            follow_bottom: true,
            new_messages: false,
            picker: false,
            picker_first: 0,
            desktop: false,
            desktop_at: None,
            preview: 0,
            preview_source: Source::Codex,
        }
    }
}
impl State {
    pub fn load() -> Self {
        let mut state = Self {
            config: agent::Config::load(),
            ..Default::default()
        };
        if state.config.sources().dsh.is_some_and(|p| p.exists()) {
            state.preview_source = Source::Dsh;
        }
        state
    }
    pub fn session_chip(&self, t: f64) -> bool {
        !self.capsule_sessions(t).is_empty()
    }
    fn sessions(&self) -> impl Iterator<Item = &Snapshot> {
        self.recent.iter().chain(self.snapshot.iter())
    }
    fn capsule_candidates(&self, t: f64) -> impl Iterator<Item = &Snapshot> {
        self.sessions().chain(
            self.finished
                .iter()
                .filter(move |f| f.until > t)
                .map(|f| &f.snapshot),
        )
    }
    fn capsule_session_visible(&self, s: &Snapshot, t: f64) -> bool {
        !s.subagent
            && (display_state(s).active()
                || self.finished.iter().any(|f| f.until > t && f.matches(s)))
    }
    fn capsule_sessions(&self, t: f64) -> Vec<&Snapshot> {
        [Source::Codex, Source::Dsh]
            .into_iter()
            .filter_map(|source| {
                if !self.config.source_enabled(source) {
                    return None;
                }
                // 完成提示先显示六秒，再恢复同一来源中仍在运行的任务。
                if let Some(finished) = self
                    .finished
                    .iter()
                    .filter(|f| f.until > t && f.snapshot.source == source)
                    .max_by(|a, b| {
                        a.until
                            .total_cmp(&b.until)
                            .then_with(|| a.snapshot.activity_ms.cmp(&b.snapshot.activity_ms))
                    })
                {
                    return Some(&finished.snapshot);
                }
                self.sessions()
                    .filter(|s| s.source == source && !s.subagent && display_state(s).active())
                    .max_by_key(|s| s.activity_ms)
            })
            .collect()
    }
    fn capsule_source_widths(&self, t: f64, measure: impl Fn(&str) -> f32) -> Vec<f32> {
        self.capsule_sources()
            .into_iter()
            .map(|source| {
                // 同一来源只展示最近的任务，但为仍在运行的最长项目名留位。
                self.capsule_candidates(t)
                    .filter(|s| s.source == source && self.capsule_session_visible(s, t))
                    .filter(|s| !s.project.is_empty())
                    .map(|s| AGENT_PROJECT_PADDING + measure(&s.project).ceil())
                    .fold(AGENT_ICON_WIDTH, f32::max)
            })
            .collect()
    }
    pub(crate) fn capsule_width(&self, ctx: &egui::Context, t: f64) -> f32 {
        let widths = ctx.fonts(|fonts| {
            self.capsule_source_widths(t, |project| {
                fonts
                    .layout_no_wrap(project.to_owned(), theme::font(10., W::Semibold), theme::WHITE)
                    .size()
                    .x
            })
        });
        widths.iter().sum::<f32>()
            + widths.len().saturating_sub(1) as f32 * AGENT_SOURCE_GAP
    }
    pub(crate) fn capsule_min_width(&self) -> f32 {
        let count = self.capsule_sources().len();
        count as f32 * AGENT_ICON_WIDTH
            + count.saturating_sub(1) as f32 * AGENT_SOURCE_GAP
    }
    fn capsule_sources(&self) -> Vec<Source> {
        [Source::Codex, Source::Dsh]
            .into_iter()
            .filter(|s| self.config.source_enabled(*s))
            .collect()
    }
    pub(crate) fn capsule_height(&self, _t: f64) -> f32 {
        self.capsule_sources().len() as f32 * 32.
    }
    pub fn release_body(&mut self) {
        if let Some(s) = &mut self.snapshot {
            s.messages = Vec::new();
        }
        self.body = Body::default();
        self.scroll = 0.;
        self.follow_bottom = true;
        self.new_messages = false;
        self.picker = false;
    }
    pub fn receive(&mut self, update: agent::Update, t: f64) {
        self.read_receipts=update.read_receipts;
        let terminal = |s: &Snapshot| {
            matches!(
                s.state,
                AgentState::Completed | AgentState::Interrupted | AgentState::Failed
            )
        };
        // 摘要与正文可能跨过新一轮开始的时刻，同一会话只采用较新的快照。
        let mut incoming: Vec<&Snapshot> = Vec::new();
        for snapshot in update.recent.iter().chain(update.selected.iter()) {
            if let Some(previous) = incoming.iter_mut()
                .find(|s| s.source == snapshot.source && s.id == snapshot.id)
            {
                if snapshot.activity_ms >= previous.activity_ms {
                    *previous = snapshot;
                }
            } else {
                incoming.push(snapshot);
            }
        }
        let newly_finished: Vec<_> = if self.observing {
            incoming.iter().copied()
                .filter(|new| {
                    !new.subagent
                        && terminal(new)
                        && self
                            .sessions()
                            .filter(|old| old.source == new.source && old.id == new.id)
                            .max_by_key(|old| old.activity_ms)
                            .is_some_and(|old| {
                                (old.turn_id == new.turn_id && old.state.active())
                                    || (new.activity_ms > old.activity_ms
                                        && new.ended_at.is_some()
                                        && new.ended_at != old.ended_at)
                            })
                })
                .map(|s| Finished {
                    snapshot: s.summary(),
                    until: t + 6.,
                })
                .collect()
        } else {
            Vec::new()
        };
        // 只从已观察到的状态变化生成提醒，首次加载不回放历史完成。
        let mut notifications: Vec<_> = newly_finished.iter()
            .filter(|f| f.snapshot.state == AgentState::Completed)
            .map(|f| f.snapshot.summary())
            .collect();
        if self.observing {
            notifications.extend(incoming.iter().copied().filter(|new| {
                !new.subagent && new.state == AgentState::Waiting
                    && !self.waiting_notices.iter().any(|(source,id,turn)|
                        *source == new.source && *id == new.id && *turn == new.turn_id)
                    && self.sessions()
                        .filter(|old| old.source == new.source && old.id == new.id)
                        .max_by_key(|old| old.activity_ms)
                        .is_none_or(|old| new.activity_ms >= old.activity_ms
                            && (old.state != AgentState::Waiting || old.turn_id != new.turn_id))
            }).map(Snapshot::summary));
        }
        // 列表短暂缺项不代表用户已答复；直到同一会话恢复执行才允许再次提醒。
        self.waiting_notices.retain(|(source,id,turn)| !incoming.iter().any(|s|
            s.source == *source && s.id == *id && (s.state != AgentState::Waiting || s.turn_id != *turn)));
        for s in incoming.iter().filter(|s| !s.subagent && s.state == AgentState::Waiting) {
            let key=(s.source,s.id.clone(),s.turn_id.clone());
            if !self.waiting_notices.contains(&key) {self.waiting_notices.push(key);}
        }
        if self.waiting_notices.len() > 64 {
            self.waiting_notices.drain(..self.waiting_notices.len() - 64);
        }
        self.notifications.extend(notifications);
        if self.notifications.len() > 8 {
            self.notifications.drain(..self.notifications.len() - 8);
        }
        self.finished.retain(|f| {
            f.until > t
                && !incoming.iter().any(|s| {
                    f.snapshot.source == s.source
                        && f.snapshot.id == s.id
                        && (!f.matches(s) || s.state.active())
                })
        });
        for finished in newly_finished {
            if !self.finished.iter().any(|f| f.matches(&finished.snapshot)) {
                self.finished.push(finished);
            }
        }
        let old = self.snapshot.as_ref();
        let new = update.selected.as_ref();
        if let (Some(old), Some(new)) = (old, new) {
            if old.source != new.source || old.id != new.id {
                self.desktop_at = None;
            }
            if old.path != new.path || old.turn_id != new.turn_id {
                self.body = Body::default();
                self.scroll = 0.;
                self.follow_bottom = true;
                self.new_messages = false;
            } else if old.messages != new.messages && !self.follow_bottom {
                self.new_messages = true;
            }
        }
        if update.selected.is_some() {
            self.snapshot = update.selected;
        } else if update.error.is_none() {
            self.release_body();
            self.snapshot = None;
        }
        self.recent = update.recent;
        self.latest = update.latest;
        self.error = update.error;
        self.dsh_status = update.dsh_status;
        self.observing = true;
    }
    pub(crate) fn take_notifications(&mut self) -> Vec<Snapshot> {
        std::mem::take(&mut self.notifications)
    }
    pub(crate) fn notification_current(&self, source: Source, path: &std::path::Path, turn: &str, state: AgentState) -> bool {
        self.sessions().filter(|s| s.source == source && s.path == path)
            .max_by_key(|s| s.activity_ms)
            .is_none_or(|s| s.turn_id == turn && s.state == state)
    }
    fn read_receipt_for(&self, notice:&Snapshot)->Option<&agent::ReadReceipt> {
        self.read_receipts.iter().find(|r|r.source==notice.source&&r.id==notice.id)
            .map(|r|&r.receipt)
            .or_else(||self.sessions().filter(|s|s.source==notice.source&&s.id==notice.id).max_by_key(|s|s.activity_ms).and_then(|s|s.read_receipt.as_ref()))
    }
    pub(crate) fn observe_notification_unread(&self, notice:&mut Snapshot) {
        if let Some(read)=self.read_receipt_for(notice).filter(|read|read.ui_unread==Some(true)) {
            if notice.read_receipt.as_ref().is_none_or(|old|old.ui_context.is_empty()||old.ui_context==read.ui_context) {
                notice.read_receipt=Some(read.clone());
            }
        }
    }
    pub(crate) fn notification_read(&self, notice:&Snapshot)->bool {
        self.read_receipt_for(notice)
            .is_some_and(|read|read.covers(&notice.turn_id,notice.activity_ms,notice.read_receipt.as_ref()))
    }
    fn retain_enabled_sources(&mut self) {
        self.read_receipts.retain(|r|self.config.source_enabled(r.source));
        self.recent.retain(|s|self.config.source_enabled(s.source));
        self.finished.retain(|f|self.config.source_enabled(f.snapshot.source));
        self.notifications.retain(|s|self.config.source_enabled(s.source));
        self.waiting_notices.retain(|(source,_,_)|self.config.source_enabled(*source));
        if self.snapshot.as_ref().is_some_and(|s|!self.config.source_enabled(s.source)) {
            self.release_body();self.snapshot=None;self.hold=None;self.desktop_at=None;
        }
    }
}
fn status_color(state: AgentState) -> Color32 {
    match state {
        AgentState::Thinking => Color32::from_rgb(177, 143, 255),
        AgentState::Working | AgentState::Answering => Color32::from_rgb(104, 172, 255),
        AgentState::Completed => Color32::from_rgb(86, 212, 155),
        AgentState::Interrupted => Color32::from_rgb(241, 184, 91),
        AgentState::Waiting => Color32::from_rgb(241, 184, 91),
        AgentState::Failed => Color32::from_rgb(245, 115, 115),
        AgentState::Unknown => theme::GRAY,
    }
}
fn display_state(s: &Snapshot) -> AgentState {
    if s.state.active()
        && s.state != AgentState::Waiting
        && chrono::Utc::now().timestamp().saturating_sub(s.updated_at) > 120
    {
        AgentState::Unknown
    } else {
        s.state
    }
}
pub(crate) fn draw_agent_symbol(c: &Card, source: Source, state: AgentState, x: f32, y: f32, t: f64) {
    if state == AgentState::Unknown {
        crate::quota_ui::monochrome_logo(c, source.provider(), x, y, 17., Color32::from_gray(166));
        return;
    }
    crate::quota_ui::logo(c, source.provider(), x, y, 17.);
    let color = status_color(state);
    match state {
        AgentState::Thinking => {
            // 思考：柔和呼吸环与两颗缓慢绕行的小点。
            let phase = (t * 1.8) as f32;
            c.ring(
                x,
                y,
                10.8,
                1.,
                color.gamma_multiply(0.18 + 0.12 * phase.sin().abs()),
            );
            for offset in [0., std::f32::consts::PI] {
                let angle = phase + offset;
                c.circle(x + angle.cos() * 10.8, y + angle.sin() * 10.8, 1.35, color);
            }
            c.ui.ctx().request_repaint_after(Duration::from_millis(33));
        }
        AgentState::Working | AgentState::Answering => {
            // 执行：方向明确的旋转弧，仅表达活动，不暗示任务进度百分比。
            let phase = (t * 4.) as f32;
            for i in 0..14 {
                let a = phase + i as f32 * 0.16;
                let b = a + 0.18;
                c.seg(
                    (x + a.cos() * 10.8, y + a.sin() * 10.8),
                    (x + b.cos() * 10.8, y + b.sin() * 10.8),
                    1.5,
                    color.gamma_multiply(0.2 + i as f32 / 18.),
                );
            }
            c.ui.ctx().request_repaint_after(Duration::from_millis(33));
        }
        AgentState::Completed
        | AgentState::Interrupted
        | AgentState::Failed
        | AgentState::Waiting => {
            let (bx, by) = (x + 7., y + 7.);
            c.circle(bx, by, 5.4, Color32::from_gray(22));
            c.circle(bx, by, 4.5, color);
            let ink = Color32::from_gray(22);
            match state {
                AgentState::Completed => c.path(
                    &[(bx - 2.4, by), (bx - 0.6, by + 1.8), (bx + 2.7, by - 1.8)],
                    1.25,
                    ink,
                ),
                AgentState::Failed => {
                    c.seg((bx - 1.7, by - 1.7), (bx + 1.7, by + 1.7), 1.2, ink);
                    c.seg((bx - 1.7, by + 1.7), (bx + 1.7, by - 1.7), 1.2, ink);
                }
                AgentState::Interrupted => {
                    for dx in [-1.5, 1.5] {
                        c.seg((bx + dx, by - 2.), (bx + dx, by + 2.), 1.2, ink);
                    }
                }
                AgentState::Waiting => {
                    c.path(&[(bx, by - 2.6), (bx, by), (bx + 2.2, by)], 1.15, ink)
                }
                _ => {}
            }
        }
        AgentState::Unknown => {}
    }
}
fn elapsed(s: &Snapshot) -> String {
    s.started_at
        .map(|start| {
            let secs = s
                .ended_at
                .unwrap_or_else(|| chrono::Utc::now().timestamp())
                .saturating_sub(start)
                .max(0);
            format!("{}:{:02}", secs / 60, secs % 60)
        })
        .unwrap_or_default()
}

impl App {
    pub fn open_ai_settings(&mut self) {
        self.settings_tab = 11;
        self.open_settings();
    }
    pub fn agent_demand(&self, visible: bool) -> Option<bool> {
        if !visible
            || !self.ai.config.enabled
            || (!self.ai.config.codex_enabled && !self.ai.config.dsh_enabled)
        {
            return None;
        }
        let full = !self.ai.config.quota_tab
            && (self.expanded_page_visible(6)
                || (self.settings_open && [3, 14].contains(&self.settings_tab) && self.page == 6));
        if full {
            Some(true)
        } else if (self.settings.notifications && [Source::Codex,Source::Dsh].into_iter().any(|s|self.ai.config.notifications_enabled(s)))
            || (self.settings.agent_capsule && (!self.effective_minimal()
                && !self.open
                && self.t >= self.notification_bell_until))
        {
            Some(false)
        } else {
            None
        }
    }
    pub fn agent_tick(&mut self, ctx: &egui::Context, visible: bool) {
        self.ai.retain_enabled_sources();
        let Some(full) = self.agent_demand(visible) else {
            self.ai.service = None;
            self.ai.hold = None;
            self.ai.observing = false;
            self.ai.finished.clear();
            self.ai.notifications.clear();
            self.ai.waiting_notices.clear();
            self.ai.read_receipts.clear();
            self.ai.release_body();
            return;
        };
        if !full {
            self.ai.hold = None;
            self.ai.release_body();
        }
        if full && self.ai.hold.is_none() && self.ai.config.pinned.is_none() {
            self.ai.hold = self.ai.snapshot.as_ref().map(|s| s.path.clone());
        }
        let request = agent::Request {
            root: self.ai.config.root(),
            full,
            pinned: self.ai.config.pinned.clone(),
            hold: self.ai.hold.clone(),
        };
        if let Some(service) = &mut self.ai.service {
            service.configure_sources(request, self.ai.config.sources());
        } else {
            self.ai.service =
                agent::Service::start_sources(request, self.ai.config.sources(), ctx.clone());
        }
        if let Some(update) = self.ai.service.as_ref().and_then(|s| s.poll()) {
            self.ai.receive(update, self.t);
        }
        if full
            && self
                .ai
                .desktop_at
                .is_none_or(|at| at.elapsed() > Duration::from_secs(2))
        {
            self.ai.desktop = agent::desktop_available_for(
                self.ai
                    .snapshot
                    .as_ref()
                    .map(|s| s.source)
                    .unwrap_or_default(),
            );
            self.ai.desktop_at = Some(Instant::now());
        }
        if let Some(until) = self
            .ai
            .finished
            .iter()
            .map(|f| f.until)
            .filter(|until| *until > self.t)
            .min_by(f64::total_cmp)
        {
            ctx.request_repaint_after(Duration::from_secs_f64((until - self.t).min(1.0).max(0.05)));
        }
    }
    pub fn agent_set_directory(&mut self, path: std::path::PathBuf) {
        self.ai.config.directory = path.to_string_lossy().into_owned();
        self.ai.config.pinned = None;
        self.ai.hold = None;
        self.ai.snapshot = None;
        self.ai.config.save();
    }
    pub(crate) fn ai_capsule_chip(&self) -> bool {
        self.settings.agent_capsule
            && self.ai.config.enabled
            && (self.ai.config.codex_enabled || self.ai.config.dsh_enabled)
            && !self.effective_minimal()
            && self.t >= self.notification_bell_until
    }
    pub(crate) fn draw_ai_bar(&mut self, c: &Card, cx: f32, layout: &crate::bar_layout::Layout) {
        self.draw_quota_bar(c, cx, layout);
        if !self.ai_capsule_chip() {
            return;
        }
        if let Some(x) = layout.x[crate::bar_layout::AGENT] {
            self.draw_ai_chip(
                c,
                cx + x,
                15.,
                layout.widths[crate::bar_layout::AGENT],
                false,
            );
        }
    }
    pub(crate) fn draw_ai_chip(&mut self, c: &Card, x: f32, y: f32, width: f32, side: bool) {
        let sources = self.ai.capsule_sources();
        let sessions: Vec<_> = self
            .ai
            .capsule_sessions(self.t)
            .into_iter()
            .map(Snapshot::summary)
            .collect();
        let mut widths = if side {
            vec![AGENT_ICON_WIDTH; sources.len()]
        } else {
            self.ai.capsule_source_widths(self.t, |project| c.text_w(project, 10., W::Semibold))
        };
        let preferred = widths.iter().sum::<f32>()
            + sources.len().saturating_sub(1) as f32 * AGENT_SOURCE_GAP;
        let used_width = width.min(preferred);
        let minimum = self.ai.capsule_min_width();
        if preferred > used_width && preferred > minimum {
            let fit = ((used_width - minimum) / (preferred - minimum)).clamp(0., 1.);
            for cell in &mut widths {
                *cell = AGENT_ICON_WIDTH + (*cell - AGENT_ICON_WIDTH) * fit;
            }
        }
        let mut left = x - used_width / 2.;
        let mut chosen = None;
        let mut clicked = false;
        for (i, source) in sources.iter().copied().enumerate() {
            let session = sessions.iter().find(|s| s.source == source);
            let state = session.map(display_state).unwrap_or_default();
            let cell_width = widths[i];
            let (cx, cy) = if side {
                (
                    x,
                    y + (i as f32 - sources.len().saturating_sub(1) as f32 / 2.) * 32.,
                )
            } else {
                (left + 13.5, y)
            };
            draw_agent_symbol(c, source, state, cx, cy, self.t);
            if !side {
                if let Some(s) = session {
                    let text_width = (cell_width - AGENT_PROJECT_PADDING).max(0.);
                    let project = c.fit(&s.project, 10., W::Semibold, text_width);
                    if c.text_w(&project, 10., W::Semibold) <= text_width {
                        c.centered_text(
                            left + AGENT_PROJECT_INSET,
                            cy,
                            &project,
                            10.,
                            W::Semibold,
                            theme::WHITE,
                            H::Left,
                        );
                    }
                }
            }
            let known = session.or_else(|| {
                self.ai
                    .sessions()
                    .filter(|s| s.source == source && !s.subagent)
                    .max_by_key(|s| s.activity_ms)
            });
            let label = if session.is_some() {
                state.label()
            } else {
                "空闲"
            };
            let tip = if let Some(s) = known {
                format!(
                    "{} · {}\n{}\n{}\n{} · {}",
                    source.label(),
                    label,
                    s.project,
                    s.title,
                    if s.model.is_empty() {
                        "模型未知"
                    } else {
                        &s.model
                    },
                    elapsed(s)
                )
            } else {
                format!(
                    "{} · {}{}",
                    source.label(),
                    label,
                    self.ai
                        .error
                        .as_ref()
                        .map(|e| format!("\n{e}"))
                        .unwrap_or_default()
                )
            };
            let hit = if side {
                c.hit(
                    cx - 15.,
                    cy - 15.,
                    30.,
                    30.,
                    ("agent-chip", source.label(), side),
                )
            } else {
                c.hit(
                    left,
                    cy - 14.,
                    cell_width,
                    28.,
                    ("agent-chip", source.label(), side),
                )
            };
            if hit.on_hover_text(tip).clicked() {
                chosen = known.map(Snapshot::summary);
                clicked = true;
            }
            left += cell_width + AGENT_SOURCE_GAP;
        }
        if clicked {
            self.ai.release_body();
            if let Some(snapshot) = chosen {
                self.ai.config.pinned = None;
                self.ai.hold = Some(snapshot.path.clone());
                self.ai.snapshot = Some(snapshot);
                self.ai.desktop_at = None;
            }
            self.ai.config.quota_tab = false;
            self.ai.config.save();
            self.show_page(6);
        }
    }
    pub(crate) fn page_ai(&mut self, c: &Card) {
        for (i, label) in ["会话", "额度"].into_iter().enumerate() {
            let on = self.ai.config.quota_tab == (i == 1);
            let x = 14. + i as f32 * 51.;
            let (r, b) = paint::capsule(
                c,
                x,
                33.,
                46.,
                21.,
                &format!("ai-tab-{i}"),
                if on { theme::BTN_HOVER } else { theme::CARD },
                theme::BTN_HOVER,
            );
            b.centered_text(
                x + 23.,
                43.5,
                label,
                10.,
                W::Semibold,
                if on { theme::WHITE } else { theme::GRAY },
                H::Center,
            );
            if r.clicked() {
                self.ai.config.quota_tab = i == 1;
                self.ai.config.save();
                self.ai.picker = false;
            }
        }
        if self.ai.config.quota_tab {
            // 原额度页的账户行让出标签栏，内容和刷新保留原位置。
            self.page_quota(c);
            return;
        }
        let selector = if self.ai.config.pinned.is_some() {
            "固定会话"
        } else {
            "自动跟随"
        };
        let (r, b) = paint::capsule(
            c,
            CONTENT_W - 124.,
            33.,
            110.,
            21.,
            "ai-session-picker",
            theme::CARD,
            theme::BTN_HOVER,
        );
        b.centered_text(
            CONTENT_W - 74.,
            43.5,
            selector,
            9.5,
            W::Regular,
            theme::GRAY,
            H::Center,
        );
        b.seg(
            (CONTENT_W - 31., 42.),
            (CONTENT_W - 28., 45.),
            1.2,
            theme::GRAY,
        );
        b.seg(
            (CONTENT_W - 28., 45.),
            (CONTENT_W - 25., 42.),
            1.2,
            theme::GRAY,
        );
        if r.clicked() {
            self.ai.picker = !self.ai.picker;
        }
        if self.ai.picker {
            self.ai_session_picker(c);
            return;
        }
        let Some(snapshot) = self
            .ai
            .snapshot
            .as_ref()
            .filter(|s| self.ai.config.source_enabled(s.source))
        else {
            c.centered_text(
                CONTENT_W / 2.,
                106.,
                if !self.ai.config.enabled {
                    "会话读取已关闭"
                } else if self.ai.error.is_some() {
                    "AI 会话暂不可用"
                } else {
                    "等待 AI 会话"
                },
                13.,
                W::Semibold,
                theme::WHITE,
                H::Center,
            );
            let note = self
                .ai
                .error
                .as_deref()
                .unwrap_or("在 Codex 或 DSH 中开始任务后，这里会显示最近一轮消息");
            for (i, line) in c
                .wrap(note, 10.5, W::Regular, CONTENT_W - 64., 3)
                .iter()
                .enumerate()
            {
                c.centered_text(
                    CONTENT_W / 2.,
                    130. + i as f32 * 15.,
                    line,
                    10.5,
                    W::Regular,
                    theme::GRAY,
                    H::Center,
                );
            }
            if paint::capsule(
                c,
                CONTENT_W / 2. - 46.,
                172.,
                92.,
                23.,
                "ai-open-settings",
                theme::BTN,
                theme::BTN_HOVER,
            )
            .0
            .clicked()
            {
                self.settings_tab = 11;
                self.open_settings();
            }
            c.centered_text(
                CONTENT_W / 2.,
                183.5,
                "来源设置",
                10.,
                W::Regular,
                theme::WHITE,
                H::Center,
            );
            return;
        };
        let state = if self.ai.error.is_some() {
            AgentState::Unknown
        } else {
            display_state(snapshot)
        };
        let color = status_color(state);
        let label = state.label();
        let status_width = c.text_w(label, 9., W::Semibold) + 16.;
        c.fill(14., 61., status_width, 18., 5., color.gamma_multiply(0.14));
        c.centered_text(22., 70., label, 9., W::Semibold, color, H::Left);
        let time = elapsed(snapshot);
        c.centered_text(
            CONTENT_W - 14.,
            70.,
            &time,
            9.,
            W::Regular,
            theme::GRAY,
            H::Right,
        );
        c.centered_text(
            23. + status_width,
            70.,
            &c.fit(
                if snapshot.title.is_empty() {
                    "Agent 会话"
                } else {
                    &snapshot.title
                },
                11.,
                W::Semibold,
                CONTENT_W - status_width - 86.,
            ),
            11.,
            W::Semibold,
            theme::WHITE,
            H::Left,
        );
        let meta = format!(
            "{} · {} · {}{}",
            snapshot.project,
            snapshot.source.label(),
            if snapshot.model.is_empty() {
                "模型未知"
            } else {
                &snapshot.model
            },
            if self.ai.error.is_some() {
                " · 记录已过期"
            } else if snapshot.truncated {
                " · 内容已截断"
            } else {
                ""
            }
        );
        let mut metrics = Vec::new();
        if let Some(plan) = &snapshot.plan {
            metrics.push(format!("计划{}/{}", plan.completed, plan.total));
        }
        if let Some(context) = &snapshot.context {
            metrics.push(format!(
                "上下文{:.0}%",
                context.used as f64 / context.total as f64 * 100.
            ));
        }
        if let Some(tool) = snapshot.tools.last() {
            metrics.push(tool.clone());
        }
        let metrics = metrics.join(" · ");
        let metrics_width = if metrics.is_empty() {
            0.
        } else {
            c.text_w(&metrics, 8.5, W::Regular).min(212.) + 12.
        };
        c.centered_text(
            16.,
            89.,
            &c.fit(&meta, 9., W::Regular, CONTENT_W - 32. - metrics_width),
            9.,
            W::Regular,
            theme::GRAY,
            H::Left,
        );
        if metrics_width > 0. {
            c.centered_text(
                CONTENT_W - 16.,
                89.,
                &c.fit(&metrics, 8.5, W::Regular, metrics_width - 12.),
                8.5,
                W::Regular,
                theme::GRAY,
                H::Right,
            );
            c.hit(14., 80., CONTENT_W - 28., 17., "ai-metrics")
                .on_hover_text(format!(
                    "{meta}\n{metrics}{}",
                    snapshot
                        .plan
                        .as_ref()
                        .map(|p| format!("\n当前步骤：{}", p.current))
                        .unwrap_or_default()
                ));
        }
        if self.ai.error.is_some() {
            c.hit(14., 80., CONTENT_W - 28., 17., "ai-stale")
                .on_hover_text(self.ai.error.as_deref().unwrap_or(""));
        }
        self.ai_body(c);
        let new_session =
            self.ai.config.pinned.is_none()
                && self.ai.latest.as_ref().is_some_and(|latest| {
                    Some(latest) != self.ai.snapshot.as_ref().map(|s| &s.path)
                });
        let back_label = if new_session {
            "切换新会话"
        } else if self.ai.new_messages {
            "有新消息 ↓"
        } else {
            "回到最新"
        };
        if ai_button(c, 14., 204., 91., back_label, "ai-latest", true) {
            if new_session {
                self.ai.hold = self.ai.latest.clone();
                self.ai.release_body();
            } else {
                self.ai.follow_bottom = true;
                self.ai.new_messages = false;
            }
        }
        let answer = self
            .ai
            .snapshot
            .as_ref()
            .and_then(|s| s.answer())
            .map(str::to_owned);
        if ai_button(
            c,
            CONTENT_W - 191.,
            204.,
            82.,
            "复制答复",
            "ai-copy",
            answer.is_some(),
        ) {
            if let Some(answer) = answer {
                self.pending.push(Cmd::Copy(answer));
            }
        }
        let source = self
            .ai
            .snapshot
            .as_ref()
            .map(|s| s.source)
            .unwrap_or_default();
        if ai_button(
            c,
            CONTENT_W - 101.,
            204.,
            87.,
            &format!("返回 {}", source.label()),
            "ai-return",
            self.ai.desktop,
        ) {
            self.pending.push(Cmd::AgentReturn(source));
        }
        if !self.ai.desktop {
            c.hit(CONTENT_W - 101., 204., 87., 21., "ai-return-tip")
                .on_hover_text(format!(
                    "未找到 {} 桌面窗口，请回到原客户端或终端",
                    source.label()
                ));
        }
        if self.ai.snapshot.as_ref().is_some_and(|s| s.state.active()) {
            c.ui.ctx().request_repaint_after(Duration::from_secs(1));
        }
    }
    fn ai_body(&mut self, c: &Card) {
        let Some(s) = &self.ai.snapshot else {
            return;
        };
        // 使用 egui 自带的布局缓存，每帧重新验证 DPI 和字体图集；不能长期持有旧图集的排版。
        self.ai.body.blocks.clear();
        let mut top = 8.;
        for message in &s.messages {
            let galley = c.painter.layout(
                message.text.clone(),
                c.font(10.5, W::Regular),
                theme::WHITE,
                (CONTENT_W - 57.) * c.s,
            );
            let height = galley.size().y / c.s;
            self.ai.body.blocks.push(Block {
                kind: message.kind,
                top,
                galley,
            });
            top += 14. + height + 12.;
        }
        self.ai.body.height = top;
        let height = 100.;
        let max = (self.ai.body.height - height).max(0.);
        let rect = c.rect(14., 99., CONTENT_W - 28., height);
        if c.live
            && rect.contains(
                c.ui.ctx()
                    .input(|i| i.pointer.hover_pos())
                    .unwrap_or(Pos2::new(-1., -1.)),
            )
        {
            let delta = c.ui.ctx().input(|i| i.raw_scroll_delta.y) / c.s;
            if delta != 0. {
                if self.ai.follow_bottom {
                    self.ai.scroll = max;
                }
                self.ai.scroll = (self.ai.scroll - delta).clamp(0., max);
                self.ai.follow_bottom = self.ai.scroll >= max - 2.;
                if self.ai.follow_bottom {
                    self.ai.new_messages = false;
                }
                c.ui.ctx().input_mut(|i| {
                    i.raw_scroll_delta = Vec2::ZERO;
                    i.smooth_scroll_delta = Vec2::ZERO;
                });
            }
        }
        if self.ai.follow_bottom {
            self.ai.scroll = max;
        }
        self.ai.scroll = self.ai.scroll.min(max);
        c.fill(14., 99., CONTENT_W - 28., height, 7., theme::CARD);
        let body = c.clipped(14., 99., CONTENT_W - 28., height);
        if self.ai.body.blocks.is_empty() {
            body.centered_text(
                CONTENT_W / 2.,
                145.,
                "等待本轮公开消息…",
                10.5,
                W::Regular,
                theme::GRAY,
                H::Center,
            );
        }
        for block in &self.ai.body.blocks {
            let y = 99. + block.top - self.ai.scroll;
            let end = y + 14. + block.galley.size().y / c.s;
            if end < 99. || y > 199. {
                continue;
            }
            let (label, color) = match block.kind {
                MessageKind::User => ("你", Color32::from_rgb(175, 152, 245)),
                MessageKind::Progress => ("进展", Color32::from_rgb(115, 180, 249)),
                MessageKind::Answer => ("答复", Color32::from_rgb(101, 210, 162)),
            };
            body.fill(21., y, 2., 11., 1., color);
            body.centered_text(29., y + 5.5, label, 8.5, W::Semibold, color, H::Left);
            body.painter
                .galley(body.pos(29., y + 14.), block.galley.clone(), theme::WHITE);
        }
        if max > 0. {
            let thumb = (height * height / self.ai.body.height).max(15.);
            let y = 99. + (height - thumb) * self.ai.scroll / max;
            body.fill(CONTENT_W - 19., y, 2., thumb, 1., theme::DIM);
        }
    }
    fn ai_session_picker(&mut self, c: &Card) {
        c.fill(14., 58., CONTENT_W - 28., 143., 8., theme::CARD);
        if ai_button(
            c,
            22.,
            64.,
            CONTENT_W - 44.,
            "自动跟随最近活动会话",
            "ai-follow",
            true,
        ) {
            self.ai.config.pinned = None;
            self.ai.hold = self.ai.latest.clone();
            self.ai.config.save();
            self.ai.picker = false;
            self.ai.release_body();
            return;
        }
        let rect = c.rect(14., 89., CONTENT_W - 28., 109.);
        if c.live
            && c.ui
                .ctx()
                .input(|i| i.pointer.hover_pos().is_some_and(|p| rect.contains(p)))
        {
            let delta = c.ui.ctx().input(|i| i.raw_scroll_delta.y);
            if delta != 0. {
                self.ai.picker_first = if delta < 0. {
                    (self.ai.picker_first + 1).min(self.ai.recent.len().saturating_sub(5))
                } else {
                    self.ai.picker_first.saturating_sub(1)
                };
                c.ui.ctx().input_mut(|i| {
                    i.raw_scroll_delta = Vec2::ZERO;
                    i.smooth_scroll_delta = Vec2::ZERO;
                });
            }
        }
        let mut choose = None;
        for (i, s) in self
            .ai
            .recent
            .iter()
            .skip(self.ai.picker_first)
            .take(5)
            .enumerate()
        {
            let y = 90. + i as f32 * 21.;
            let color = status_color(display_state(s));
            c.circle(27., y + 9., 2.5, color);
            let text = format!("{} · {} · {}", s.source.label(), s.project, s.title);
            c.centered_text(
                36.,
                y + 9.,
                &c.fit(&text, 9., W::Regular, CONTENT_W - 72.),
                9.,
                W::Regular,
                theme::WHITE,
                H::Left,
            );
            if c.hit(20., y, CONTENT_W - 40., 20., ("ai-pick", &s.id))
                .on_hover_text(format!("{}\n{}", s.title, s.state.label()))
                .clicked()
            {
                choose = Some(s.path.clone());
            }
        }
        if self.ai.recent.is_empty() {
            c.centered_text(
                CONTENT_W / 2.,
                132.,
                "暂无可读取的会话",
                10.,
                W::Regular,
                theme::GRAY,
                H::Center,
            );
        }
        if let Some(path) = choose {
            self.ai.config.pinned = Some(path);
            self.ai.hold = None;
            self.ai.config.save();
            self.ai.picker = false;
            self.ai.release_body();
        }
    }
    pub(crate) fn ai_settings(&mut self, ui: &mut egui::Ui) {
        let before = self.ai.config.clone();
        crate::extras::settings_card(ui, |ui| {
            crate::extras::agent_source_row(ui, "Codex", Source::Codex, "enabled", &mut self.ai.config.codex_enabled);
            crate::extras::settings_hint(
                ui,
                "桌面版 / CLI 会话。关闭后停止读取，并隐藏 Codex 胶囊与提醒。",
            );
            ui.add_enabled_ui(self.ai.config.codex_enabled, |ui| {
                crate::extras::agent_source_row(ui, "完成与询问通知", Source::Codex, "notifications", &mut self.ai.config.codex_notifications);
                crate::extras::settings_hint(ui, "先弹出通知，随后在最右侧保留提醒图标，点击查看后清除。");
                ui.label(
                    egui::RichText::new("Codex 数据目录")
                        .size(11.)
                        .color(ui.visuals().weak_text_color()),
                );
                ui.horizontal(|ui| {
                    ui.add_sized(
                        [(ui.available_width() - 66.).max(100.), 26.],
                        egui::TextEdit::singleline(&mut self.ai.config.directory)
                            .hint_text("自动使用 CODEX_HOME 或用户 .codex 目录"),
                    );
                    if ui.button("选择…").clicked() {
                        self.pending.push(Cmd::AgentChooseDirectory);
                    }
                });
                ui.horizontal(|ui| {
                    if ui.small_button("使用默认目录").clicked() {
                        self.ai.config.directory.clear();
                        self.ai.config.pinned = None;
                        self.ai.hold = None;
                        self.ai.snapshot = None;
                    }
                    if ui.small_button("自动跟随").clicked() {
                        self.ai.config.pinned = None;
                        self.ai.hold = None;
                    }
                    if ui.small_button("重新读取").clicked() {
                        self.ai.service = None;
                        self.ai.error = None;
                    }
                });
                ui.label(
                    egui::RichText::new("每个来源显示最近的活跃会话；会话正文只在展开时加载。")
                        .size(11.)
                        .color(ui.visuals().weak_text_color()),
                );
            });
        });
        crate::extras::settings_card(ui, |ui| {
            crate::extras::agent_source_row(ui, "DSH", Source::Dsh, "enabled", &mut self.ai.config.dsh_enabled);
            crate::extras::settings_hint(ui, "桌面版会话。关闭后停止读取，并隐藏 DSH 胶囊与提醒。");
            ui.add_enabled_ui(self.ai.config.dsh_enabled, |ui| {
                crate::extras::agent_source_row(ui, "完成与询问通知", Source::Dsh, "notifications", &mut self.ai.config.dsh_notifications);
                crate::extras::settings_hint(ui, "先弹出通知，随后在最右侧保留提醒图标，点击查看后清除。");
                ui.label(egui::RichText::new("只读事件桥接").size(12.).strong());
                ui.add_sized(
                    [ui.available_width(), 26.],
                    egui::TextEdit::singleline(&mut self.ai.config.dsh_discovery)
                        .hint_text("默认：本机 DSH 桥接发现文件"),
                );
                let installed = self.ai.config.sources().dsh.is_some_and(|p| p.exists());
                ui.label(
                    egui::RichText::new(if !self.ai.config.dsh_enabled {
                        "DSH 会话读取已关闭"
                    } else if !self.ai.dsh_status.is_empty() {
                        self.ai.dsh_status.as_str()
                    } else if installed {
                        "桥接插件已启动，显示会话时连接"
                    } else {
                        "等待桥接插件，首次使用请运行安装脚本"
                    })
                    .size(11.)
                    .color(ui.visuals().weak_text_color()),
                );
                if ui.small_button("打开桥接安装目录").clicked() {
                    let bundled = std::env::current_exe()
                        .ok()
                        .and_then(|p| p.parent().map(|p| p.join("dsh-bridge")))
                        .filter(|p| p.is_dir());
                    self.pending.push(Cmd::Open(bundled.unwrap_or_else(|| {
                        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                            .join("plugins/dsh-island-bridge")
                    })));
                }
            });
        });
        if before.directory != self.ai.config.directory {
            self.ai.config.pinned = None;
            self.ai.hold = None;
            self.ai.snapshot = None;
            self.ai.release_body();
        }
        if before != self.ai.config {
            self.ai.retain_enabled_sources();
            if self.ai.config.pinned.as_ref().is_some_and(|p| {
                !self
                    .ai
                    .config
                    .source_enabled(if agent::dsh::session_id(p).is_some() {
                        Source::Dsh
                    } else {
                        Source::Codex
                    })
            }) {
                self.ai.config.pinned = None;
                self.ai.hold = None;
            }
            self.ai.config.save();
            self.tick_alerts();
        }
        ui.add_space(12.);
        crate::extras::settings_label(ui, "账户与额度");
        self.quota_settings(ui);
    }
    pub(crate) fn ai_settings_preview(&mut self, ui: &mut egui::Ui) {
        if !self.ai.config.source_enabled(self.ai.preview_source) {
            let Some(source)=[Source::Codex,Source::Dsh].into_iter().find(|s|self.ai.config.source_enabled(*s)) else {
                crate::extras::settings_hint(ui,"开启 Codex 或 DSH 后可预览状态显示。");return;
            };
            self.ai.preview_source=source;
        }
        crate::extras::settings_label(ui, "显示预览 · 示例数据");
        ui.horizontal_wrapped(|ui| {
            for source in [Source::Codex,Source::Dsh] {if self.ai.config.source_enabled(source) {
                ui.selectable_value(&mut self.ai.preview_source,source,source.label());
            }}
            for (i, label) in ["运行", "完成", "空闲", "异常", "待确认", "同时运行", "思考"]
                .into_iter()
                .enumerate()
            {
                ui.selectable_value(&mut self.ai.preview, i, label);
            }
        });
        self.ai_demo_preview(ui);
    }
    pub(crate) fn ai_demo_preview(&mut self, ui: &mut egui::Ui) {
        let preview = self.ai.preview;
        let source = self.ai.preview_source;
        let state = match preview {
            1 => AgentState::Completed,
            2 | 3 => AgentState::Unknown,
            4 => AgentState::Waiting,
            6 => AgentState::Thinking,
            _ => AgentState::Working,
        };
        let mut demo = State::default();
        demo.config=self.ai.config.clone();
        demo.snapshot = Some(Snapshot {
            source,
            id: "preview".into(),
            project: "示例项目".into(),
            title: "检查资源占用并优化后台任务".into(),
            model: if source == Source::Dsh {
                "DeepSeek · 模型名称"
            } else {
                "Codex · 模型名称"
            }
            .into(),
            state,
            updated_at: chrono::Utc::now().timestamp(),
            context: (source == Source::Dsh).then_some(agent::ContextUsage {
                used: 24000,
                total: 100000,
            }),
            plan: (source == Source::Dsh).then_some(agent::PlanProgress {
                completed: 2,
                total: 4,
                current: "优化后台资源".into(),
            }),
            messages: vec![
                agent::Message {
                    kind: MessageKind::User,
                    text: "检查内存，没有打开的组件应释放资源。".into(),
                },
                agent::Message {
                    kind: MessageKind::Progress,
                    text: "正在检查后台任务与资源生命周期。".into(),
                },
                agent::Message {
                    kind: MessageKind::Answer,
                    text: "已将会话正文改为展开后加载，关闭后释放缓存。".into(),
                },
            ],
            ..Default::default()
        });
        if preview == 1 {
            demo.finished.push(Finished {
                snapshot: demo.snapshot.as_ref().unwrap().summary(),
                until: self.t + 6.,
            });
        }
        if preview == 5 {
            let mut other = demo.snapshot.as_ref().unwrap().summary();
            other.source = if source == Source::Codex {
                Source::Dsh
            } else {
                Source::Codex
            };
            other.id = "preview-other".into();
            other.project = "另一个项目".into();
            other.state = AgentState::Thinking;
            demo.recent = vec![other];
        }
        if preview == 3 {
            demo.error = Some("示例：来源目录暂不可读".into());
            demo.snapshot = None;
        }
        let original = std::mem::replace(&mut self.ai, demo);
        let (rect, _) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), 40.), egui::Sense::hover());
        ui.painter()
            .rect_filled(rect, 8., Color32::from_rgb(19, 25, 36));
        let card = Card::new(ui, Pos2::new(rect.center().x, rect.top() + 20.), rect).ghost();
        self.draw_ai_chip(
            &card,
            0.,
            0.,
            rect.width() - 24.,
            false,
        );
        self.feature_page_preview(ui, 6);
        self.ai = original;
    }
}
fn ai_button(c: &Card, x: f32, y: f32, width: f32, label: &str, id: &str, enabled: bool) -> bool {
    let button = if enabled {
        c.clone()
    } else {
        c.faded(0.38).ghost()
    };
    let (r, b) = paint::capsule(&button, x, y, width, 21., id, theme::BTN, theme::BTN_HOVER);
    b.centered_text(
        x + width / 2.,
        y + 10.5,
        label,
        9.,
        W::Regular,
        theme::WHITE,
        H::Center,
    );
    enabled && r.clicked()
}
