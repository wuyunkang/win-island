//! 在设置页以只读方式复用真实的展开页绘制。
use crate::{
    app::{App, Env, CONTENT_X, FLARE, FLARE_H, PANEL_H, PANEL_W, RAD},
    paint::{self, Card, H},
    theme::{self, W},
};
use chrono::Datelike;
use eframe::egui::{self, Color32, Pos2, Rect, Vec2};

impl App {
    /// 与主界面相同的轮廓；毛玻璃预览不模拟真实背景模糊。
    pub(crate) fn paint_feature_preview_shell(
        &self,
        painter: &egui::Painter,
        rect: Rect,
        scale: f32,
    ) {
        let pill = self.settings.island_shape == crate::model::IslandShape::Pill;
        let fill = self.preview_fill();
        let radius = if pill { 26.0 * scale } else { RAD * scale };
        if pill {
            painter.rect_filled(rect, radius, fill);
        } else if self.side() {
            crate::shape::fill_docked(
                painter,
                rect,
                self.dock(),
                FLARE * scale,
                FLARE_H * scale,
                radius,
                fill,
                1.0 / painter.ctx().pixels_per_point(),
            );
        } else {
            crate::shape::fill(
                painter,
                rect,
                FLARE * scale,
                FLARE_H * scale,
                radius,
                fill,
                1.0 / painter.ctx().pixels_per_point(),
            );
        }
        if self.settings.skin.is_glass() {
            let rim = egui::Stroke::new(scale.max(0.5), Color32::from_white_alpha(60));
            if pill {
                painter.rect_stroke(rect, radius, rim, egui::StrokeKind::Inside);
            } else {
                let edge = rect.shrink(0.5 * scale);
                let points = if self.side() {
                    crate::shape::outline_docked(
                        edge,
                        self.dock(),
                        FLARE * scale,
                        FLARE_H * scale,
                        (radius - 0.5 * scale).max(0.0),
                    )
                } else {
                    crate::shape::outline(
                        edge,
                        FLARE * scale,
                        FLARE_H * scale,
                        (radius - 0.5 * scale).max(0.0),
                    )
                };
                painter.add(egui::Shape::line(points, rim));
            }
        }
    }
    pub(crate) fn feature_page_preview(&mut self, ui: &mut egui::Ui, page: usize) {
        let available = ui.available_width().max(1.0);
        let outer_width = if self.settings.island_shape == crate::model::IslandShape::Pill {
            PANEL_W
        } else {
            PANEL_W + 2.0 * FLARE
        };
        let scale = ((available - 12.0) / outer_width).min(0.85).max(0.2);
        let size = Vec2::new(PANEL_W * scale, PANEL_H * scale);
        let side_pad =
            if self.settings.island_shape != crate::model::IslandShape::Pill && self.side() {
                FLARE * scale + 2.0
            } else {
                0.0
            };
        let (stage, _) = ui.allocate_exact_size(
            Vec2::new(available, size.y + 12.0 + 2.0 * side_pad),
            egui::Sense::hover(),
        );
        ui.painter()
            .rect_filled(stage, 9.0, Color32::from_rgb(19, 25, 36));
        let origin = Pos2::new(
            stage.center().x - size.x / 2.0,
            stage.top() + 6.0 + side_pad,
        );
        let panel = Rect::from_min_size(origin, size);
        self.paint_feature_preview_shell(ui.painter(), panel, scale);
        let from = crate::skin_paint::shape_count(ui.ctx(), ui.layer_id());
        let mut preview = egui::Ui::new(
            ui.ctx().clone(),
            ui.id().with(("feature-page-preview", page)),
            egui::UiBuilder::new()
                .max_rect(panel)
                .layer_id(ui.layer_id())
                .style(ui.style().clone()),
        );
        preview.set_clip_rect(panel.intersect(ui.clip_rect()));
        preview.disable();
        let mut card = Card::new(&preview, origin, preview.clip_rect()).ghost();
        // 保持只读，但按实际颜色绘制，避免把预览误解为不可用功能。
        card.painter.set_fade_to_color(None);
        card.s = scale;
        let now = ui
            .ctx()
            .data(|d| {
                d.get_temp::<chrono::DateTime<chrono::Local>>(egui::Id::new(
                    "settings-preview-time",
                ))
            })
            .unwrap_or_else(chrono::Local::now);
        let env = Env {
            t: self.t,
            clock: now.format("%H:%M:%S").to_string(),
            today: now.format("%Y-%m-%d").to_string(),
            date: format!(
                "{}月{}日 周{}",
                now.month(),
                now.day(),
                ["一", "二", "三", "四", "五", "六", "日"]
                    [now.weekday().num_days_from_monday() as usize]
            ),
            media: self.media.clone(),
            battery: self.resident_power.0,
            charging: self.resident_power.1,
            ..Default::default()
        };
        let old_page = self.page;
        self.page = page;
        let visible = self.settings.visible_pages[page];
        self.settings.visible_pages[page] = true;
        self.feature_tabs(&card);
        card.centered_text(
            PANEL_W / 2.0,
            18.0,
            &format!("{}  {}", env.date, self.settings.clock_text(&env.clock)),
            11.0,
            W::Regular,
            theme::WHITE,
            H::Center,
        );
        paint::gear(&card, PANEL_W - 25.0, 18.0, theme::GRAY);
        let content =
            card.shifted(CONTENT_X, 0.0)
                .clipped(0.0, 0.0, crate::app::CONTENT_W, PANEL_H);
        if page == 3 {
            std::mem::swap(&mut self.transfer_ui, &mut self.preview_transfer_ui);
        }
        self.page_title(page, &content);
        self.render_page(page, &content, &env, &mut Vec::new());
        self.nav(&content);
        self.settings.visible_pages[page] = visible;
        if page == 3 {
            std::mem::swap(&mut self.transfer_ui, &mut self.preview_transfer_ui);
        }
        self.page = old_page;
        if let Some(palette) = self.skin_palette() {
            let end = crate::skin_paint::shape_count(ui.ctx(), ui.layer_id());
            crate::skin_paint::recolor_layer(ui.ctx(), ui.layer_id(), &[(from, end)], &palette);
        }
    }
}
