//! 剪贴板记录的类型图标；文件夹检查在后台完成，避免慢磁盘卡住界面。
use crate::transfer::Kind;
use eframe::egui::{self, Color32, Pos2, Rect, Sense, Stroke, Vec2};
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver, SyncSender},
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum IconKind {
    Text,
    Link,
    Code,
    Color,
    Image,
    Word,
    Excel,
    PowerPoint,
    Folder,
    Archive,
    File,
    Files,
}

impl IconKind {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Text => "文字",
            Self::Link => "链接",
            Self::Code => "代码",
            Self::Color => "颜色",
            Self::Image => "图片",
            Self::Word => "Word 文档",
            Self::Excel => "Excel 表格",
            Self::PowerPoint => "PowerPoint 演示文稿",
            Self::Folder => "文件夹",
            Self::Archive => "压缩包",
            Self::File => "文件",
            Self::Files => "多个文件",
        }
    }
}

pub(crate) fn icon_for(kind: Kind, files: &[PathBuf], is_dir: Option<bool>) -> IconKind {
    if files.len() > 1 {
        return IconKind::Files;
    }
    if let Some(path) = files.first() {
        if is_dir == Some(true) {
            return IconKind::Folder;
        }
        let extension = path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        return match extension.as_str() {
            "doc" | "docx" | "docm" | "dot" | "dotx" | "dotm" | "wps" => IconKind::Word,
            "xls" | "xlsx" | "xlsm" | "xlsb" | "xlt" | "xltx" | "xltm" | "et" => IconKind::Excel,
            "ppt" | "pptx" | "pptm" | "pps" | "ppsx" | "pot" | "potx" | "dps" => {
                IconKind::PowerPoint
            }
            "zip" | "rar" | "7z" | "tar" | "gz" | "bz2" | "xz" | "zst" | "tgz" | "cab" => {
                IconKind::Archive
            }
            "txt" | "md" | "rtf" | "log" => IconKind::Text,
            _ => IconKind::File,
        };
    }
    match kind {
        Kind::Text => IconKind::Text,
        Kind::Link => IconKind::Link,
        Kind::Code => IconKind::Code,
        Kind::Color => IconKind::Color,
        Kind::Image => IconKind::Image,
        Kind::Files => IconKind::File,
    }
}

type Lookup = (PathBuf, Option<bool>);

#[derive(Default)]
pub(crate) struct DirectoryCache {
    entries: HashMap<PathBuf, (Option<bool>, Instant)>,
    pending: HashSet<PathBuf>,
    worker: Option<(SyncSender<PathBuf>, Receiver<Lookup>)>,
}

impl DirectoryCache {
    pub(crate) fn clear(&mut self) {
        *self = Self::default();
    }
    fn start(&mut self, ctx: egui::Context, load: impl Fn(&Path) -> Option<bool> + Send + 'static) {
        let (requests, rx) = mpsc::sync_channel::<PathBuf>(16);
        let (tx, results) = mpsc::channel();
        if std::thread::Builder::new()
            .name("island-clipboard-types".into())
            .spawn(move || {
                while let Ok(path) = rx.recv() {
                    let is_dir = load(&path);
                    if tx.send((path, is_dir)).is_err() {
                        break;
                    }
                    ctx.request_repaint();
                }
            })
            .is_ok()
        {
            self.worker = Some((requests, results));
        }
    }

    pub(crate) fn get(&mut self, ctx: &egui::Context, path: &Path) -> Option<bool> {
        if self.worker.is_none() {
            self.start(ctx.clone(), |path| {
                std::fs::metadata(path).ok().map(|m| m.is_dir())
            });
        }
        let (tx, rx) = self.worker.as_ref()?;
        while let Ok((path, result)) = rx.try_recv() {
            self.pending.remove(&path);
            if self.entries.len() >= 128 {
                if let Some(oldest) = self
                    .entries
                    .iter()
                    .min_by_key(|(_, (_, at))| *at)
                    .map(|(path, _)| path.clone())
                {
                    self.entries.remove(&oldest);
                }
            }
            self.entries.insert(path, (result, Instant::now()));
        }
        if let Some((value, at)) = self.entries.get(path) {
            if at.elapsed() < Duration::from_secs(30) {
                return *value;
            }
        }
        self.entries.remove(path);
        if !self.pending.contains(path) && tx.try_send(path.to_path_buf()).is_ok() {
            self.pending.insert(path.to_path_buf());
        }
        None
    }
}

