use crate::transfer::{Groups, ItemInfo, Kind, FAVORITES};
use serde::{Deserialize, Serialize};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};
pub const MAX_RECENT: usize = 100;
pub const MAX_PINNED: usize = 200;
pub const MAX_CHARS: usize = 1_000_000;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Picture {
    pub width: u32,
    pub height: u32,
    #[serde(with = "image_bytes")]
    pub png: Arc<Vec<u8>>,
}
mod image_bytes {
    use super::*;
    use base64::{engine::general_purpose::STANDARD, Engine};
    pub fn serialize<S: serde::Serializer>(v: &Arc<Vec<u8>>, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&STANDARD.encode(v.as_slice()))
    }
    pub fn deserialize<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Arc<Vec<u8>>, D::Error> {
        let encoded = String::deserialize(d)?;
        let data = STANDARD.decode(encoded).map_err(serde::de::Error::custom)?;
        if data.len() > 8 * 1024 * 1024 {
            return Err(serde::de::Error::custom("图片超过 8 MiB"));
        }
        Ok(Arc::new(data))
    }
}
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Clip {
    pub text: String,
    pub at: String,
    pub pinned: bool,
    pub id: u64,
    pub kind: Kind,
    pub source: String,
    pub html: String,
    pub rtf: String,
    pub picture: Option<Picture>,
    pub files: Vec<PathBuf>,
    pub categories: Vec<u64>,
    pub manual: bool,
}
impl Clip {
    pub fn plain_text(&self)->String{
        if self.files.is_empty(){self.text.clone()}else{self.files.iter().map(|p|p.to_string_lossy()).collect::<Vec<_>>().join("\n")}
    }
    pub fn info(&self) -> ItemInfo {
        ItemInfo {
            kind: self.kind.label().into(),
            text: self.text.clone(),
            source: self.source.clone(),
            path: self
                .files
                .iter()
                .map(|p| p.to_string_lossy())
                .collect::<Vec<_>>()
                .join("\n"),
        }
    }
    pub fn preview(&self) -> String {
        if !self.files.is_empty() {
            let first = self.files[0]
                .file_name()
                .unwrap_or(self.files[0].as_os_str())
                .to_string_lossy();
            if self.files.len() == 1 {
                first.into_owned()
            } else {
                format!("{first} 等 {} 个文件", self.files.len())
            }
        } else if let Some(p) = &self.picture {
            format!("图片 · {} × {}", p.width, p.height)
        } else {
            self.text
                .lines()
                .next()
                .unwrap_or("")
                .chars()
                .take(300)
                .collect()
        }
    }
    pub fn bytes(&self) -> usize {
        self.text.len()
            + self.html.len()
            + self.rtf.len()
            + self.picture.as_ref().map_or(0, |p| p.png.len())
            + self
                .files
                .iter()
                .map(|p| p.as_os_str().len() * 2)
                .sum::<usize>()
    }
}
#[derive(Debug)]
pub struct History {
    pub items: Vec<Clip>,
    pub groups: Groups,
    pub version: u64,
    pub error: Option<String>,
    pub max_recent: usize,
    path: Option<PathBuf>,
}
impl Default for History {
    fn default() -> Self {
        Self {
            items: vec![],
            groups: Groups::clipboard(),
            version: 0,
            error: None,
            max_recent: MAX_RECENT,
            path: None,
        }
    }
}
impl History {
    pub fn push(&mut self, text: &str, at: &str) -> bool {
        self.push_clip(Clip {
            text: text.into(),
            at: at.into(),
            ..Default::default()
        })
    }
    pub fn push_clip(&mut self, mut clip: Clip) -> bool {
        if clip.files.len() > 1000
            || clip
                .files
                .iter()
                .any(|p| !p.is_absolute() || p.as_os_str().len() > 32768)
        {
            return false;
        }
        if clip.picture.is_none()
            && clip.files.is_empty()
            && (clip.text.trim().is_empty() || clip.text.chars().count() > MAX_CHARS)
        {
            return false;
        }
        if clip.bytes() > 8 * 1024 * 1024 {
            return false;
        }
        clip.kind = if !clip.files.is_empty() {
            clip.text = clip
                .files
                .iter()
                .map(|p| p.to_string_lossy())
                .collect::<Vec<_>>()
                .join("\n");
            Kind::Files
        } else if clip.picture.is_some() {
            Kind::Image
        } else {
            crate::transfer::classify(&clip.text)
        };
        let same = |old: &Clip| {
            if !clip.files.is_empty() {
                old.files == clip.files
            } else if !old.files.is_empty() {
                false
            } else if let Some(image) = &clip.picture {
                old.picture.as_ref().is_some_and(|p| p == image)
            } else {
                old.picture.is_none() && old.text == clip.text
            }
        };
        if let Some(index) = self.items.iter().position(same) {
            if index == 0
                && !clip.manual
                && self.items[0].source == clip.source
                && self.items[0].html == clip.html
                && self.items[0].rtf == clip.rtf
            {
                return false;
            }
            let old = self.items.remove(index);
            clip.id = old.id;
            clip.pinned |= old.pinned;
            for id in old.categories {
                if !clip.categories.contains(&id) {
                    clip.categories.push(id);
                }
            }
            clip.manual |= old.manual;
        } else {
            clip.id = self.items.iter().map(|c| c.id).max().unwrap_or(0) + 1;
        }
        if !clip.manual {
            self.groups.apply(&clip.info(), &mut clip.categories);
        }
        self.items.insert(0, clip);
        let mut recent = 0;
        let mut bytes = 0;
        self.items.retain(|c| {
            let keep = c.pinned || c.manual || self.groups.kept(&c.categories);
            bytes += c.bytes();
            keep || {
                recent += 1;
                recent <= self.max_recent && bytes <= 32 * 1024 * 1024
            }
        });
        self.changed();
        true
    }
    pub fn add_phrase(&mut self, text: &str) -> bool {
        self.push_clip(Clip {
            text: text.into(),
            at: chrono::Local::now().format("%H:%M").to_string(),
            pinned: true,
            manual: true,
            categories: vec![FAVORITES],
            ..Default::default()
        })
    }
    pub fn remove(&mut self, index: usize) {
        if index < self.items.len() {
            self.items.remove(index);
            self.changed();
        }
    }
    pub fn get_id(&self, id: u64) -> Option<&Clip> {
        self.items.iter().find(|c| c.id == id)
    }
    pub fn remove_id(&mut self, id: u64) {
        if let Some(index) = self.items.iter().position(|c| c.id == id) {
            self.remove(index);
        }
    }
    pub fn pin_id(&mut self, id: u64) -> bool {
        self.items
            .iter()
            .position(|c| c.id == id)
            .is_some_and(|index| self.toggle_pin(index))
    }
    pub fn clear_recent(&mut self) {
        self.items
            .retain(|c| c.pinned || c.manual || self.groups.kept(&c.categories));
        self.changed();
    }
    pub fn clear_session(&mut self) {
        self.clear_recent();
    }
    pub fn toggle_pin(&mut self, index: usize) -> bool {
        let count = self.items.iter().filter(|c| c.pinned).count();
        let Some(clip) = self.items.get_mut(index) else {
            return false;
        };
        if !clip.pinned && count >= MAX_PINNED {
            return false;
        }
        clip.pinned = !clip.pinned;
        if clip.pinned && !clip.categories.contains(&FAVORITES) {
            clip.categories.push(FAVORITES);
        } else if !clip.pinned {
            clip.categories.retain(|id| *id != FAVORITES);
        }
        self.changed();
        true
    }
    pub(crate) fn to_json(&self) -> Vec<u8> {
        serde_json::to_vec(
            &serde_json::json!({"version":2,"items":self.items,"groups":self.groups}),
        )
        .unwrap_or_default()
    }
    pub fn from_json(raw: &[u8]) -> Self {
        let value: serde_json::Value = serde_json::from_slice(raw).unwrap_or_default();
        let (rows, groups) = if value.is_array() {
            (
                value.as_array().cloned().unwrap_or_default(),
                Groups::clipboard(),
            )
        } else {
            (
                value["items"].as_array().cloned().unwrap_or_default(),
                serde_json::from_value(value["groups"].clone())
                    .unwrap_or_else(|_| Groups::clipboard()),
            )
        };
        let mut history = Self {
            groups,
            ..Default::default()
        };
        history.groups.has_favorites = true;
        history.groups.normalize();
        for mut row in rows.into_iter().take(MAX_RECENT + MAX_PINNED) {
            if row.get("pinned").is_none() {
                row["pinned"] = true.into();
            }
            if let Ok(mut clip) = serde_json::from_value::<Clip>(row) {
                if clip.text.is_empty() && clip.picture.is_none() && clip.files.is_empty() {
                    continue;
                }
                if clip.id == 0 || history.items.iter().any(|c| c.id == clip.id) {
                    clip.id = history.items.iter().map(|c| c.id).max().unwrap_or(0) + 1;
                }
                if !clip.files.is_empty() {
                    clip.kind = Kind::Files;
                } else if clip.picture.is_none() {
                    clip.kind = crate::transfer::classify(&clip.text);
                }
                history.items.push(clip);
            }
        }
        history
    }
    pub fn load() -> Self {
        let directory = crate::model::config_dir();
        let path = directory.join("clipboard.dat");
        if path.exists() {
            let mut h = Self::load_from(path);
            h.clear_session();
            h
        } else {
            let legacy = directory.join("clipboard.json");
            let raw = std::fs::read(&legacy).ok();
            let valid = raw
                .as_ref()
                .is_some_and(|raw| serde_json::from_slice::<serde_json::Value>(raw).is_ok());
            let mut h = raw.map(|raw| Self::from_json(&raw)).unwrap_or_default();
            h.path = Some(path.clone());
            h.clear_session();
            if valid && h.error.is_none() && crate::secure_store::read(&path).is_ok() {
                let _ = std::fs::remove_file(legacy);
            }
            h
        }
    }
    pub(crate) fn load_from(path: PathBuf) -> Self {
        let raw = crate::secure_store::read(&path).or_else(|_| std::fs::read(&path));
        let mut h = raw.map(|raw| Self::from_json(&raw)).unwrap_or_default();
        h.path = Some(path);
        h
    }
    pub fn changed(&mut self) {
        self.version += 1;
        if let Some(path) = &self.path {
            self.error = crate::secure_store::write(path, &self.to_json())
                .err()
                .map(|e| format!("记录保存失败：{e}"));
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecordOptions {
    pub excluded: Vec<String>,
    pub images: bool,
    pub max_recent: usize,
}
impl Default for RecordOptions {
    fn default() -> Self {
        Self {
            excluded: vec![],
            images: true,
            max_recent: MAX_RECENT,
        }
    }
}
#[derive(Clone)]
pub struct Shared {
    pub history: Arc<Mutex<History>>,
    pub enabled: Arc<AtomicBool>,
    pub options: Arc<Mutex<RecordOptions>>,
}
impl Shared {
    pub fn new(enabled: bool) -> Self {
        Self {
            history: Arc::new(Mutex::new(History::load())),
            enabled: Arc::new(AtomicBool::new(enabled)),
            options: Default::default(),
        }
    }
    pub fn set_enabled(&self, on: bool) {
        self.enabled.store(on, Ordering::Relaxed);
    }
    pub fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }
    pub fn lock(&self) -> std::sync::MutexGuard<'_, History> {
        self.history.lock().unwrap_or_else(|e| e.into_inner())
    }
    pub fn configure(&self, excluded: Vec<String>, images: bool, max_recent: usize) {
        let options = RecordOptions {
            excluded,
            images,
            max_recent: max_recent.clamp(30, 500),
        };
        if let Ok(mut current) = self.options.lock() {
            if *current != options {
                self.lock().max_recent = options.max_recent;
                *current = options;
            }
        }
    }
    pub fn allows_source(&self, source: &str) -> bool {
        let name = |value: &str| {
            std::path::Path::new(value)
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_lowercase()
                .trim_end_matches(".exe")
                .to_string()
        };
        self.is_enabled()
            && self
                .options
                .lock()
                .map(|o| {
                    !o.excluded
                        .iter()
                        .any(|excluded| name(excluded) == name(source))
                })
                .unwrap_or(false)
    }
    pub(crate) fn record_update(&self, text: &str, from_history: bool) -> bool {
        !from_history && self.record(text)
    }
    pub fn record(&self, text: &str) -> bool {
        self.record_source(text, "", "", "")
    }
    pub fn record_source(&self, text: &str, source: &str, html: &str, rtf: &str) -> bool {
        if !self.allows_source(source) {
            return false;
        }
        let mut history = self.lock();
        if !self.is_enabled() {
            return false;
        }
        history.push_clip(Clip {
            text: text.into(),
            at: chrono::Local::now().format("%H:%M").to_string(),
            source: source.into(),
            html: html.into(),
            rtf: rtf.into(),
            ..Default::default()
        })
    }
    pub fn record_picture(&self, picture: Picture, source: &str) -> bool {
        if !self.allows_source(source) || !self.options.lock().is_ok_and(|o| o.images) {
            return false;
        }
        let mut history = self.lock();
        if !self.is_enabled() {
            return false;
        }
        history.push_clip(Clip {
            picture: Some(picture),
            source: source.into(),
            at: chrono::Local::now().format("%H:%M").to_string(),
            ..Default::default()
        })
    }
    pub fn record_files(&self, files: Vec<PathBuf>, source: &str) -> bool {
        if files.is_empty() || !self.allows_source(source) {
            return false;
        }
        let mut history = self.lock();
        if !self.is_enabled() {
            return false;
        }
        history.push_clip(Clip {
            files,
            source: source.into(),
            at: chrono::Local::now().format("%H:%M").to_string(),
            ..Default::default()
        })
    }
}
impl Default for Shared {
    fn default() -> Self {
        Self::new(true)
    }
}
pub fn preview(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}
