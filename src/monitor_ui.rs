//! 性能页和设置共享相同读数、配色与曲线。
use crate::{
    app::App,
    paint::{Card, H},
    system_monitor::{self as monitor, Palette, Reading},
    theme::{self, W},
};
use eframe::egui;

const COMPACT_ICON_WIDTH: f32 = 12.;
const COMPACT_TEXT_GAP: f32 = 4.;
const COMPACT_ITEM_GAP: f32 = 8.;

struct CompactReading {
    reading: Reading,
    value: String,
    width: f32,
}

#[derive(Clone)]
struct CompactWidthCache {
    atlas: std::sync::Arc<egui::epaint::mutex::Mutex<egui::epaint::TextureAtlas>>,
    widths: (f32, f32),
}

fn compact_width(items: &[CompactReading]) -> f32 {
    items.iter().map(|item| item.width).sum::<f32>()
        + items.len().saturating_sub(1) as f32 * COMPACT_ITEM_GAP
}

/// 统一使用矢量图标，在设置、展开页和胶囊内保持相同含义。
pub(crate) fn reading_icon(
    c: &Card,
    reading: Reading,
    x: f32,
    y: f32,
    size: f32,
    color: egui::Color32,
) {
    let icon = c.shifted(x, y).scaled(0., 0., size / 16.);
    let c = &icon;
    match reading {
        Reading::Cpu => {
            c.stroke(-5., -5., 10., 10., 2., 1.3, color);
            c.stroke(-2., -2., 4., 4., 0.5, 1., color);
            for n in [-3., 0., 3.] {
                for side in [-1., 1.] {
                    c.seg((n, side * 5.), (n, side * 7.5), 1.2, color);
                    c.seg((side * 5., n), (side * 7.5, n), 1.2, color);
                }
            }
        }
        Reading::Memory => {
            c.stroke(-7., -5., 14., 9., 1.5, 1.3, color);
            for n in [-4., 0., 4.] {
                c.stroke(n - 1., -2., 2., 3., 0.3, 1., color);
                c.seg((n, 4.), (n, 7.), 1.3, color);
            }
        }
        Reading::Gpu => {
            c.stroke(-7., -5., 14., 10., 1.5, 1.3, color);
            c.ring(0., 0., 3., 1.1, color);
            c.seg((-2., -2.), (2., 2.), 1.1, color);
            c.seg((-2., 2.), (2., -2.), 1.1, color);
            c.seg((-5., 5.), (-5., 7.), 1.3, color);
            c.seg((4., 5.), (4., 7.), 1.3, color);
        }
        Reading::Download | Reading::Upload => {
            let direction = if reading == Reading::Download {
                1.
            } else {
                -1.
            };
            c.seg((0., -6. * direction), (0., 3. * direction), 1.5, color);
            c.seg((-3.5, -0.5 * direction), (0., 3. * direction), 1.5, color);
            c.seg((3.5, -0.5 * direction), (0., 3. * direction), 1.5, color);
            c.seg((-6., 4.), (-6., 7.), 1.2, color);
            c.seg((-6., 7.), (6., 7.), 1.2, color);
            c.seg((6., 7.), (6., 4.), 1.2, color);
        }
        Reading::Storage => {
            c.stroke(-7., -5., 14., 11., 2., 1.3, color);
            c.seg((-6., 1.), (6., 1.), 1., color);
            c.circle(4., 3.5, 1., color);
            c.seg((-4., 3.5), (-1., 3.5), 1.1, color);
        }
    }
}

fn process_icon(c: &Card, x: f32, y: f32, color: egui::Color32) {
    c.stroke(x - 7., y - 6., 14., 12., 2., 1.2, color);
    for (offset, height) in [(-4., 4.), (0., 7.), (4., 3.)] {
        c.seg(
            (x + offset, y + 3.),
            (x + offset, y + 3. - height),
            1.4,
            color,
        );
    }
}

