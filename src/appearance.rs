//! 面板主题、展开尺寸与交互配置。
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum PanelTheme {
    #[default]
    Dark,
    Light,
    OneDark,
    GitHubDark,
    GitHubLight,
    Dracula,
    NightOwl,
    Cyberpunk,
    IslandHoliday,
    Nord,
    Custom,
}
impl PanelTheme {
    pub const ALL: [Self; 11] = [
        Self::Dark,
        Self::Light,
        Self::OneDark,
        Self::GitHubDark,
        Self::GitHubLight,
        Self::Dracula,
        Self::NightOwl,
        Self::Cyberpunk,
        Self::IslandHoliday,
        Self::Nord,
        Self::Custom,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Dark => "深色",
            Self::Light => "浅色",
            Self::OneDark => "One Dark Pro",
            Self::GitHubDark => "GitHub Dark",
            Self::GitHubLight => "GitHub Light",
            Self::Dracula => "Dracula",
            Self::NightOwl => "Night Owl",
            Self::Cyberpunk => "赛博霓虹",
            Self::IslandHoliday => "海岛假日",
            Self::Nord => "Nord 极地",
            Self::Custom => "自定义",
        }
    }
    pub fn description(self)->&'static str {
        match self {
            Self::Dark=>"纯黑背景 · 清晰的系统蓝",
            Self::Light=>"柔白背景 · 适合明亮桌面",
            Self::OneDark=>"石墨灰 · 柔和蓝色",
            Self::GitHubDark=>"深邃墨色 · 通透蓝色",
            Self::GitHubLight=>"纯白底色 · 鲜明蓝色",
            Self::Dracula=>"午夜紫灰 · 薰衣草紫",
            Self::NightOwl=>"深海蓝 · 温柔的薄荷绿",
            Self::Cyberpunk=>"暗紫背景 · 明亮霓虹粉",
            Self::IslandHoliday=>"奶油白 · 热带青绿",
            Self::Nord=>"极地灰蓝 · 冰川青色",
            Self::Custom=>"自由组合背景、文字和强调色",
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Appearance {
    pub theme: PanelTheme,
    pub background: String,
    pub foreground: String,
    pub accent: String,
    pub transparency: u8,
    pub clipboard_width: u32,
    pub clipboard_height: u32,
    pub font: String,
    pub open_delay: u32,
    pub close_delay: u32,
    pub click_to_expand: bool,
    pub animations: bool,
    pub frame_rate: u32,
}
impl Default for Appearance {
    fn default() -> Self {
        Self {
            theme: PanelTheme::Dark,
            background: "#16161A".into(),
            foreground: "#F2F2F7".into(),
            accent: "#9CB9FF".into(),
            transparency: 0,
            clipboard_width: 520,
            clipboard_height: 240,
            font: "Segoe UI".into(),
            open_delay: 200,
            close_delay: 120,
            click_to_expand: false,
            animations: true,
            frame_rate: 60,
        }
    }
}
impl Appearance {
    pub(crate) fn settings(&mut self, ui: &mut eframe::egui::Ui) {
        use crate::extras::{
            setting_number, setting_row, settings_card, settings_control_row_right as row,
            settings_hint, settings_label, settings_segmented as segmented,
        };
        use eframe::egui::{ComboBox, Slider};
        settings_label(ui, "主题与文字");
        settings_card(ui, |ui| {
            row(ui, "面板主题", |ui| {
                ui.label(self.theme.label())
            });
            if self.theme == PanelTheme::Custom {
                for (label, value) in [
                    ("背景", &mut self.background),
                    ("文字", &mut self.foreground),
                    ("强调色", &mut self.accent),
                ] {
                    row(ui, label, |ui| {
                        let mut rgb = crate::transfer::color(value).unwrap_or([128; 3]);
                        let r = ui.color_edit_button_srgb(&mut rgb);
                        if r.changed() {
                            *value = format!("#{:02X}{:02X}{:02X}", rgb[0], rgb[1], rgb[2]);
                        }
                        r
                    });
                }
            }
            ui.separator();
            row(ui, "背景透明度", |ui| {
                ui.spacing_mut().slider_width = 140.;
                ui.add(Slider::new(&mut self.transparency, 0..=90).suffix("%"))
            });
            ui.separator();
            row(ui, "字体", |ui| {
                ComboBox::from_id_salt("panel-font")
                    .selected_text(&self.font)
                    .width(165.)
                    .show_ui(ui, |ui| {
                        for font in ["Segoe UI", "微软雅黑", "等线"] {
                            ui.selectable_value(&mut self.font, font.into(), font);
                        }
                    })
                    .response
            });
            settings_hint(
                ui,
                "中转站与普通页面保持同一尺寸；整体大小使用上方统一缩放设置。",
            );
        });
        settings_label(ui, "展开与动画");
        settings_card(ui, |ui| {
            row(ui, "展开方式", |ui| {
                segmented(
                    ui,
                    &mut self.click_to_expand,
                    &[(false, "悬停"), (true, "点击")],
                    68.,
                );
                ui.response()
            });
            ui.separator();
            setting_number(ui, "展开延迟", &mut self.open_delay, 0, 2000, " ms", 50.);
            ui.separator();
            setting_number(ui, "收起延迟", &mut self.close_delay, 0, 3000, " ms", 50.);
            ui.separator();
            setting_row(
                ui,
                "展开/收起动画",
                "关闭后直接切换展开与收起状态",
                &mut self.animations,
            );
            ui.separator();
            row(ui, "动画帧率", |ui| {
                segmented(
                    ui,
                    &mut self.frame_rate,
                    &[(30, "30 FPS"), (60, "60 FPS"), (120, "120 FPS")],
                    68.,
                );
                ui.response()
            });
        });
        self.normalize();
    }
    pub fn normalize(&mut self) {
        self.clipboard_width = crate::app::PANEL_W as u32;
        self.clipboard_height = crate::app::PANEL_H as u32;
        self.transparency = self.transparency.min(90);
        self.open_delay = self.open_delay.min(2000);
        self.close_delay = self.close_delay.min(3000);
        self.frame_rate = match self.frame_rate {
            30 | 120 => self.frame_rate,
            _ => 60,
        };
        for (value, default) in [
            (&mut self.background, "#16161A"),
            (&mut self.foreground, "#F2F2F7"),
            (&mut self.accent, "#9CB9FF"),
        ] {
            if crate::transfer::color(value).is_none() {
                *value = default.into();
            }
        }
        if self.font.trim().is_empty() {
            self.font = "Segoe UI".into();
        }
    }
    pub fn colors(&self) -> ([u8; 3], [u8; 3], [u8; 3]) {
        match self.theme {
            PanelTheme::Dark => ([0, 0, 0], [255, 255, 255], [62, 172, 248]),
            PanelTheme::Light => ([247, 248, 250], [31, 35, 40], [10, 112, 230]),
            PanelTheme::OneDark => ([40, 44, 52], [171, 178, 191], [97, 175, 239]),
            PanelTheme::GitHubDark => ([13, 17, 23], [230, 237, 243], [88, 166, 255]),
            PanelTheme::GitHubLight => ([255, 255, 255], [31, 35, 40], [9, 105, 218]),
            PanelTheme::Dracula => ([40, 42, 54], [248, 248, 242], [189, 147, 249]),
            PanelTheme::NightOwl => ([1, 22, 39], [214, 222, 235], [127, 219, 202]),
            PanelTheme::Cyberpunk => ([25, 17, 39], [249, 241, 255], [255, 100, 213]),
            PanelTheme::IslandHoliday => ([255, 248, 233], [42, 60, 58], [14, 128, 119]),
            PanelTheme::Nord => ([46, 52, 64], [236, 239, 244], [136, 192, 208]),
            PanelTheme::Custom => (
                crate::transfer::color(&self.background).unwrap_or([22, 22, 26]),
                crate::transfer::color(&self.foreground).unwrap_or([242, 242, 247]),
                crate::transfer::color(&self.accent).unwrap_or([156, 185, 255]),
            ),
        }
    }
    pub fn palette(&self) -> Option<crate::skin_paint::Palette> {
        if self.theme == PanelTheme::Dark {
            return None;
        }
        let (paper, ink, accent) = self.colors();
        let light = paper.iter().map(|v| *v as u32).sum::<u32>() > 384;
        Some(crate::skin_paint::Palette {
            paper,
            ink,
            accent: Some(accent),
            dark_alpha: 1.0,
            lift: 1.0,
            darken: if light { 0.78 } else { 1.0 },
            chip: 1.0,
        })
    }
}
#[cfg(test)]
mod tests {
    #[test]
    fn appearance_ranges_and_custom_colors_are_validated() {
        let mut a = super::Appearance {
            clipboard_width: 0,
            clipboard_height: u32::MAX,
            transparency: 255,
            background: "invalid".into(),
            ..Default::default()
        };
        a.normalize();
        assert_eq!(
            (a.clipboard_width, a.clipboard_height, a.transparency),
            (520, 240, 90)
        );
        assert_eq!(a.background, "#16161A");
        a.theme = super::PanelTheme::Light;
        let p = a.palette().unwrap();
        assert!(p.paper[0] > p.ink[0]);
    }
}
