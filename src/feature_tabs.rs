//! 展开区域左侧图标选项卡，遵循页面开关与排序。
use crate::{
    app::{App, PAGE_NAMES},
    paint::{self, Card},
    theme,
};
use eframe::egui::Color32;
pub(crate) const TAB_TOP: f32 = 34.0;
pub(crate) const TAB_STEP: f32 = 22.0;
pub(crate) const TAB_CENTER: f32 = 45.0;
pub(crate) fn accent(page: usize) -> Color32 {
    match page {
        0 => Color32::from_rgb(72, 183, 255),
        1 => Color32::from_rgb(255, 100, 116),
        3 => Color32::from_rgb(255, 191, 85),
        4 => Color32::from_rgb(67, 214, 152),
        5 => Color32::from_rgb(85, 161, 255),
        6 => Color32::from_rgb(179, 132, 255),
        9 => Color32::from_rgb(103, 221, 176),
        _ => Color32::from_rgb(87, 210, 222),
    }
}
pub(crate) fn icon(c: &Card, page: usize, x: f32, y: f32, ink: Color32) {
    match page {
        0 => paint::note(c, x, y, 16.0, ink),
        // 小尺寸使用等线宽图标，避免原大秒表的固定表冠挤到相邻入口。
        1 => {
            c.ring(x, y + 1.0, 6.0, 1.3, ink);
            c.seg((x - 2.5, y - 7.0), (x + 2.5, y - 7.0), 1.3, ink);
            c.seg((x, y - 7.0), (x, y - 5.0), 1.3, ink);
            c.path(&[(x, y - 2.0), (x, y + 1.0), (x + 2.5, y + 2.0)], 1.3, ink);
        }
        3 => paint::clipboard_glyph(c, x, y - 0.5, 13.0, ink, Color32::TRANSPARENT),
        4 => {
            for dy in [-5.0, 0.0, 5.0] {
                c.path(
                    &[
                        (x - 7.0, y + dy),
                        (x - 5.5, y + dy + 1.5),
                        (x - 3.5, y + dy - 1.5),
                    ],
                    1.2,
                    ink,
                );
                c.seg((x - 0.5, y + dy), (x + 6.5, y + dy), 1.2, ink);
            }
        }
        5 => {
            c.stroke(x - 7.0, y - 5.0, 14.0, 10.0, 2.0, 1.3, ink);
            c.path(
                &[(x - 6.0, y - 3.5), (x, y + 0.5), (x + 6.0, y - 3.5)],
                1.3,
                ink,
            );
        }
        6 => {
            c.ring(x, y, 7.0, 1.3, ink);
            c.path(
                &[(x - 3.0, y + 3.0), (x, y - 3.0), (x + 3.0, y + 3.0)],
                1.2,
                ink,
            );
            c.seg((x - 2.0, y + 1.0), (x + 2.0, y + 1.0), 1.2, ink);
        }
        7 => {
            c.stroke(x - 7.0, y - 5.0, 14.0, 12.0, 2.0, 1.3, ink);
            c.seg((x - 7.0, y - 1.0), (x + 7.0, y - 1.0), 1.2, ink);
            for dx in [-3.0, 3.0] {
                c.seg((x + dx, y - 7.0), (x + dx, y - 3.0), 1.3, ink);
            }
            for dx in [-3.0, 2.0] {
                c.circle(x + dx, y + 3.0, 0.9, ink);
            }
        }
        8 => { c.ring(x, y, 4.5, 1.3, ink); for (dx,dy) in [(0.0,-8.0),(0.0,8.0),(-8.0,0.0),(8.0,0.0)] { c.circle(x+dx,y+dy,1.0,ink); } }
        9 => { c.stroke(x-7.0,y-6.0,14.0,10.0,2.0,1.2,ink);c.path(&[(x-5.0,y+1.0),(x-2.0,y-2.0),(x,y+1.0),(x+4.0,y-3.0)],1.2,ink);c.seg((x,y+4.0),(x,y+7.0),1.2,ink);c.seg((x-4.0,y+7.0),(x+4.0,y+7.0),1.2,ink); }
        _ => {}
    }
}
impl App {
    pub(crate) fn feature_tabs(&mut self, c: &Card) {
        let tabs: Vec<_> = self
            .settings
            .page_order
            .into_iter()
            .filter(|p| *p != 2 && self.settings.visible_pages[*p])
            .collect();
        for (i, page) in tabs.into_iter().enumerate() {
            let y = TAB_TOP + i as f32 * TAB_STEP;
            let selected = self.page == page;
            let hit = c
                .hit(10.0, y, 30.0, 22.0, ("feature-tab", page))
                .on_hover_text(PAGE_NAMES[page]);
            let color = accent(page);
            if selected || hit.hovered() {
                c.fill(
                    10.0,
                    y,
                    30.0,
                    22.0,
                    6.0,
                    if selected {
                        Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 40)
                    } else {
                        theme::BTN
                    },
                );
            }
            icon(
                c,
                page,
                25.0,
                y + 11.0,
                if selected {
                    color
                } else {
                    Color32::from_gray(153)
                },
            );
            if hit.clicked() {
                self.show_page(page);
                self.cancel_edit();
                self.subpage = None;
                self.transfer_ui.detail = None;
            }
        }
        c.fill(
            49.0,
            35.0,
            0.6,
            self.expanded_height() - 49.0,
            0.0,
            Color32::from_gray(40),
        );
    }
}