impl App {
    pub(crate) fn monitor_polling_needed(&self) -> bool {
        self.monitor_sampling_settings().has_readings()
    }
    pub(crate) fn monitor_sampling_settings(&self) -> monitor::Settings {
        self.settings.monitor.sampling_for_views(
            self.feature_view_needed(9),
            self.capsule_enabled(9) || (self.settings_open && self.settings_tab == 13),
        )
    }
    pub(crate) fn monitor_tick(&mut self, ctx: &egui::Context) {
        let settings = self.monitor_sampling_settings();
        self.monitor.update(ctx, settings.has_readings(), &settings);
    }
    fn monitor_compact_readings(&self, ctx: &egui::Context) -> Vec<CompactReading> {
        if self.settings.monitor.capsule_items().next().is_none() {
            return Vec::new();
        }
        // 字形图集变化意味着字体或缩放已经实际生效，需要重新测量。
        let atlas = ctx.fonts(|fonts| fonts.texture_atlas());
        let key = egui::Id::new("monitor-compact-value-widths");
        let widths = ctx.data(|data| data.get_temp::<CompactWidthCache>(key))
            .filter(|cache| std::sync::Arc::ptr_eq(&cache.atlas, &atlas))
            .map(|cache| cache.widths);
        let (percent_width, rate_width) = widths.unwrap_or_else(|| {
            let widths = ctx.fonts(|fonts| {
                let measure = |text: String| {
                    fonts
                        .layout_no_wrap(text, theme::font(9., W::Semibold), theme::WHITE)
                        .size()
                        .x
                };
                let digit = ('0'..='9')
                    .map(|digit| (digit, measure(format!("{digit}{digit}{digit}"))))
                    .max_by(|a, b| a.1.total_cmp(&b.1))
                    .map(|(digit, _)| digit)
                    .unwrap_or('8');
                let unknown_width = measure("—".into());
                let percent_width = measure(format!("{digit}{digit}{digit}%")).max(unknown_width);
                let rate_width = monitor::RATE_UNITS
                    .iter()
                    .flat_map(|unit| [
                        format!("{digit}{digit}{digit} {unit}"),
                        format!("{digit}{digit}.{digit} {unit}"),
                    ])
                    .map(measure)
                    .fold(unknown_width, f32::max);
                (percent_width, rate_width)
            });
            ctx.data_mut(|data| data.insert_temp(key, CompactWidthCache { atlas, widths }));
            widths
        });
        self.settings
            .monitor
            .capsule_items()
            .map(|reading| {
                let value = reading.value(&self.monitor.sample);
                let text_width = if matches!(reading, Reading::Download | Reading::Upload) {
                    rate_width
                } else {
                    percent_width
                };
                let padding = COMPACT_ICON_WIDTH + COMPACT_TEXT_GAP;
                // 为不同缩放下的字形取整留出余量，不随采样数值改变占位。
                let width = padding + text_width.ceil() + 3.;
                CompactReading { reading, value, width }
            })
            .collect()
    }
    pub(crate) fn monitor_capsule_width(&self, ctx: &egui::Context) -> f32 {
        compact_width(&self.monitor_compact_readings(ctx))
    }
    pub(crate) fn draw_monitor_compact(&self, c: &Card, x: f32, y: f32) {
        let colors = self.settings.monitor.colors(&self.settings.appearance);
        let items = self.monitor_compact_readings(c.ui.ctx());
        let mut left = x - compact_width(&items) / 2.;
        for item in items {
            reading_icon(
                c,
                item.reading,
                left + COMPACT_ICON_WIDTH / 2.,
                y,
                COMPACT_ICON_WIDTH,
                colors[item.reading as usize],
            );
            c.centered_text(
                left + COMPACT_ICON_WIDTH + COMPACT_TEXT_GAP,
                y,
                &c.fit(
                    &item.value,
                    9.,
                    W::Semibold,
                    item.width - COMPACT_ICON_WIDTH - COMPACT_TEXT_GAP,
                ),
                9.,
                W::Semibold,
                theme::WHITE,
                H::Left,
            );
            left += item.width + COMPACT_ITEM_GAP;
        }
    }
    pub(crate) fn page_monitor(&mut self, c: &Card) {
        let colors = self.settings.monitor.colors(&self.settings.appearance);
        let sample = &self.monitor.sample;
        c.text(14., 25., "系统监控", 13., W::Bold, theme::WHITE, H::Left);
        let stale = self.monitor.received.is_some_and(|at| {
            at.elapsed().as_secs() > self.settings.monitor.interval as u64 * 3 + 2
        });
        c.text(
            crate::app::CONTENT_W - 46.,
            24.,
            if stale {
                "读数已暂停"
            } else if self.monitor.received.is_some() {
                "实时 · Windows"
            } else {
                "等待采样"
            },
            9.,
            W::Regular,
            theme::GRAY,
            H::Right,
        );
        if !self.settings.monitor.has_readings() {
            c.text(
                14.,
                65.,
                "未选择展开读数",
                12.,
                W::Semibold,
                theme::GRAY,
                H::Left,
            );
            c.text(
                14.,
                86.,
                "在系统监控设置中开启需要的项目",
                10.,
                W::Regular,
                theme::DIM,
                H::Left,
            );
            return;
        }
        let readings: Vec<_> = [Reading::Cpu, Reading::Memory, Reading::Gpu]
            .into_iter()
            .filter(|reading| self.settings.monitor.expanded_reading(*reading))
            .collect();
        let count = readings.len().max(1);
        let width = (crate::app::CONTENT_W - 28. - (count - 1) as f32 * 8.) / count as f32;
        for (i, reading) in readings.iter().copied().enumerate() {
            let (value, series) = match reading {
                Reading::Cpu => (sample.cpu, &self.monitor.history.cpu),
                Reading::Memory => (sample.memory_percent(), &self.monitor.history.memory),
                _ => (sample.gpu, &self.monitor.history.gpu),
            };
            let x = 14. + i as f32 * (width + 8.);
            let color = colors[reading as usize];
            c.fill(x, 38., width, 78., 9., theme::CARD);
            reading_icon(c, reading, x + 12., 50., 12., color);
            c.text(
                x + 23.,
                53.,
                reading.label(),
                9.5,
                W::Semibold,
                theme::GRAY,
                H::Left,
            );
            c.text(
                x + 10.,
                78.,
                &monitor::percent(value),
                22.,
                W::Bold,
                theme::WHITE,
                H::Left,
            );
            let mut previous = None;
            let count = series.len().max(2);
            for (n, value) in series.iter().enumerate() {
                let point = value.map(|v| {
                    (
                        x + 9. + n as f32 / (count - 1) as f32 * (width - 18.),
                        108. - v.clamp(0., 100.) as f32 / 100. * 21.,
                    )
                });
                if let (Some(a), Some(b)) = (previous, point) {
                    c.seg(a, b, 1.4, color);
                }
                previous = point;
            }
            if value.is_none() {
                c.text(
                    x + width - 9.,
                    78.,
                    if reading == Reading::Gpu && self.monitor.received.is_some() {
                        "不可用"
                    } else {
                        "采样中"
                    },
                    8.5,
                    W::Regular,
                    theme::DIM,
                    H::Right,
                );
            }
        }
        let mut y = if readings.is_empty() { 38. } else { 122. };
        if self.settings.monitor.show_memory {
            let memory = match (sample.memory_used, sample.memory_total) {
                (Some(used), Some(total)) => {
                    format!("{} / {}", monitor::bytes(used), monitor::bytes(total))
                }
                _ => "内存读数不可用".into(),
            };
            c.text(14., y + 10., &memory, 9.5, W::Regular, colors[1], H::Left);
        }
        let mut right = crate::app::CONTENT_W - 14.;
        for reading in [Reading::Upload, Reading::Download] {
            if !self.settings.monitor.expanded_reading(reading) {
                continue;
            }
            let color = colors[reading as usize];
            let value = reading.value(sample);
            let width = c.text_w(&value, 9.5, W::Semibold);
            c.text(right, y + 10., &value, 9.5, W::Semibold, color, H::Right);
            reading_icon(c, reading, right - width - 9., y + 6., 11., color);
            right -= width + 30.;
        }
        if self.settings.monitor.show_memory
            || self.settings.monitor.show_download
            || self.settings.monitor.show_upload
        {
            y += 22.;
        }
        if self.settings.monitor.show_storage {
            let count = sample.drives.len().clamp(1, 3);
            let cell = (crate::app::CONTENT_W - 28.) / count as f32;
            if sample.drives.is_empty() {
                reading_icon(c, Reading::Storage, 20., y + 8., 11., colors[5]);
                c.text(
                    30.,
                    y + 12.,
                    "磁盘 · 暂无可用读数",
                    9.,
                    W::Regular,
                    theme::DIM,
                    H::Left,
                );
            }
            for (i, drive) in sample.drives.iter().take(3).enumerate() {
                let x = 14. + i as f32 * cell;
                let used = drive.used as f64 / drive.total.max(1) as f64;
                reading_icon(c, Reading::Storage, x + 6., y + 6., 10., colors[5]);
                c.text(
                    x + 16.,
                    y + 10.,
                    &format!("{}  {:.0}%", drive.name, used * 100.),
                    9.,
                    W::Semibold,
                    colors[5],
                    H::Left,
                );
                c.fill(x, y + 16., cell - 12., 3., 1.5, theme::TRACK);
                c.fill(
                    x,
                    y + 16.,
                    ((cell - 12.) * used as f32).max(1.),
                    3.,
                    1.5,
                    colors[5],
                );
                c.hit(x, y, cell - 8., 23., ("monitor-drive", i))
                    .on_hover_text(format!(
                        "已用 {} / 总计 {}",
                        monitor::bytes(drive.used),
                        monitor::bytes(drive.total)
                    ));
            }
            if sample.drives.len() > 3 {
                c.hit(
                    14.,
                    y,
                    crate::app::CONTENT_W - 28.,
                    23.,
                    "monitor-drives-all",
                )
                .on_hover_text(
                    sample
                        .drives
                        .iter()
                        .map(|d| {
                            format!(
                                "{} 已用 {} / {}",
                                d.name,
                                monitor::bytes(d.used),
                                monitor::bytes(d.total)
                            )
                        })
                        .collect::<Vec<_>>()
                        .join("\n"),
                );
            }
            y += 30.;
        }
        if self.settings.monitor.show_processes {
            process_icon(c, 21., y + 6., theme::GRAY);
            c.text(
                34.,
                y + 10.,
                "高占用应用",
                9.,
                W::Semibold,
                theme::GRAY,
                H::Left,
            );
            for (i, label) in ["CPU", "内存"].into_iter().enumerate() {
                let x = crate::app::CONTENT_W - 93. + i as f32 * 43.;
                let selected = self.settings.monitor.sort_memory == (i == 1);
                c.text(
                    x + 18.,
                    y + 10.,
                    label,
                    9.,
                    W::Semibold,
                    if selected { colors[0] } else { theme::DIM },
                    H::Center,
                );
                if c.hit(x, y - 2., 38., 16., ("monitor-sort", i)).clicked() {
                    self.settings.monitor.sort_memory = i == 1;
                    self.monitor.sample.sort_processes(i == 1);
                    self.settings.save();
                }
            }
            y += 14.;
            let rows = ((222. - y) / 17.).floor().max(1.) as usize;
            for (i, process) in self.monitor.sample.processes.iter().take(rows).enumerate() {
                let y = y + i as f32 * 17.;
                c.text(
                    14.,
                    y + 11.,
                    &c.fit(&process.name, 9., W::Regular, 215.),
                    9.,
                    W::Regular,
                    theme::WHITE,
                    H::Left,
                );
                c.text(
                    crate::app::CONTENT_W - 100.,
                    y + 11.,
                    &monitor::percent(process.cpu),
                    9.,
                    W::Semibold,
                    colors[0],
                    H::Right,
                );
                c.text(
                    crate::app::CONTENT_W - 14.,
                    y + 11.,
                    &monitor::bytes(process.memory),
                    9.,
                    W::Regular,
                    theme::GRAY,
                    H::Right,
                );
            }
            if self.monitor.sample.processes.is_empty() {
                c.text(
                    14.,
                    y + 12.,
                    "正在等待可读取的应用…",
                    9.,
                    W::Regular,
                    theme::DIM,
                    H::Left,
                );
            }
        } else {
            c.text(
                14.,
                y + 14.,
                &format!(
                    "最近 {} 个样本 · 每 {} 秒更新",
                    self.monitor.history.cpu.len(),
                    self.settings.monitor.interval
                ),
                9.,
                W::Regular,
                theme::DIM,
                H::Left,
            );
        }
    }
    pub(crate) fn monitor_settings(&mut self, ui: &mut egui::Ui) {
        use crate::extras::*;
        self.monitor_capsule_preview(ui);
        settings_label(ui, "监控项目");
        settings_card(ui, |ui| {
            let width = ui.available_width();
            let (header, _) = ui.allocate_exact_size(egui::vec2(width, 19.), egui::Sense::hover());
            for (offset, label) in [(94., "胶囊态"), (17., "展开态")] {
                ui.painter().text(
                    egui::pos2(header.right() - offset, header.center().y),
                    egui::Align2::CENTER_CENTER,
                    label,
                    egui::FontId::proportional(11.5),
                    ui.visuals().text_color(),
                );
            }
            let colors = self.settings.monitor.colors(&self.settings.appearance);
            for index in 0..7 {
                if index > 0 {
                    ui.separator();
                }
                let reading = Reading::ALL.get(index).copied();
                let (label, description, color) = reading.map_or(
                    (
                        "进程列表",
                        "高占用应用，可按 CPU 或内存排序",
                        ui.visuals().weak_text_color(),
                    ),
                    |r| (r.label(), r.description(), colors[index]),
                );
                let description = ui.painter().layout(
                    description.to_owned(),
                    egui::FontId::proportional(11.),
                    ui.visuals().weak_text_color(),
                    (width - 196.).max(70.),
                );
                let (row, _) = ui.allocate_exact_size(
                    egui::vec2(width, (description.size().y + 25.).max(43.)),
                    egui::Sense::hover(),
                );
                let icon = Card::new(ui, row.min, ui.clip_rect()).ghost();
                // 设置中的颜色加深，以适应浅色卡片背景。
                let icon_color = if ui.visuals().dark_mode {
                    color
                } else {
                    egui::Color32::from_rgb(
                        (color.r() as f32 * 0.65) as u8,
                        (color.g() as f32 * 0.65) as u8,
                        (color.b() as f32 * 0.65) as u8,
                    )
                };
                if let Some(reading) = reading {
                    reading_icon(&icon, reading, 12., row.height() / 2., 19., icon_color);
                } else {
                    process_icon(&icon, 12., row.height() / 2., icon_color);
                }
                ui.painter().text(
                    row.min + egui::vec2(34., 3.),
                    egui::Align2::LEFT_TOP,
                    label,
                    egui::FontId::proportional(13.),
                    ui.visuals().text_color(),
                );
                ui.painter().galley(
                    row.min + egui::vec2(34., 22.),
                    description,
                    ui.visuals().weak_text_color(),
                );
                let mut expanded = reading.map_or(self.settings.monitor.show_processes, |r| {
                    self.settings.monitor.expanded_reading(r)
                });
                let mut capsule =
                    reading.is_some_and(|r| self.settings.monitor.capsule_readings[r as usize]);
                for (offset, mode, value) in [
                    (94., "capsule", &mut capsule),
                    (17., "expanded", &mut expanded),
                ] {
                    let center = egui::pos2(row.right() - offset, row.center().y);
                    if reading.is_none() && mode == "capsule" {
                        ui.painter().text(
                            center,
                            egui::Align2::CENTER_CENTER,
                            "—",
                            egui::FontId::proportional(11.),
                            ui.visuals().weak_text_color(),
                        );
                    } else {
                        let rect = egui::Rect::from_center_size(center, egui::vec2(34., 20.));
                        let mut control = ui.new_child(egui::UiBuilder::new().max_rect(rect));
                        monitor_switch(&mut control, index, mode, value);
                    }
                }
                if let Some(reading) = reading {
                    self.settings
                        .monitor
                        .set_expanded_reading(reading, expanded);
                    self.settings.monitor.capsule_readings[index] = capsule;
                } else {
                    self.settings.monitor.show_processes = expanded;
                }
            }
            settings_hint(ui, "两列独立控制；页面和胶囊的总开关位于“页面管理”。磁盘胶囊显示所有固定磁盘的合计占用率。");
        });
        settings_label(ui, "读数配色");
        settings_card(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                for palette in Palette::ALL {
                    let settings = monitor::Settings {
                        palette,
                        ..self.settings.monitor.clone()
                    };
                    let colors = settings.colors(&self.settings.appearance);
                    let (rect, response) =
                        ui.allocate_exact_size(egui::vec2(100., 54.), egui::Sense::click());
                    let selected = self.settings.monitor.palette == palette;
                    ui.painter().rect_filled(
                        rect,
                        7.,
                        if selected {
                            ui.visuals().selection.bg_fill
                        } else {
                            ui.visuals().extreme_bg_color
                        },
                    );
                    for (i, color) in colors.iter().take(3).enumerate() {
                        ui.painter().circle_filled(
                            rect.left_top() + egui::vec2(31. + i as f32 * 16., 15.),
                            4.,
                            *color,
                        );
                    }
                    ui.painter().text(
                        rect.left_bottom() + egui::vec2(50., -14.),
                        egui::Align2::CENTER_CENTER,
                        palette.label(),
                        egui::FontId::proportional(11.),
                        ui.visuals().text_color(),
                    );
                    if response
                        .on_hover_text(format!("将各项读数设为{}配色", palette.label()))
                        .clicked()
                    {
                        self.settings.monitor.palette = palette;
                    }
                }
            });
            ui.separator();
            let colors = self.settings.monitor.colors(&self.settings.appearance);
            let columns = if ui.available_width() < 500. { 3 } else { 6 };
            egui::Grid::new("monitor-reading-colors")
                .num_columns(columns)
                .spacing(egui::vec2(10., 8.))
                .show(ui, |ui| {
                    for (i, label) in ["CPU", "内存", "GPU", "下载", "上传", "磁盘"]
                        .iter()
                        .enumerate()
                    {
                        ui.vertical(|ui| {
                            let mut rgb = [colors[i].r(), colors[i].g(), colors[i].b()];
                            ui.horizontal(|ui| {
                                if ui.color_edit_button_srgb(&mut rgb).changed() {
                                    if self.settings.monitor.palette != Palette::Custom {
                                        self.settings.monitor.custom_colors = colors.map(|c| {
                                            format!("#{:02X}{:02X}{:02X}", c.r(), c.g(), c.b())
                                        });
                                    }
                                    self.settings.monitor.palette = Palette::Custom;
                                    self.settings.monitor.custom_colors[i] =
                                        format!("#{:02X}{:02X}{:02X}", rgb[0], rgb[1], rgb[2]);
                                }
                                ui.label(*label);
                            });
                            ui.label(
                                egui::RichText::new(format!(
                                    "#{:02X}{:02X}{:02X}",
                                    rgb[0], rgb[1], rgb[2]
                                ))
                                .size(10.)
                                .color(ui.visuals().weak_text_color()),
                            );
                        });
                        if (i + 1) % columns == 0 {
                            ui.end_row();
                        }
                    }
                });
        });
        settings_label(ui, "采样与显示");
        settings_card(ui, |ui| {
            settings_control_row_right(ui, "刷新间隔", |ui| {
                settings_segmented(
                    ui,
                    &mut self.settings.monitor.interval,
                    &[(1, "1 秒"), (2, "2 秒"), (5, "5 秒")],
                    60.,
                );
            });
            settings_hint(ui,"CPU 与网速需要两次采样。GPU 通过 Windows 性能计数器读取；不可用时显示“—”。曲线保留最近 60 个样本。");
            settings_hint(
                ui,
                "仅在监控页面、设置预览或监控胶囊可见时采样；关闭这些视图后停止请求新读数。",
            );
        });
    }

    pub(crate) fn monitor_capsule_tooltip(&self) -> String {
        self.settings
            .monitor
            .capsule_items()
            .map(|reading| {
                format!(
                    "{} {}",
                    reading.label(),
                    reading.value(&self.monitor.sample)
                )
            })
            .collect::<Vec<_>>()
            .join(" · ")
    }

    pub(crate) fn draw_monitor_side(&self, c: &Card, x: f32, y: f32) {
        let colors = self.settings.monitor.colors(&self.settings.appearance);
        let count = self.settings.monitor.capsule_items().count();
        let top = y - count as f32 * 30. / 2.;
        for (index, reading) in self.settings.monitor.capsule_items().enumerate() {
            let mid = top + index as f32 * 30. + 15.;
            reading_icon(c, reading, x, mid - 6., 11., colors[reading as usize]);
            let value = reading.value(&self.monitor.sample);
            c.centered_text(
                x,
                mid + 7.,
                &c.fit(&value, 7., W::Semibold, 30.),
                7.,
                W::Semibold,
                theme::WHITE,
                H::Center,
            );
        }
    }

    pub(crate) fn monitor_capsule_preview(&self, ui: &mut egui::Ui) {
        use crate::extras::{settings_hint, settings_label};
        settings_label(ui, "胶囊预览");
        let width = self.monitor_capsule_width(ui.ctx());
        let side = self.side();
        let notch = self.settings.island_shape != crate::model::IslandShape::Pill;
        let count = self.settings.monitor.capsule_items().count();
        let size = if side {
            egui::vec2(36., count as f32 * 30. + 14.)
        } else {
            egui::vec2(width + 32., 30.)
        };
        let (stage, _) = ui.allocate_exact_size(
            egui::vec2(
                ui.available_width(),
                if side {
                    size.y.max(46.) + 20. + if notch { crate::app::FLARE * 2. } else { 0. }
                } else {
                    66.
                },
            ),
            egui::Sense::hover(),
        );
        ui.painter()
            .rect_filled(stage, 9., egui::Color32::from_rgb(19, 25, 36));
        if count == 0 {
            ui.painter().text(
                stage.center(),
                egui::Align2::CENTER_CENTER,
                "未选择胶囊读数",
                egui::FontId::proportional(12.),
                theme::GRAY,
            );
        } else {
            let outer_width = size.x
                + if notch && !side {
                    crate::app::FLARE * 2.
                } else {
                    0.
                };
            let scale = ((stage.width() - 24.) / outer_width).min(1.);
            let body = egui::Rect::from_center_size(stage.center(), size * scale);
            self.paint_feature_preview_shell(ui.painter(), body, scale);
            let from = crate::skin_paint::shape_count(ui.ctx(), ui.layer_id());
            let mut card = Card::new(ui, body.min, stage.intersect(ui.clip_rect())).ghost();
            card.s = scale;
            if side {
                self.draw_monitor_side(&card, size.x / 2., size.y / 2.);
            } else {
                self.draw_monitor_compact(&card, size.x / 2., size.y / 2.);
            }
            if let Some(palette) = self.skin_palette() {
                let end = crate::skin_paint::shape_count(ui.ctx(), ui.layer_id());
                crate::skin_paint::recolor_layer(ui.ctx(), ui.layer_id(), &[(from, end)], &palette);
            }
        }
        settings_hint(
            ui,
            if self.capsule_enabled(9) {
                "预览使用当前读数，随图标、开关和配色更新。"
            } else {
                "监控胶囊总开关已关闭，可在“页面管理”中开启；这里仍可预览所选读数。"
            },
        );
    }
}
