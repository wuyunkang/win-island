//! 本机实时活动与 HTTP 接入；不依赖桌面界面或 Windows API。

mod http;
pub use http::Server;
use serde::{Deserialize, Serialize};
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

pub type Shared = Arc<Mutex<Store>>;

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Activity,
    Notification,
}

#[derive(Debug, Deserialize)]
pub struct ActivityRequest {
    pub id: Option<String>,
    pub title: Option<String>,
    pub subtitle: Option<String>,
    pub progress: Option<f32>,
    pub duration: Option<f64>,
    #[serde(default = "default_priority")]
    pub priority: u8,
}
fn default_priority() -> u8 {
    50
}

#[derive(Clone, Debug, Serialize)]
pub struct Activity {
    pub id: String,
    pub title: String,
    pub subtitle: String,
    pub progress: Option<f32>,
    pub priority: u8,
    pub kind: Kind,
    #[serde(skip)]
    expires: Option<Instant>,
    #[serde(skip)]
    sequence: u64,
}

#[derive(Default)]
pub struct Store {
    items: Vec<Activity>,
    sequence: u64,
}

pub fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_' | b'.'))
}
fn clean(value: String, max: usize) -> String {
    value
        .chars()
        .filter(|c| !c.is_control())
        .take(max)
        .collect::<String>()
        .trim()
        .to_owned()
}

impl Store {
    pub fn upsert(
        &mut self,
        request: ActivityRequest,
        kind: Kind,
        now: Instant,
    ) -> Result<String, String> {
        if request.id.as_ref().is_some_and(|id| !valid_id(id)) {
            return Err("id 只能包含 1–64 个字母、数字、点、下划线或横线".into());
        }
        let title = clean(request.title.unwrap_or_default(), 120);
        if title.is_empty() && request.progress.is_none() {
            return Err("需要 title 或 progress".into());
        }
        if request
            .progress
            .is_some_and(|p| !p.is_finite() || !(0.0..=1.0).contains(&p))
        {
            return Err("progress 必须介于 0 和 1".into());
        }
        if request.priority > 99 {
            return Err("priority 必须介于 0 和 99".into());
        }
        if request
            .duration
            .is_some_and(|d| !d.is_finite() || d <= 0.0 || d > 86400.0)
        {
            return Err("duration 必须大于 0 且不超过 86400 秒".into());
        }
        self.expire(now);
        self.sequence += 1;
        let id = request.id.unwrap_or_else(|| loop {
            let id = format!("event-{}", self.sequence);
            if self.get(&id).is_none() {
                break id;
            }
            self.sequence += 1;
        });
        if self.items.len() >= 64 && self.get(&id).is_none() {
            return Err("实时活动已达到 64 项，请先撤销旧任务".into());
        }
        let duration = request
            .duration
            .or((kind == Kind::Notification).then_some(5.0));
        let expires = duration.map(|d| now + Duration::from_secs_f64(d));
        let activity = Activity {
            id: id.clone(),
            title: if title.is_empty() {
                "任务进度".into()
            } else {
                title
            },
            subtitle: clean(request.subtitle.unwrap_or_default(), 300),
            progress: request.progress,
            priority: request.priority,
            kind,
            expires,
            sequence: self.sequence,
        };
        self.remove(&id);
        self.items.push(activity);
        Ok(id)
    }
    pub fn get(&self, id: &str) -> Option<&Activity> {
        self.items.iter().find(|a| a.id == id)
    }
    pub fn remove(&mut self, id: &str) -> bool {
        let len = self.items.len();
        self.items.retain(|a| a.id != id);
        len != self.items.len()
    }
    pub fn clear(&mut self) {
        self.items.clear();
    }
    pub fn expire(&mut self, now: Instant) {
        self.items
            .retain(|a| a.expires.is_none_or(|until| now < until));
    }
    pub fn list(&mut self, now: Instant) -> Vec<Activity> {
        self.expire(now);
        let mut list = self.items.clone();
        list.sort_by(|a, b| {
            b.priority
                .cmp(&a.priority)
                .then_with(|| b.sequence.cmp(&a.sequence))
        });
        list
    }
    pub fn selected(&mut self, now: Instant) -> Option<Activity> {
        self.list(now).into_iter().next()
    }
}
