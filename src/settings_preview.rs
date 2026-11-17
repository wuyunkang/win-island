//! 设置中的主题缩略图、标签和只读预览。
use crate::{
    app::App,
    appearance::{Appearance, PanelTheme},
    extras::{settings_card, settings_hint, settings_label},
};
use eframe::egui::{self, Color32, Pos2, Rect, Vec2};

pub(crate) fn badge(ui: &mut egui::Ui, text: &str, color: Color32) {
    egui::Frame::new()
        .fill(color.gamma_multiply(if ui.visuals().dark_mode { 0.18 } else { 0.08 }))
        .corner_radius(5.)
        .inner_margin(egui::Margin::symmetric(7, 3))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 5.;
                let (dot, _) = ui.allocate_exact_size(Vec2::splat(6.), egui::Sense::hover());
                ui.painter().circle_filled(dot.center(), 3., color);
                ui.label(
                    egui::RichText::new(text)
                        .size(10.5)
                        .color(ui.visuals().text_color()),
                );
            });
        });
}

impl Appearance {
    pub(crate) fn theme_cards(&mut self, ui: &mut egui::Ui) {
        settings_hint(
            ui,
            "点击卡片立即应用。缩略图展示背景、文字与强调色，下方可查看实际功能页。",
        );
        let columns = if ui.available_width() >= 480. { 3 } else { 2 };
        let gap = 10.;
        let width = (ui.available_width() - gap * (columns - 1) as f32) / columns as f32;
        for (heading, themes) in [
            ("基础主题", &PanelTheme::ALL[..2]),
            ("主题预设", &PanelTheme::ALL[2..10]),
            ("自定义", &PanelTheme::ALL[10..]),
        ] {
            settings_label(ui, heading);
            for row in themes.chunks(columns) {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = gap;
                    for theme in row {
                        let selected = self.theme == *theme;
                        let mut appearance = self.clone();
                        appearance.theme = *theme;
                        let (bg, fg, accent) = appearance.colors();
                        let color = |v: [u8; 3]| Color32::from_rgb(v[0], v[1], v[2]);
                        let (rect, _) =
                            ui.allocate_exact_size(Vec2::new(width, 116.), egui::Sense::hover());
                        let stroke = if selected {
                            ui.visuals().selection.stroke.color
                        } else {
                            ui.visuals().widgets.noninteractive.bg_stroke.color
                        };
                        ui.painter()
                            .rect_filled(rect, 9., ui.visuals().extreme_bg_color);
                        ui.painter().rect_stroke(
                            rect,
                            9.,
                            egui::Stroke::new(if selected { 1.8_f32 } else { 1.0_f32 }, stroke),
                            egui::StrokeKind::Inside,
                        );
                        let preview = Rect::from_min_size(
                            rect.min + egui::vec2(8., 8.),
                            egui::vec2(width - 16., 51.),
                        );
                        ui.painter().rect_filled(preview, 6., color(bg));
                        ui.painter().text(
                            preview.min + egui::vec2(10., 13.),
                            egui::Align2::LEFT_CENTER,
                            "09:41",
                            egui::FontId::proportional(11.),
                            color(fg),
                        );
                        let chip = Rect::from_min_size(
                            preview.min + egui::vec2(10., 30.),
                            egui::vec2(width - 36., 9.),
                        );
                        ui.painter()
                            .rect_filled(chip, 4., color(fg).gamma_multiply(0.12));
                        ui.painter().rect_filled(
                            Rect::from_min_size(chip.min, egui::vec2(chip.width() * 0.62, 9.)),
                            4.,
                            color(accent),
                        );
                        ui.painter().text(
                            rect.min + egui::vec2(10., 74.),
                            egui::Align2::LEFT_CENTER,
                            theme.label(),
                            egui::FontId::proportional(12.),
                            ui.visuals().text_color(),
                        );
                        ui.painter().text(
                            rect.min + egui::vec2(10., 99.),
                            egui::Align2::LEFT_CENTER,
                            if selected {
                                "已选用"
                            } else if bg.iter().map(|v| *v as u32).sum::<u32>() > 384 {
                                "浅色"
                            } else {
                                "深色"
                            },
                            egui::FontId::proportional(10.),
                            ui.visuals().weak_text_color(),
                        );
                        for (i, rgb) in [bg, fg, accent].into_iter().enumerate() {
                            ui.painter().circle_filled(
                                rect.right_bottom() + egui::vec2(-41. + i as f32 * 12., -17.),
                                3.5,
                                color(rgb),
                            );
                        }
                        let response = ui
                            .interact(
                                rect,
                                egui::Id::new(("theme-choice", theme.label())),
                                egui::Sense::click(),
                            )
                            .on_hover_text(theme.description())
                            .on_hover_cursor(egui::CursorIcon::PointingHand);
                        if response.clicked() {
                            self.theme = *theme;
                        }
                    }
                });
                ui.add_space(3.);
            }
        }
        if self.theme == PanelTheme::Custom {
            settings_label(ui, "自定义颜色");
            settings_card(ui, |ui| {
                for (label, value) in [
                    ("背景", &mut self.background),
                    ("文字", &mut self.foreground),
                    ("强调色", &mut self.accent),
                ] {
                    crate::extras::settings_control_row_right(ui, label, |ui| {
                        let mut rgb = crate::transfer::color(value).unwrap_or([128; 3]);
                        if ui.color_edit_button_srgb(&mut rgb).changed() {
                            *value = format!("#{:02X}{:02X}{:02X}", rgb[0], rgb[1], rgb[2]);
                        }
                        let id = ui.id().with(("custom-theme-hex", label));
                        let mut draft = ui
                            .data(|d| d.get_temp::<String>(id))
                            .unwrap_or_else(|| value.clone());
                        let response =
                            ui.add(egui::TextEdit::singleline(&mut draft).desired_width(92.));
                        if response.changed() {
                            if let Some(rgb) = crate::transfer::color(&draft) {
                                *value = format!("#{:02X}{:02X}{:02X}", rgb[0], rgb[1], rgb[2]);
                            }
                        }
                        if response.has_focus() {
                            ui.data_mut(|d| d.insert_temp(id, draft));
                        } else {
                            ui.data_mut(|d| d.remove::<String>(id));
                        }
                    });
                }
                settings_hint(
                    ui,
                    "支持颜色选择器和 #RRGGBB；无效输入不会覆盖已保存的颜色。",
                );
            });
        }
    }
}

