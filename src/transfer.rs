//! 剪贴板与文件中转的分类、规则与多选。
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
pub const FAVORITES: u64 = 1;

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum Kind {
    #[default]
    Text,
    Link,
    Code,
    Color,
    Image,
    Files,
}
impl Kind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Text => "文字",
            Self::Link => "链接",
            Self::Code => "代码",
            Self::Color => "颜色",
            Self::Image => "图片",
            Self::Files => "文件",
        }
    }
}
pub fn classify(text: &str) -> Kind {
    let text = text.trim();
    if !text.contains('\n')
        && url::Url::parse(text)
            .is_ok_and(|u| matches!(u.scheme(), "http" | "https") && u.host_str().is_some())
    {
        return Kind::Link;
    }
    if color(text).is_some() {
        return Kind::Color;
    }
    if [
        "fn ",
        "function ",
        "const ",
        "let ",
        "class ",
        "public ",
        "import ",
        "SELECT ",
        "def ",
        "<div",
        "{\"",
    ]
    .iter()
    .any(|p| text.contains(p))
        && (text.contains(['\n', '{', ';']) || text.starts_with("SELECT "))
    {
        return Kind::Code;
    }
    Kind::Text
}
pub fn color(text: &str) -> Option<[u8; 3]> {
    let hex = text.trim().strip_prefix('#')?;
    if hex.len() == 6 && hex.is_ascii() {
        Some([
            u8::from_str_radix(&hex[0..2], 16).ok()?,
            u8::from_str_radix(&hex[2..4], 16).ok()?,
            u8::from_str_radix(&hex[4..6], 16).ok()?,
        ])
    } else if hex.len() == 3 && hex.is_ascii() {
        let mut out = [0; 3];
        for (i, b) in hex.bytes().enumerate() {
            out[i] = (b as char).to_digit(16)? as u8 * 17;
        }
        Some(out)
    } else {
        None
    }
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum Field {
    Kind,
    #[default]
    Text,
    Domain,
    Source,
    Extension,
    FileName,
    Folder,
}
impl Field {
    pub const ALL: [Self; 7] = [
        Self::Kind,
        Self::Text,
        Self::Domain,
        Self::Source,
        Self::Extension,
        Self::FileName,
        Self::Folder,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Kind => "内容类型",
            Self::Text => "文字内容",
            Self::Domain => "链接域名",
            Self::Source => "来源应用",
            Self::Extension => "扩展名",
            Self::FileName => "文件名",
            Self::Folder => "所在文件夹",
        }
    }
}
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum Op {
    #[default]
    Contains,
    StartsWith,
    EndsWith,
    Equals,
    Regex,
}
impl Op {
    pub const ALL: [Self; 5] = [
        Self::Contains,
        Self::StartsWith,
        Self::EndsWith,
        Self::Equals,
        Self::Regex,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Contains => "包含",
            Self::StartsWith => "开头是",
            Self::EndsWith => "结尾是",
            Self::Equals => "等于",
            Self::Regex => "正则",
        }
    }
}
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Condition {
    pub field: Field,
    pub op: Op,
    pub value: String,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Rule {
    pub all: bool,
    pub conditions: Vec<Condition>,
}
#[derive(Clone, Debug, Default)]
pub struct ItemInfo {
    pub kind: String,
    pub text: String,
    pub source: String,
    pub path: String,
}
impl Rule {
    pub fn problem(&self) -> Option<String> {
        self.conditions
            .iter()
            .filter(|c| c.op == Op::Regex && !c.value.trim().is_empty())
            .find_map(|c| regex::Regex::new(&c.value).err().map(|e| e.to_string()))
    }
    pub fn matches(&self, item: &ItemInfo) -> bool {
        let conditions: Vec<_> = self
            .conditions
            .iter()
            .filter(|c| !c.value.trim().is_empty())
            .collect();
        if conditions.is_empty() || self.problem().is_some() {
            return false;
        }
        let test = |c: &&Condition| {
            let path = std::path::Path::new(&item.path);
            let value = match c.field {
                Field::Kind => item.kind.clone(),
                Field::Text => item.text.clone(),
                Field::Domain => url::Url::parse(item.text.trim())
                    .ok()
                    .and_then(|u| u.host_str().map(str::to_owned))
                    .unwrap_or_default(),
                Field::Source => item.source.clone(),
                Field::Extension => path
                    .extension()
                    .map(|v| v.to_string_lossy().to_string())
                    .unwrap_or_default(),
                Field::FileName => path
                    .file_name()
                    .map(|v| v.to_string_lossy().to_string())
                    .unwrap_or_default(),
                Field::Folder => path
                    .parent()
                    .map(|v| v.to_string_lossy().to_string())
                    .unwrap_or_default(),
            };
            if c.op == Op::Regex {
                return regex::RegexBuilder::new(&c.value)
                    .case_insensitive(true)
                    .size_limit(1_000_000)
                    .build()
                    .is_ok_and(|r| r.is_match(&value));
            }
            let value = value.to_lowercase();
            let needle = c
                .value
                .trim()
                .trim_start_matches(if c.field == Field::Extension {
                    '.'
                } else {
                    '\0'
                })
                .to_lowercase();
            match c.op {
                Op::Contains => value.contains(&needle),
                Op::StartsWith => value.starts_with(&needle),
                Op::EndsWith => value.ends_with(&needle),
                Op::Equals => value == needle,
                Op::Regex => false,
            }
        };
        if self.all {
            conditions.iter().all(test)
        } else {
            conditions.iter().any(test)
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Category {
    pub id: u64,
    pub name: String,
    #[serde(default = "default_color")]
    pub color: String,
    #[serde(default)]
    pub keep: bool,
    #[serde(default)]
    pub rule: Rule,
}
fn default_color() -> String {
    "#9CB9FF".into()
}
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Groups {
    pub entries: Vec<Category>,
    #[serde(default)]
    pub has_favorites: bool,
}
impl Groups {
    pub fn clipboard() -> Self {
        Self {
            entries: vec![Category {
                id: FAVORITES,
                name: "常用".into(),
                color: default_color(),
                keep: true,
                rule: Rule::default(),
            }],
            has_favorites: true,
        }
    }
    pub fn normalize(&mut self) {
        let mut ids = HashSet::new();
        self.entries
            .retain(|c| !c.name.trim().is_empty() && ids.insert(c.id));
        if self.has_favorites {
            let mut favorite = self
                .entries
                .iter()
                .find(|c| c.id == FAVORITES)
                .cloned()
                .unwrap_or_else(|| Self::clipboard().entries.remove(0));
            favorite.name = "常用".into();
            favorite.keep = true;
            self.entries.retain(|c| c.id != FAVORITES);
            self.entries.insert(0, favorite);
        }
    }
    pub fn add(&mut self, name: &str) -> Result<u64, String> {
        let name = name.trim();
        if name.is_empty() || name.chars().count() > 30 {
            return Err("分类名称应为 1–30 个字符".into());
        }
        if self.entries.iter().any(|c| c.name == name) {
            return Err("已有同名分类".into());
        }
        let id = self.entries.iter().map(|c| c.id).max().unwrap_or(1) + 1;
        self.entries.push(Category {
            id,
            name: name.into(),
            color: default_color(),
            keep: false,
            rule: Rule::default(),
        });
        Ok(id)
    }
    pub fn rename(&mut self, id: u64, name: &str) -> bool {
        let name = name.trim();
        if self.has_favorites && id == FAVORITES
            || name.is_empty()
            || name.chars().count() > 30
            || self.entries.iter().any(|c| c.id != id && c.name == name)
        {
            return false;
        }
        if let Some(c) = self.entries.iter_mut().find(|c| c.id == id) {
            c.name = name.into();
            true
        } else {
            false
        }
    }
    pub fn remove(&mut self, id: u64) -> bool {
        if self.has_favorites && id == FAVORITES {
            return false;
        }
        let old = self.entries.len();
        self.entries.retain(|c| c.id != id);
        old != self.entries.len()
    }
    pub fn move_by(&mut self, id: u64, offset: isize) -> bool {
        let Some(index) = self.entries.iter().position(|c| c.id == id) else {
            return false;
        };
        let to = index as isize + offset;
        if to < (self.has_favorites as isize)
            || to >= self.entries.len() as isize
            || self.has_favorites && id == FAVORITES
        {
            return false;
        }
        self.entries.swap(index, to as usize);
        true
    }
    pub fn kept(&self, ids: &[u64]) -> bool {
        self.entries
            .iter()
            .any(|c| ids.contains(&c.id) && (c.keep || self.has_favorites && c.id == FAVORITES))
    }
    pub fn apply(&self, item: &ItemInfo, ids: &mut Vec<u64>) {
        for c in &self.entries {
            if c.rule.matches(item) && !ids.contains(&c.id) {
                ids.push(c.id);
            }
        }
    }
}
#[derive(Clone, Debug, Default)]
pub struct Selection {
    pub ids: HashSet<String>,
    anchor: Option<usize>,
}
impl Selection {
    pub fn click(&mut self, index: usize, visible: &[String], control: bool, shift: bool) {
        if index >= visible.len() {
            return;
        }
        if shift {
            let start = self.anchor.unwrap_or(index).min(visible.len() - 1);
            if !control {
                self.ids.clear();
            }
            for id in &visible[start.min(index)..=start.max(index)] {
                self.ids.insert(id.clone());
            }
        } else {
            if !control {
                self.ids.clear();
            }
            let id = &visible[index];
            if control && !self.ids.insert(id.clone()) {
                self.ids.remove(id);
            } else {
                self.ids.insert(id.clone());
            }
            self.anchor = Some(index);
        }
    }
    pub fn ordered(&self, visible: &[String]) -> Vec<String> {
        visible
            .iter()
            .filter(|id| self.ids.contains(*id))
            .cloned()
            .collect()
    }
}
