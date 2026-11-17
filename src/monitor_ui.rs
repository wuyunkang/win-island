//! 性能页和设置共享相同读数、配色与曲线。
use crate::{
    app::App,
    paint::{Card, H},
    system_monitor::{self as monitor, Palette},
    theme::{self, W},
};
use eframe::egui;

impl App {
    pub(crate) fn monitor_polling_needed(&self) -> bool {
        self.capsule_enabled(9) || self.feature_view_needed(9)
    }
    pub(crate) fn monitor_tick(&mut self, ctx: &egui::Context) {
        let needed = self.monitor_polling_needed();
        let mut settings = self.settings.monitor.clone();
        if !self.feature_view_needed(9) {
            settings.show_processes = false;
            settings.show_storage = false;
            settings.compact = true;
        }
        self.monitor.update(ctx, needed, &settings);
    }
    pub(crate) fn draw_monitor_compact(&self, c: &Card, x: f32, y: f32) {
        let colors = self.settings.monitor.colors(&self.settings.appearance);
        for (offset, label, value, color) in [
            (-18., "C", self.monitor.sample.cpu, colors[0]),
            (18., "M", self.monitor.sample.memory_percent(), colors[1]),
        ] {
            c.text(
                x + offset - 14.,
                y + 3.5,
                label,
                8.,
                W::Semibold,
                color,
                H::Left,
            );
            c.text(
                x + offset + 14.,
                y + 3.5,
                &monitor::percent(value),
                9.,
                W::Semibold,
                theme::WHITE,
                H::Right,
            );
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
        let width = (crate::app::CONTENT_W - 44.) / 3.;
        for (i, (label, value, series)) in [
            ("CPU", sample.cpu, &self.monitor.history.cpu),
            (
                "内存",
                sample.memory_percent(),
                &self.monitor.history.memory,
            ),
            ("GPU", sample.gpu, &self.monitor.history.gpu),
        ]
        .into_iter()
        .enumerate()
        {
            let x = 14. + i as f32 * (width + 8.);
            let color = colors[i];
            c.fill(x, 38., width, 78., 9., theme::CARD);
            c.circle(x + 11., 50., 2.5, color);
            c.text(x + 19., 53., label, 9.5, W::Semibold, theme::GRAY, H::Left);
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
                    if i == 2 && self.monitor.received.is_some() {
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
        let memory = match (sample.memory_used, sample.memory_total) {
            (Some(used), Some(total)) => {
                format!("{} / {}", monitor::bytes(used), monitor::bytes(total))
            }
            _ => "内存读数不可用".into(),
        };
        c.text(14., 132., &memory, 9.5, W::Regular, colors[1], H::Left);
        let upload = format!("↑ {}", monitor::rate(sample.upload));
        let upload_width = c.text_w(&upload, 9.5, W::Semibold);
        c.text(
            crate::app::CONTENT_W - 14.,
            132.,
            &upload,
            9.5,
            W::Semibold,
            colors[4],
            H::Right,
        );
        c.text(
            crate::app::CONTENT_W - 26. - upload_width,
            132.,
            &format!("↓ {}", monitor::rate(sample.download)),
            9.5,
            W::Semibold,
            colors[3],
            H::Right,
        );
        let mut y = 142.;
        if self.settings.monitor.show_storage {
            let count = sample.drives.len().clamp(1, 3);
            let cell = (crate::app::CONTENT_W - 28.) / count as f32;
            if sample.drives.is_empty() {
                c.text(
                    14.,
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
                c.text(
                    x,
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
            c.text(
                14.,
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
            ui.separator();
            setting_row(
                ui,
                "显示进程列表",
                "按 CPU 或内存列出当前可读取的应用",
                &mut self.settings.monitor.show_processes,
            );
            ui.separator();
            setting_row(
                ui,
                "显示磁盘",
                "显示本机固定磁盘的空间占用",
                &mut self.settings.monitor.show_storage,
            );
            settings_hint(ui,"CPU 与网速需要两次采样。GPU 通过 Windows 性能计数器读取；不可用时显示“—”。曲线保留最近 60 个样本。");
            settings_hint(
                ui,
                "仅在监控页面、设置预览或监控胶囊可见时采样；关闭这些视图后停止请求新读数。",
            );
        });
    }
}