/// 小尺寸矢量图标使用形状和字母区分类型，不依赖系统关联程序。
pub(crate) fn show(ui: &mut egui::Ui, kind: IconKind) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(14.0), Sense::hover());
    let p = ui.painter_at(rect);
    let at = |x: f32, y: f32| Pos2::new(rect.left() + x, rect.top() + y);
    let box_at = |x: f32, y: f32, w: f32, h: f32| Rect::from_min_size(at(x, y), Vec2::new(w, h));
    match kind {
        IconKind::Folder => {
            p.rect_filled(
                box_at(0.5, 2.0, 6.0, 4.0),
                1.2,
                Color32::from_rgb(230, 169, 67),
            );
            p.rect_filled(
                box_at(0.5, 4.0, 13.0, 8.5),
                1.5,
                Color32::from_rgb(255, 191, 80),
            );
            p.line_segment(
                [at(1.6, 5.2), at(12.1, 5.2)],
                Stroke::new(0.8, Color32::from_rgb(255, 217, 140)),
            );
        }
        IconKind::Archive => {
            p.rect_filled(
                box_at(1.5, 0.5, 11.0, 13.0),
                2.0,
                Color32::from_rgb(144, 104, 211),
            );
            for y in [2.0, 4.0, 6.0, 8.0] {
                p.line_segment([at(6.0, y), at(8.0, y)], Stroke::new(1.0, Color32::WHITE));
            }
            p.rect_filled(
                box_at(6.1, 10.0, 1.8, 2.0),
                0.5,
                Color32::from_rgb(236, 224, 255),
            );
        }
        IconKind::Word | IconKind::Excel | IconKind::PowerPoint => {
            let (color, letter) = match kind {
                IconKind::Word => (Color32::from_rgb(50, 117, 210), "W"),
                IconKind::Excel => (Color32::from_rgb(42, 151, 102), "X"),
                _ => (Color32::from_rgb(224, 112, 73), "P"),
            };
            p.rect_filled(box_at(1.0, 0.5, 12.0, 13.0), 2.0, color);
            p.text(
                at(7.0, 7.0),
                egui::Align2::CENTER_CENTER,
                letter,
                egui::FontId::proportional(10.0),
                Color32::WHITE,
            );
        }
        IconKind::Text | IconKind::File | IconKind::Files => {
            if kind == IconKind::Files {
                p.rect_filled(
                    box_at(0.5, 0.5, 10.0, 11.0),
                    1.4,
                    Color32::from_rgb(120, 147, 176),
                );
            }
            let x = if kind == IconKind::Files { 3.0 } else { 2.0 };
            let y = if kind == IconKind::Files { 2.5 } else { 0.5 };
            p.rect_filled(
                box_at(x, y, 9.0, 11.0),
                1.4,
                Color32::from_rgb(199, 217, 236),
            );
            let ink = Color32::from_rgb(85, 120, 158);
            for dy in [3.0, 5.5, 8.0] {
                let end = if dy == 8.0 { 6.0 } else { 7.0 };
                p.line_segment(
                    [at(x + 2.0, y + dy), at(x + end, y + dy)],
                    Stroke::new(0.9, ink),
                );
            }
        }
        IconKind::Link => {
            let stroke = Stroke::new(1.7, Color32::from_rgb(102, 203, 203));
            p.circle_stroke(at(5.0, 8.5), 3.1, stroke);
            p.circle_stroke(at(9.0, 5.5), 3.1, stroke);
            p.line_segment([at(5.3, 8.4), at(8.7, 5.6)], stroke);
        }
        IconKind::Code => {
            p.rect_filled(
                box_at(0.5, 1.0, 13.0, 12.0),
                2.0,
                Color32::from_rgb(87, 105, 158),
            );
            p.text(
                at(7.0, 7.0),
                egui::Align2::CENTER_CENTER,
                "<>",
                egui::FontId::monospace(8.0),
                Color32::WHITE,
            );
        }
        IconKind::Color => {
            p.circle_filled(at(7.0, 7.0), 5.5, Color32::from_rgb(160, 138, 207));
        }
        IconKind::Image => {
            p.rect_filled(
                box_at(0.5, 1.0, 13.0, 12.0),
                2.0,
                Color32::from_rgb(82, 153, 138),
            );
            p.circle_filled(at(10.0, 4.5), 1.3, Color32::WHITE);
            p.add(egui::Shape::line(
                vec![
                    at(2.0, 10.5),
                    at(5.0, 7.0),
                    at(7.0, 9.0),
                    at(8.5, 7.7),
                    at(12.0, 11.0),
                ],
                Stroke::new(1.0, Color32::WHITE),
            ));
        }
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slow_directory_lookup_does_not_block_ui_or_unboundedly_queue() {
        let ctx = egui::Context::default();
        let mut cache = DirectoryCache::default();
        let (started, ready) = mpsc::channel();
        let (release, blocked) = mpsc::channel();
        cache.start(ctx.clone(), move |_| {
            let _ = started.send(());
            let _ = blocked.recv();
            Some(true)
        });
        let path = Path::new("slow-directory");
        assert_eq!(cache.get(&ctx, path), None);
        ready.recv_timeout(Duration::from_secs(2)).unwrap();
        let at = Instant::now();
        for _ in 0..100 {
            assert_eq!(cache.get(&ctx, path), None);
        }
        assert!(at.elapsed() < Duration::from_millis(200));
        assert_eq!(cache.pending.len(), 1);
        for i in 0..100 {
            cache.get(&ctx, Path::new(&format!("queued-{i}")));
        }
        assert!(cache.pending.len() <= 17, "后台请求必须有上限");
        release.send(()).unwrap();
    }
}
