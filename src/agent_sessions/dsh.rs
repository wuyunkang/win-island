//! 与 DSH 只读桥接通信。只接受带临时令牌的回环地址，不读取 DSH 登录凭据。
use super::{
    clip, AgentState, ContextUsage, Message, MessageKind, PlanProgress, Snapshot, Source, Update,
    MAX_MESSAGES, MAX_TEXT,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    fs::File,
    io::Read,
    path::{Path, PathBuf},
    time::Duration,
};

pub fn session_path(id: &str) -> PathBuf {
    PathBuf::from("@dsh").join(id)
}
pub fn session_id(path: &Path) -> Option<&str> {
    let id = path.strip_prefix("@dsh").ok()?.to_str()?;
    valid_id(id).then_some(id)
}
fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"_.-".contains(&c))
}
pub fn parse_snapshot(raw: &Value, full: bool) -> Result<Snapshot, String> {
    if raw["subagent"].as_bool() == Some(true) {
        return Err("忽略 DSH 子代理会话".into());
    }
    let id = raw["id"]
        .as_str()
        .filter(|id| valid_id(id))
        .ok_or("DSH 会话标识无效")?;
    let str_field = |key: &str, limit| clip(raw[key].as_str().unwrap_or(""), limit);
    let state = match raw["state"].as_str().unwrap_or("") {
        "thinking" => AgentState::Thinking,
        "working" => AgentState::Working,
        "answering" => AgentState::Answering,
        "waiting" => AgentState::Waiting,
        "completed" => AgentState::Completed,
        "interrupted" => AgentState::Interrupted,
        "failed" => AgentState::Failed,
        _ => AgentState::Unknown,
    };
    let at = raw["updatedAt"].as_i64().filter(|n| *n >= 0).unwrap_or(0);
    let cwd = str_field("cwd", 2048);
    let project = cwd
        .trim_end_matches(['\\', '/'])
        .rsplit(['\\', '/'])
        .next()
        .unwrap_or("")
        .to_owned();
    let mut s = Snapshot {
        source: Source::Dsh,
        id: format!("dsh:{id}"),
        path: session_path(id),
        project,
        title: str_field("title", 240),
        model: str_field("model", 100),
        state,
        activity_ms: at,
        updated_at: at / 1000,
        started_at: raw["startedAt"]
            .as_i64()
            .filter(|n| *n >= 0)
            .map(|n| n / 1000),
        ended_at: raw["endedAt"]
            .as_i64()
            .filter(|n| *n >= 0)
            .map(|n| n / 1000),
        turn_id: str_field("turnId", 128),
        truncated: raw["truncated"].as_bool().unwrap_or(false),
        ..Default::default()
    };
    if let (Some(used), Some(total)) = (
        raw["context"]["used"].as_u64(),
        raw["context"]["total"].as_u64().filter(|n| *n > 0),
    ) {
        s.context = Some(ContextUsage { used, total });
    }
    if let (Some(completed), Some(total)) = (
        raw["plan"]["completed"].as_u64(),
        raw["plan"]["total"]
            .as_u64()
            .filter(|n| *n > 0 && *n <= 1000),
    ) {
        if completed <= total {
            s.plan = Some(PlanProgress {
                completed: completed as u32,
                total: total as u32,
                current: clip(raw["plan"]["current"].as_str().unwrap_or(""), 160),
            });
        }
    }
    if let Some(tools) = raw["tools"].as_array() {
        s.tools = tools
            .iter()
            .rev()
            .take(3)
            .filter_map(|t| t.as_str())
            .map(|t| clip(t, 80))
            .collect();
        s.tools.reverse();
    }
    if full {
        let mut bytes = 0;
        if let Some(messages) = raw["messages"].as_array() {
            for m in messages {
                let kind = match m["kind"].as_str() {
                    Some("user") => MessageKind::User,
                    Some("progress") => MessageKind::Progress,
                    Some("answer") => MessageKind::Answer,
                    _ => continue,
                };
                let Some(text) = m["text"].as_str().filter(|s| !s.trim().is_empty()) else {
                    continue;
                };
                if s.messages.len() >= MAX_MESSAGES || bytes >= MAX_TEXT {
                    s.truncated = true;
                    break;
                }
                let clipped = clip(text, (64 * 1024).min(MAX_TEXT - bytes));
                s.truncated |= clipped.len() < text.len();
                bytes += clipped.len();
                s.messages.push(Message {
                    kind,
                    text: clipped,
                });
            }
        }
    }
    Ok(s)
}
#[derive(Deserialize)]
struct Discovery {
    version: u8,
    endpoint: String,
    token: String,
}
pub struct Client {
    path: PathBuf,
    http: ureq::Agent,
}
impl Client {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            http: ureq::AgentBuilder::new()
                .redirects(0)
                .timeout_connect(Duration::from_millis(250))
                .timeout_read(Duration::from_millis(1200))
                .timeout_write(Duration::from_millis(250))
                .build(),
        }
    }
    pub fn poll(&self, full: bool, requested: Option<&Path>) -> Result<Update, String> {
        let mut bytes = Vec::new();
        File::open(&self.path)
            .map_err(|_| "等待 DSH 桥接插件")?
            .take(8193)
            .read_to_end(&mut bytes)
            .map_err(|_| "DSH 桥接发现文件暂不可读")?;
        if bytes.len() > 8192 {
            return Err("DSH 桥接发现文件过大".into());
        }
        let d: Discovery = serde_json::from_slice(&bytes).map_err(|_| "DSH 桥接发现文件无效")?;
        let url = url::Url::parse(&d.endpoint).map_err(|_| "DSH 桥接地址无效")?;
        if d.version != 1
            || url.scheme() != "http"
            || !matches!(
                url.host_str(),
                Some("127.0.0.1") | Some("[::1]") | Some("::1")
            )
            || url.port().is_none()
            || url.path() != "/"
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
            || d.token.len() != 64
            || !d.token.bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err("DSH 桥接仅允许本机受认证的只读接口".into());
        }
        let body =
            serde_json::to_vec(&json!({"full":full,"selected":requested.and_then(session_id)}))
                .map_err(|_| "无法创建 DSH 快照请求")?;
        let response = self
            .http
            .post(&format!("{}/snapshot", d.endpoint.trim_end_matches('/')))
            .set("Authorization", &format!("Bearer {}", d.token))
            .set("Content-Type", "application/json")
            .send_bytes(&body)
            .map_err(|_| "DSH 桥接暂不可用，请检查插件是否启用")?;
        let mut bytes = Vec::new();
        response
            .into_reader()
            .take(2 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| "DSH 会话读取中断")?;
        if bytes.len() > 2 * 1024 * 1024 {
            return Err("DSH 快照超过读取上限".into());
        }
        let value: Value = serde_json::from_slice(&bytes).map_err(|_| "DSH 快照格式无效")?;
        if value["version"] != 1 {
            return Err("DSH 桥接协议版本不兼容".into());
        }
        let mut recent = value["sessions"]
            .as_array()
            .ok_or("DSH 会话列表无效")?
            .iter()
            .take(32)
            .filter_map(|v| parse_snapshot(v, false).ok())
            .collect::<Vec<_>>();
        recent.sort_by_key(|s| std::cmp::Reverse(s.activity_ms));
        let mut selected = if value["selected"].is_null() {
            None
        } else {
            Some(parse_snapshot(&value["selected"], full)?)
        };
        let read_receipts:Vec<_>=value["readReceipts"].as_array().into_iter().flatten().take(128).filter_map(|r| {
            let id=r["id"].as_str().filter(|id|valid_id(id))?;
            let viewed_at_ms=r["viewedAt"].as_i64().filter(|at|*at>0)?;
            Some(super::SessionReadReceipt {source:Source::Dsh,id:format!("dsh:{id}"),receipt:super::ReadReceipt {viewed_at_ms:Some(viewed_at_ms),..Default::default()}})
        }).collect();
        for s in recent.iter_mut().chain(selected.iter_mut()) {
            s.read_receipt=read_receipts.iter().find(|r|r.id==s.id).map(|r|r.receipt.clone());
        }
        let error = (selected.is_none() && requested.is_some())
            .then(|| "固定的 DSH 会话暂不可读，请重新选择会话".into());
        Ok(Update {
            latest: recent.first().map(|s| s.path.clone()),
            selected,
            recent,
            error,
            dsh_status: "DSH 桥接已连接".into(),
            ready: true,
            read_receipts,
        })
    }
}
