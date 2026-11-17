//! Codex 本地会话的只读适配器。只解析公开消息，不读取凭据或推理正文。
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;

mod desktop;
pub mod dsh;
mod projects;
mod read_receipts;
mod reader;
mod sources;
pub use desktop::{activate as activate_desktop, available as desktop_available};
pub use desktop::{activate_for as activate_desktop_for, available_for as desktop_available_for};
pub use reader::{Reader, Request, Service, Update};
pub use read_receipts::{ReadReceipt, SessionReadReceipt};
pub use sources::{merge_updates, Sources};

pub const MAX_MESSAGES: usize = 64;
pub const MAX_TEXT: usize = 256 * 1024;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AgentState {
    #[default]
    Unknown,
    Thinking,
    Working,
    Answering,
    Waiting,
    Completed,
    Interrupted,
    Failed,
}
impl AgentState {
    pub fn active(self) -> bool {
        matches!(
            self,
            Self::Thinking | Self::Working | Self::Answering | Self::Waiting
        )
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Unknown => "等待更新",
            Self::Thinking => "思考中",
            Self::Working => "执行中",
            Self::Answering => "回答中",
            Self::Waiting => "等待确认",
            Self::Completed => "已完成",
            Self::Interrupted => "已中断",
            Self::Failed => "失败",
        }
    }
    pub fn short(self) -> &'static str {
        match self {
            Self::Thinking => "思考",
            Self::Working => "执行",
            Self::Answering => "回答",
            Self::Waiting => "待确认",
            Self::Completed => "完成",
            Self::Interrupted => "中断",
            Self::Failed => "失败",
            Self::Unknown => "等待",
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    #[default]
    Codex,
    Dsh,
}
impl Source {
    pub fn label(self) -> &'static str {
        match self {
            Self::Codex => "Codex",
            Self::Dsh => "DSH",
        }
    }
    pub fn provider(self) -> crate::quota::Provider {
        match self {
            Self::Codex => crate::quota::Provider::Codex,
            Self::Dsh => crate::quota::Provider::DeepSeek,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContextUsage {
    pub used: u64,
    pub total: u64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlanProgress {
    pub completed: u32,
    pub total: u32,
    pub current: String,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MessageKind {
    User,
    Progress,
    Answer,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Message {
    pub kind: MessageKind,
    pub text: String,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Snapshot {
    pub source: Source,
    pub id: String,
    pub path: PathBuf,
    pub project: String,
    pub title: String,
    pub model: String,
    pub state: AgentState,
    pub updated_at: i64,
    pub activity_ms: i64,
    pub started_at: Option<i64>,
    pub ended_at: Option<i64>,
    pub turn_id: String,
    pub messages: Vec<Message>,
    pub truncated: bool,
    pub subagent: bool,
    pub context: Option<ContextUsage>,
    pub plan: Option<PlanProgress>,
    pub tools: Vec<String>,
    pub read_receipt: Option<ReadReceipt>,
}
impl Snapshot {
    pub fn summary(&self) -> Self {
        Self {
            source: self.source,
            messages: Vec::new(),
            ..self.clone_without_body()
        }
    }
    fn clone_without_body(&self) -> Self {
        Self {
            source: self.source,
            id: self.id.clone(),
            path: self.path.clone(),
            project: self.project.clone(),
            title: self.title.clone(),
            model: self.model.clone(),
            state: self.state,
            updated_at: self.updated_at,
            activity_ms: self.activity_ms,
            started_at: self.started_at,
            ended_at: self.ended_at,
            turn_id: self.turn_id.clone(),
            messages: Vec::new(),
            truncated: self.truncated,
            subagent: self.subagent,
            context: self.context.clone(),
            plan: self.plan.clone(),
            tools: self.tools.clone(),
            read_receipt: self.read_receipt.clone(),
        }
    }
    pub fn answer(&self) -> Option<&str> {
        self.messages
            .iter()
            .rev()
            .find(|m| m.kind == MessageKind::Answer)
            .map(|m| m.text.as_str())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub enabled: bool,
    pub directory: String,
    pub pinned: Option<PathBuf>,
    pub quota_tab: bool,
    pub codex_enabled: bool,
    pub dsh_enabled: bool,
    pub codex_notifications: bool,
    pub dsh_notifications: bool,
    pub dsh_discovery: String,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            enabled: true,
            directory: String::new(),
            pinned: None,
            quota_tab: false,
            codex_enabled: true,
            dsh_enabled: true,
            codex_notifications: true,
            dsh_notifications: true,
            dsh_discovery: String::new(),
        }
    }
}
impl Config {
    pub fn notifications_enabled(&self, source: Source) -> bool {
        self.source_enabled(source) && match source {
            Source::Codex => self.codex_notifications,
            Source::Dsh => self.dsh_notifications,
        }
    }
    pub fn source_enabled(&self, source: Source) -> bool {
        self.enabled
            && match source {
                Source::Codex => self.codex_enabled,
                Source::Dsh => self.dsh_enabled,
            }
    }
    pub fn sources(&self) -> Sources {
        Sources {
            codex: self.codex_enabled,
            dsh: self.dsh_enabled.then(|| {
                if self.dsh_discovery.trim().is_empty() {
                    crate::model::config_dir().join("dsh-bridge.json")
                } else {
                    self.dsh_discovery.trim().into()
                }
            }),
        }
    }
    pub fn load() -> Self {
        let mut config: Self = std::fs::read(crate::model::config_dir().join("ai-sessions.json"))
            .ok()
            .filter(|v| v.len() < 64 * 1024)
            .and_then(|v| serde_json::from_slice(&v).ok())
            .unwrap_or_default();
        // 旧版统一关闭迁移为两个来源均关闭，之后只使用各自总开关。
        if !config.enabled {
            config.codex_enabled=false;config.dsh_enabled=false;config.enabled=true;
        }
        config
    }
    pub fn save(&self) {
        let root = crate::model::config_dir();
        if std::fs::create_dir_all(&root).is_ok() {
            if let Ok(bytes) = serde_json::to_vec_pretty(self) {
                let _ = crate::model::atomic_write(&root.join("ai-sessions.json"), &bytes);
            }
        }
    }
    pub fn root(&self) -> PathBuf {
        if !self.directory.trim().is_empty() {
            return PathBuf::from(self.directory.trim());
        }
        std::env::var_os("CODEX_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                std::env::var_os("USERPROFILE")
                    .or_else(|| std::env::var_os("HOME"))
                    .map(PathBuf::from)
                    .unwrap_or_default()
                    .join(".codex")
            })
    }
}

pub struct Parser {
    data: Snapshot,
    cwd: String,
    full: bool,
    before_start: bool,
    user_hash: Option<u64>,
    last_public: Option<(MessageKind, u64)>,
    bytes: usize,
    event_ms: i64,
    waiting_calls: std::collections::HashSet<String>,
    awaiting_async_input: bool,
}
impl Parser {
    pub fn new(full: bool) -> Self {
        Self {
            data: Snapshot::default(),
            cwd: String::new(),
            full,
            before_start: false,
            user_hash: None,
            last_public: None,
            bytes: 0,
            event_ms: 0,
            waiting_calls: Default::default(),
            awaiting_async_input: false,
        }
    }
    pub fn snapshot(&self) -> Snapshot {
        self.data.clone()
    }
    pub fn set_path(&mut self, path: PathBuf) {
        self.data.id = path
            .file_stem()
            .and_then(|s| s.to_str())
            .map(|s| {
                s.chars()
                    .rev()
                    .take(36)
                    .collect::<String>()
                    .chars()
                    .rev()
                    .collect()
            })
            .unwrap_or_default();
        self.data.path = path;
    }
    pub fn mark_truncated(&mut self) {
        self.data.truncated = true;
    }
    fn reset(&mut self) {
        self.waiting_calls.clear();
        self.awaiting_async_input=false;
        self.data.messages.clear();
        self.bytes = 0;
        self.user_hash = None;
        self.last_public = None;
        self.data.truncated = false;
        self.data.started_at = None;
        self.data.ended_at = None;
        self.data.turn_id.clear();
    }
    fn touch(&mut self, at: i64) {
        self.data.updated_at = self.data.updated_at.max(at);
        self.data.activity_ms = self.data.activity_ms.max(self.event_ms);
    }
    fn add(&mut self, kind: MessageKind, text: &str, at: i64) -> bool {
        let text = text.trim();
        if text.is_empty() {
            return false;
        }
        // 两种日志格式会镜像同一条消息；仅当前轮去重，不跨轮丢弃相同请求。
        if self
            .last_public
            .as_ref()
            .is_some_and(|(k, h)| *k == kind && *h == fingerprint(text))
            || self
                .data
                .messages
                .iter()
                .any(|m| m.kind == kind && m.text == text)
        {
            return false;
        }
        self.last_public = Some((kind, fingerprint(text)));
        self.touch(at);
        if !self.full {
            return true;
        }
        let clipped = clip(text, 64 * 1024);
        self.data.truncated |= clipped.len() < text.len();
        self.bytes += clipped.len();
        self.data.messages.push(Message {
            kind,
            text: clipped,
        });
        while self.data.messages.len() > MAX_MESSAGES || self.bytes > MAX_TEXT {
            // 尽量保留用户请求，移除较早的进展。
            let i = if self.data.messages.len() > 1
                && self.data.messages[0].kind == MessageKind::User
            {
                1
            } else {
                0
            };
            self.bytes -= self.data.messages.remove(i).text.len();
            self.data.truncated = true;
        }
        true
    }
    fn user(&mut self, text: &str, at: i64) {
        if injected(text) || text.trim().is_empty() {
            return;
        }
        if self.data.ended_at.is_none() && self.user_hash == Some(fingerprint(text.trim())) {
            return;
        }
        if !(self.data.ended_at.is_none()
            && self.user_hash.is_none()
            && self.data.started_at.is_some())
        {
            self.reset();
            self.before_start = true;
        }
        self.user_hash = Some(fingerprint(text.trim()));
        self.data.state = AgentState::Thinking;
        self.data.title = text
            .lines()
            .find(|s| !s.trim().is_empty())
            .map(|s| clip(s.trim(), 240))
            .unwrap_or_default();
        self.add(MessageKind::User, text, at);
    }
    fn start(&mut self, id: &str, at: i64) {
        if !id.is_empty() && self.data.turn_id == id && self.data.state.active() {
            return;
        }
        if !self.before_start {
            self.reset();
        }
        self.before_start = false;
        self.data.turn_id = id.into();
        self.data.started_at = (at > 0).then_some(at);
        self.data.state = AgentState::Thinking;
        self.touch(at);
    }
    fn assistant(&mut self, text: &str, phase: &str, at: i64) {
        if phase == "analysis" {
            return;
        }
        let kind = if phase == "commentary" {
            MessageKind::Progress
        } else {
            MessageKind::Answer
        };
        if self.add(kind, text, at) && self.data.ended_at.is_none() {
            let state = if kind == MessageKind::Answer {
                AgentState::Answering
            } else {
                AgentState::Working
            };
            self.activity(state,at);
        }
    }
    fn activity(&mut self, state: AgentState, at: i64) {
        if self.data.ended_at.is_none() {
            self.data.state = if self.waiting_calls.is_empty() && !self.awaiting_async_input {state}else{AgentState::Waiting};
            self.before_start = false;
            self.touch(at);
        }
    }
    pub fn ingest(&mut self, value: &Value) {
        let p = &value["payload"];
        let timestamp = value["timestamp"]
            .as_str()
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok());
        let at = timestamp.map(|d| d.timestamp()).unwrap_or(0);
        self.event_ms = timestamp.map(|d| d.timestamp_millis()).unwrap_or(0);
        match value["type"].as_str().unwrap_or("") {
            "session_meta" => {
                self.data.id = str_at(p, "id").to_owned();
                if self.data.id.is_empty() {
                    self.data.id = str_at(p, "session_id").into();
                }
                let cwd = str_at(p, "cwd");
                self.cwd = cwd.to_owned();
                self.data.project = cwd
                    .trim_end_matches(['\\', '/'])
                    .rsplit(['\\', '/'])
                    .next()
                    .unwrap_or("")
                    .to_owned();
                self.data.subagent = p["source"].get("subagent").is_some()
                    || p["thread_source"].get("subagent").is_some()
                    || str_at(p, "source") == "subagent";
            }
            "turn_context" => {
                if let Some(model) = p["model"].as_str() {
                    self.data.model = clip(model, 100);
                }
                if self.data.project.is_empty() {
                    self.cwd = str_at(p, "cwd").to_owned();
                    self.data.project = str_at(p, "cwd")
                        .trim_end_matches(['\\', '/'])
                        .rsplit(['\\', '/'])
                        .next()
                        .unwrap_or("")
                        .to_owned();
                }
            }
            "event_msg" => match str_at(p, "type") {
                "task_started" | "turn_started" => self.start(str_at(p, "turn_id"), at),
                "task_complete" | "turn_complete" => {
                    if let Some(text) = p["last_agent_message"].as_str() {
                        self.add(MessageKind::Answer, text, at);
                    }
                    self.data.state = if self.awaiting_async_input {AgentState::Waiting}else{AgentState::Completed};
                    self.data.ended_at = Some(at);
                    self.before_start = false;
                    self.touch(at);
                }
                "turn_aborted" | "task_aborted" | "turn_failed" | "task_failed" => {
                    self.data.state = if str_at(p, "type").ends_with("failed") {
                        AgentState::Failed
                    } else {
                        AgentState::Interrupted
                    };
                    self.data.ended_at = Some(at);
                    self.before_start = false;
                    self.touch(at);
                }
                "user_message" => self.user(str_at(p, "message"), at),
                "agent_message" => self.assistant(str_at(p, "message"), str_at(p, "phase"), at),
                "agent_reasoning" => self.activity(AgentState::Thinking, at),
                "item_started" | "item_completed" => {
                    let item = &p["item"];
                    match str_at(item, "type") {
                        "UserMessage" => self.user(&public_text(item), at),
                        "AgentMessage" => {
                            self.assistant(&public_text(item), str_at(item, "phase"), at)
                        }
                        "Reasoning" => self.activity(AgentState::Thinking, at),
                        "CommandExecution" | "McpToolCall" | "DynamicToolCall" | "FileChange"
                        | "WebSearch" => self.activity(AgentState::Working, at),
                        _ => {}
                    }
                }
                _ => {}
            },
            "response_item" => match str_at(p, "type") {
                "message" => match str_at(p, "role") {
                    "user" => self.user(&public_text(p), at),
                    "assistant" => self.assistant(
                        &public_text(p),
                        if str_at(p, "phase").is_empty() {
                            str_at(p, "channel")
                        } else {
                            str_at(p, "phase")
                        },
                        at,
                    ),
                    _ => {}
                },
                "function_call" | "custom_tool_call" => {
                    let name=str_at(p,"name").rsplit('.').next().unwrap_or("");
                    let call_id=str_at(p,"call_id");
                    if name=="request_user_input" && !call_id.is_empty() && call_id.len() <= 256
                        && self.waiting_calls.len() < MAX_MESSAGES && self.data.ended_at.is_none() {
                        self.waiting_calls.insert(call_id.to_owned());
                    }
                    if name=="request_user_input_async" && self.data.ended_at.is_none() {
                        self.awaiting_async_input=true;
                    }
                    self.activity(AgentState::Working,at);
                },
                "function_call_output" | "custom_tool_call_output" => {
                    if self.waiting_calls.remove(str_at(p,"call_id")) {
                        self.activity(AgentState::Working,at);
                    }
                },
                "reasoning" => self.activity(AgentState::Thinking, at),
                _ => {}
            },
            _ => {}
        }
    }
}
fn str_at<'a>(v: &'a Value, key: &str) -> &'a str {
    v[key].as_str().unwrap_or("")
}
fn fingerprint(text: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut h);
    h.finish()
}
pub(crate) fn clip(s: &str, limit: usize) -> String {
    let mut end = s.len().min(limit);
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    s[..end].to_owned()
}
fn public_text(v: &Value) -> String {
    fn content(v: &Value, out: &mut String) {
        match v {
            Value::String(s) => {
                if !out.is_empty() {
                    out.push('\n');
                }
                out.push_str(s);
            }
            Value::Array(a) => {
                for x in a {
                    content(x, out)
                }
            }
            Value::Object(o) => {
                if let Some(t) = o.get("text") {
                    content(t, out)
                }
            }
            _ => {}
        }
    }
    let mut out = String::new();
    if let Some(c) = v.get("content") {
        content(c, &mut out);
    } else if let Some(c) = v.get("text") {
        content(c, &mut out);
    }
    out
}
fn injected(text: &str) -> bool {
    let text = text.trim_start();
    [
        "# AGENTS.md instructions for",
        "<environment_context>",
        "<permissions instructions>",
        "<skills_instructions>",
        "<turn_aborted>",
        "<environment>",
        "# Context from my IDE",
    ]
    .iter()
    .any(|prefix| text.starts_with(prefix))
}