impl App {
    pub(crate) fn appearance_preview(&mut self, ui: &mut egui::Ui) {
        settings_label(ui, "实时预览");
        ui.horizontal_wrapped(|ui| {
            let accent = self.settings.appearance.colors().2;
            badge(
                ui,
                self.settings.appearance.theme.label(),
                Color32::from_rgb(accent[0], accent[1], accent[2]),
            );
            badge(
                ui,
                self.settings.skin.label(),
                Color32::from_rgb(145, 158, 180),
            );
            badge(
                ui,
                self.settings.island_shape.label(),
                Color32::from_rgb(145, 158, 180),
            );
            badge(
                ui,
                &format!("{}%", self.settings.island_size),
                Color32::from_rgb(145, 158, 180),
            );
        });
        self.feature_page_preview(ui, if self.page == 2 { 0 } else { self.page });
        settings_hint(
            ui,
            "预览随主题、皮肤和形状更新；整体缩放应用于桌面岛体。毛玻璃背景以示意方式显示。",
        );
    }
    pub(crate) fn theme_settings(&mut self, ui: &mut egui::Ui) {
        self.appearance_preview(ui);
        self.settings.appearance.theme_cards(ui);
    }
    pub(crate) fn notification_settings_preview(&self, ui: &mut egui::Ui) {
        settings_label(ui, "提醒预览");
        let colors = self.settings.appearance.colors();
        let (stage, _) =
            ui.allocate_exact_size(egui::vec2(ui.available_width(), 94.), egui::Sense::hover());
        ui.painter()
            .rect_filled(stage, 9., Color32::from_rgb(22, 29, 42));
        let card = Rect::from_center_size(
            stage.center(),
            egui::vec2((stage.width() - 28.).min(360.), 68.),
        );
        let rgb = |v: [u8; 3]| Color32::from_rgb(v[0], v[1], v[2]);
        ui.painter().rect_filled(card, 14., rgb(colors.0));
        ui.painter()
            .circle_filled(card.min + egui::vec2(26., 28.), 12., rgb(colors.2));
        ui.painter().text(
            card.min + egui::vec2(26., 28.),
            egui::Align2::CENTER_CENTER,
            "铃",
            egui::FontId::proportional(10.),
            rgb(colors.0),
        );
        ui.painter().text(
            card.min + egui::vec2(48., 22.),
            egui::Align2::LEFT_CENTER,
            "示例通知",
            egui::FontId::proportional(12.),
            rgb(colors.1),
        );
        let text = if !self.settings.notifications {
            "提醒已关闭"
        } else if self.settings.notification_preview {
            "这是一条演示消息"
        } else {
            "收到新消息 · 正文已隐藏"
        };
        ui.painter().text(
            card.min + egui::vec2(48., 44.),
            egui::Align2::LEFT_CENTER,
            text,
            egui::FontId::proportional(11.),
            rgb(colors.1).gamma_multiply(0.7),
        );
        ui.horizontal_wrapped(|ui| {
            badge(ui, "示例数据", Color32::from_rgb(145, 158, 180));
            badge(
                ui,
                if self.settings.notification_preview {
                    "显示正文"
                } else {
                    "隐藏正文"
                },
                Color32::from_rgb(104, 185, 255),
            );
        });
    }
    pub(crate) fn dock_settings_preview(&self, ui: &mut egui::Ui) {
        let (stage, _) =
            ui.allocate_exact_size(egui::vec2(ui.available_width(), 85.), egui::Sense::hover());
        let screen = Rect::from_center_size(
            stage.center(),
            egui::vec2(stage.width().min(260.) - 20., 68.),
        );
        ui.painter()
            .rect_filled(screen, 7., ui.visuals().extreme_bg_color);
        ui.painter().rect_stroke(
            screen,
            7.,
            egui::Stroke::new(1.0_f32, ui.visuals().widgets.noninteractive.bg_stroke.color),
            egui::StrokeKind::Inside,
        );
        let shift = self.settings.island_offset as f32 / crate::model::ISLAND_OFFSET_MAX as f32;
        let scale = self.settings.island_scale();
        let (center, size) = match self.settings.dock {
            crate::model::Dock::Top => (
                Pos2::new(
                    screen.center().x + shift * (screen.width() / 2. - 30. * scale),
                    screen.top() + 4. * scale,
                ),
                egui::vec2(52. * scale, 8. * scale),
            ),
            crate::model::Dock::Left => (
                Pos2::new(
                    screen.left() + 4. * scale,
                    screen.center().y + shift * (screen.height() / 2. - 15. * scale),
                ),
                egui::vec2(8. * scale, 30. * scale),
            ),
            crate::model::Dock::Right => (
                Pos2::new(
                    screen.right() - 4. * scale,
                    screen.center().y + shift * (screen.height() / 2. - 15. * scale),
                ),
                egui::vec2(8. * scale, 30. * scale),
            ),
        };
        ui.painter().rect_filled(
            Rect::from_center_size(center, size),
            4.,
            ui.visuals().selection.stroke.color,
        );
        ui.painter().text(
            screen.center() + egui::vec2(0., 8.),
            egui::Align2::CENTER_CENTER,
            format!("{} · 位置预览", self.settings.dock.label()),
            egui::FontId::proportional(10.),
            ui.visuals().weak_text_color(),
        );
    }
}
