//! 胶囊与展开态分别控制；按实际可见视图申请资源，提醒独立工作。
use crate::{
    app::App,
    bar_layout::{self as bar, Layout},
    paint::{Card, H},
    theme::{self, W},
};
use eframe::egui::Color32;

pub(crate) const FEATURE_PAGES: [usize; 9] = [0, 1, 3, 4, 5, 6, 7, 8, 9];
pub(crate) fn settings_page(tab: usize) -> Option<usize> {
    match tab {
        1 => Some(0),
        2 => Some(1),
        4 => Some(4),
        5 => Some(3),
        7 => Some(5),
        8 => Some(7),
        11 => Some(6),
        12 => Some(8),
        13 => Some(9),
        _ => None,
    }
}
pub(crate) fn feature_slot(page: usize) -> Option<usize> {
    match page {
        3 => Some(bar::CLIPBOARD),
        4 => Some(bar::TODO),
        5 => Some(bar::MAIL),
        7 => Some(bar::CALENDAR),
        9 => Some(bar::MONITOR),
        _ => None,
    }
}
impl App {
    pub(crate) fn settings_preview_needed(&self,page:usize)->bool {
        self.settings_open && (settings_page(self.settings_tab)==Some(page)||([3,14].contains(&self.settings_tab)&&self.page==page))
    }
    pub(crate) fn feature_view_needed(&self,page:usize)->bool {
        self.expanded_page_visible(page)||self.settings_preview_needed(page)
    }
    /// 只有正在显示的音乐页面、胶囊或设置预览需要媒体监听。
    pub fn music_polling_needed(&self) -> bool {
        self.feature_view_needed(0)||self.capsule_enabled(0)
    }
    pub fn weather_polling_needed(&self)->bool {
        self.feature_view_needed(8)||self.capsule_enabled(8)
    }
    pub fn quota_polling_needed(&self)->bool {
        let view=(self.expanded_page_visible(6)&&self.ai.config.quota_tab)||(self.settings_open&&(self.settings_tab==11||([3,14].contains(&self.settings_tab)&&self.page==6&&self.ai.config.quota_tab)));
        let capsule=self.capsule_enabled(6)&&!self.open;
        self.quota.config.enabled&&(view||capsule||self.quota.connect_open||self.quota.login.is_some()||self.quota.workbuddy_connecting.is_some()||self.quota.deepseek_connecting.is_some()||self.quota.gemini_connecting.is_some())
    }
    pub(crate) fn music_artwork_needed(&self)->bool {
        self.feature_view_needed(0)||self.island_lyrics_active()||self.settings.skin==crate::model::Skin::Fluid
    }
    pub(crate) fn music_lyrics_needed(&self)->bool {
        (self.feature_view_needed(0)&&self.show_lyrics)||self.island_lyrics_active()
    }
    pub(crate) fn capsule_enabled(&self, page: usize) -> bool {
        match page {
            0 | 1 | 3 | 4 | 5 | 7 | 9 => self.settings.capsule_pages[page],
            6 => self.settings.capsule_pages[6] && self.quota.config.compact,
            8 => self.settings.show_weather,
            _ => false,
        }
    }
    pub(crate) fn set_capsule_enabled(&mut self, page: usize, enabled: bool) {
        match page {
            0 | 1 | 3 | 4 | 5 | 7 | 9 => self.settings.capsule_pages[page] = enabled,
            6 => {
                self.settings.capsule_pages[6] = enabled;
                self.quota.config.compact = enabled;
                self.quota.config.save();
            }
            8 => {
                self.settings.show_weather = enabled;
                self.settings.capsule_pages[8] = enabled;
            }
            _ => {}
        }
    }
    /// 使用与主界面相同的条件和顺序，供预览与真实布局共同使用。
    pub(crate) fn capsule_layout_for_preview(&self) -> Layout {
        let bell = self.t < self.notification_bell_until && !self.compact_notices();
        let battery = self.settings.show_battery && self.resident_power.0.is_some();
        let mut layout = Layout::new(
            battery,
            self.quota_bar_indices().len(),
            self.media.is_music() && self.capsule_enabled(0) && !bell,
            self.pomo.running && self.capsule_enabled(1),
            bell,
            self.pet_enabled(),
        );
        if self.capsule_enabled(8) {
            layout.add_weather();
        }
        for page in [3, 4, 5, 7, 9] {
            if self.capsule_enabled(page) {
                layout.add_feature(feature_slot(page).unwrap());
            }
        }
        if self.ai_capsule_chip(){
            layout.add_feature(bar::AGENT);
            let reserve=if self.settings.clock_period!=0{40.0}else{0.0};
            let preferred=self.ai.capsule_width(self.t);
            let minimum=if preferred>160. {168.} else if preferred>27. {80.} else {27.};
            let width=preferred.min((crate::app::WIN_W-32.-reserve-layout.width+27.).max(minimum));
            layout.widen_slot(bar::AGENT,width-27.);
        }
        let default = bar::default_order(battery);
        let order = if bar::valid_order(&self.settings.resident_order) {
            self.settings.resident_order.as_slice()
        } else {
            &default
        };
        layout.reorder(order);
        layout
    }
    /// None 表示数据源尚无可用状态，不显示可能误导的 0。
    pub(crate) fn capsule_count(&self, page: usize, today: &str) -> Option<usize> {
        match page {
            3 => Some(self.clipboard.lock().items.len()),
            4 => {
                let (done, total) = self.todos.progress(today);
                Some(total.saturating_sub(done))
            }
            5 => {
                let states: Vec<_> = self
                    .mail
                    .recent
                    .iter()
                    .filter_map(|item| self.mail_seen(item, crate::mail_manage::Folder::Inbox))
                    .collect();
                (!states.is_empty()).then(|| states.iter().filter(|seen| !**seen).count())
            }
            7 => chrono::NaiveDate::parse_from_str(today, "%Y-%m-%d")
                .ok()
                .map(|day| self.calendar_entries(day).len()),
            _ => None,
        }
    }
    pub(crate) fn draw_capsule_feature(
        &mut self,
        c: &Card,
        page: usize,
        x: f32,
        y: f32,
        today: &str,
    ) {
        if page==9 {
            self.draw_monitor_compact(c,x,y);
            if c.hit(x-35.,y-12.,70.,24.,("capsule-feature",page)).on_hover_text("系统监控 · CPU / 内存").clicked(){self.show_page(9);}
            return;
        }
        let count = self.capsule_count(page, today);
        let color = match page {
            3 => Color32::from_rgb(255, 191, 85),
            4 => Color32::from_rgb(67, 214, 152),
            5 => Color32::from_rgb(85, 161, 255),
            _ => Color32::from_rgb(87, 210, 222),
        };
        let ix = if count.is_some() { x - 9.0 } else { x };
        crate::feature_tabs::icon(c, page, ix, y, color);
        if let Some(count) = count {
            let value = if count > 99 {
                "99+".to_owned()
            } else {
                count.to_string()
            };
            c.centered_text(
                x + 9.0,
                y,
                &value,
                9.5,
                W::Semibold,
                theme::WHITE,
                H::Center,
            );
        }
        let label = match page {
            3 => "剪贴板记录",
            4 => "今天未完成的待办",
            5 => "当前已加载且状态已知的未读邮件",
            7 => "今日日程",
            _ => "功能",
        };
        let tip = match count {
            Some(n) => format!("{label}：{n}"),
            None => format!("{label}：暂无可用状态"),
        };
        if c.hit(x - 19.0, y - 12.0, 38.0, 24.0, ("capsule-feature", page))
            .on_hover_text(tip)
            .clicked()
        {
            self.show_page(page);
        }
    }
}
