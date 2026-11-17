//! 单列剪贴板、文件历史与分类配置。
use crate::{
    app::{App, Cmd},
    clipboard::{Clip, Picture},
    paint::Card,
    transfer::{self, Category, Groups, FAVORITES},
};
use eframe::egui::{self, Color32, RichText, Sense, Vec2};
use std::collections::HashMap;
#[derive(Default)]
pub struct TransferUi {
    pub clip_group: Option<u64>,
    pub editing: bool,
    pub detail: Option<Clip>,
    pub edit: Option<(u64, String)>,
    pub category: Option<(bool, Category)>,
    pub new_category: String,
    pub error: String,
    pub textures: HashMap<u64, egui::TextureHandle>,
    file_types: crate::clipboard_icon::DirectoryCache,
}
impl TransferUi {
    pub(crate) fn release_resources(&mut self){self.textures=HashMap::new();self.file_types.clear();}
}
fn photo(
    ui: &mut egui::Ui,
    textures: &mut HashMap<u64, egui::TextureHandle>,
    id: u64,
    p: &Picture,
    size: Vec2,
) {
    if let std::collections::hash_map::Entry::Vacant(e) = textures.entry(id) {
        if ui.painter().opacity() >= 0.99 {
            if let Ok(image) = image::load_from_memory(p.png.as_slice()) {
                let rgba = image.thumbnail(640, 400).to_rgba8();
                e.insert(ui.ctx().load_texture(
                    format!("clip-image-{id}"),
                    egui::ColorImage::from_rgba_unmultiplied(
                        [rgba.width() as usize, rgba.height() as usize],
                        rgba.as_raw(),
                    ),
                    egui::TextureOptions::LINEAR,
                ));
            }
        }
    }
    if let Some(texture) = textures.get(&id) {
        ui.add(
            egui::Image::new(texture)
                .fit_to_exact_size(size)
                .maintain_aspect_ratio(true),
        );
    }
}
fn line(ui: &mut egui::Ui, text: RichText) {
    ui.add(egui::Label::new(text).truncate());
}
fn tabs(ui: &mut egui::Ui, groups: &Groups, current: &mut Option<u64>) {
    let label = current
        .and_then(|id| groups.entries.iter().find(|c| c.id == id))
        .map_or("全部", |c| c.name.as_str());
    egui::ComboBox::from_id_salt(ui.id().with("groups"))
        .selected_text(label)
        .width(48.)
        .show_ui(ui, |ui| {
            ui.selectable_value(current, None, "全部");
            for c in &groups.entries {
                ui.selectable_value(current, Some(c.id), &c.name);
            }
        });
}
impl App {
    pub(crate) fn transfer_page(&mut self, c: &Card, cmds: &mut Vec<Cmd>) {
        let width = crate::app::CONTENT_W;
        let height = self.expanded_height();
        let rect = c.rect(12., 33., width - 24., height - 48.);
        let mut ui = egui::Ui::new(
            c.ui.ctx().clone(),
            c.ui.id().with("transfer-station"),
            egui::UiBuilder::new()
                .layer_id(c.painter.layer_id())
                .max_rect(rect)
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        ui.set_clip_rect(rect.intersect(c.painter.clip_rect()));
        ui.set_enabled(c.live);
        ui.set_opacity(c.painter.opacity());
        ui.style_mut().visuals = egui::Visuals::dark();
        ui.spacing_mut().item_spacing = Vec2::new(4., 3.);
        self.transfer_ui.editing = false;

        if let Some((id, mut text)) = self.transfer_ui.edit.clone() {
            ui.horizontal(|ui| {
                if ui.button("返回").clicked() {
                    self.transfer_ui.edit = None;
                }
                ui.strong(if id == 0 {
                    "新建常用语"
                } else {
                    "编辑文字"
                });
            });
            let response = ui.add_sized(
                [ui.available_width(), ui.available_height() - 35.],
                egui::TextEdit::multiline(&mut text).hint_text("输入内容"),
            );
            self.transfer_ui.editing = response.has_focus();
            if response.clicked() {
                cmds.push(Cmd::Focus);
            }
            if self.transfer_ui.edit.is_none() {
                return;
            }
            self.transfer_ui.edit = Some((id, text.clone()));
            if ui
                .add_enabled(!text.trim().is_empty(), egui::Button::new("保存"))
                .clicked()
            {
                if id == 0 {
                    self.clipboard.lock().add_phrase(&text);
                } else {
                    let mut h = self.clipboard.lock();
                    if let Some(clip) = h.items.iter_mut().find(|c| c.id == id) {
                        clip.text = text;
                        clip.kind = transfer::classify(&clip.text);
                        clip.html.clear();
                        clip.rtf.clear();
                    }
                    h.changed();
                }
                self.transfer_ui.edit = None;
            }
            return;
        }
        if let Some(clip) = self.transfer_ui.detail.clone() {
            self.transfer_ui.textures.retain(|id, _| *id == clip.id);
            let is_dir = if clip.files.len() == 1 {
                self.transfer_ui.file_types.get(ui.ctx(), &clip.files[0])
            } else { None };
            let icon = crate::clipboard_icon::icon_for(clip.kind, &clip.files, is_dir);
            ui.horizontal(|ui| {
                if ui.button("返回").clicked() {
                    self.transfer_ui.detail = None;
                }
                crate::clipboard_icon::show(ui, icon);
                ui.strong(icon.label());
                if ui.button("粘贴").clicked() {
                    cmds.push(Cmd::PasteClip(clip.clone(), false));
                }
                if ui.button("只复制").clicked() {
                    cmds.push(Cmd::CopyClip(clip.clone(), false));
                }
                if clip.picture.is_none() && clip.files.is_empty() && ui.button("编辑").clicked() {
                    self.transfer_ui.detail = None;
                    self.transfer_ui.edit = Some((clip.id, clip.text.clone()));
                }
            });
            ui.label(format!(
                "来源：{} · {} · {} 字符",
                if clip.source.is_empty() {
                    "手动添加"
                } else {
                    &clip.source
                },
                clip.at,
                clip.text.chars().count()
            ));
            egui::ScrollArea::vertical()
                .id_salt("clip-detail")
                .show(&mut ui, |ui| {
                    if !clip.files.is_empty() {
                        ui.label(format!(
                            "文件记录 · {} 项，仅保存位置，不复制文件内容",
                            clip.files.len()
                        ));
                        for path in &clip.files {
                            let is_dir = self.transfer_ui.file_types.get(ui.ctx(), path);
                            let icon = crate::clipboard_icon::icon_for(transfer::Kind::Files, std::slice::from_ref(path), is_dir);
                            ui.horizontal(|ui| {
                                crate::clipboard_icon::show(ui, icon);
                                ui.add(
                                    egui::Label::new(path.to_string_lossy())
                                        .selectable(true)
                                        .wrap(),
                                );
                                if ui.small_button("定位").clicked() {
                                    cmds.push(Cmd::Reveal(path.clone()));
                                }
                            });
                        }
                    } else if let Some(p) = &clip.picture {
                        photo(
                            ui,
                            &mut self.transfer_ui.textures,
                            clip.id,
                            p,
                            Vec2::new(ui.available_width(), height - 125.),
                        );
                        ui.label(format!("{} × {}", p.width, p.height));
                    } else {
                        ui.add(egui::Label::new(&clip.text).selectable(true).wrap());
                        if clip.kind == transfer::Kind::Link {
                            ui.hyperlink_to("打开链接", &clip.text);
                        }
                    }
                });
            return;
        }
        ui.spacing_mut().interact_size.y = 16.;
        ui.spacing_mut().item_spacing.y = 2.;
        ui.horizontal(|ui| {
            let groups = self.clipboard.lock().groups.clone();

            tabs(ui, &groups, &mut self.transfer_ui.clip_group);

            ui.menu_button("操作", |ui| {
                if !self.settings.clipboard_history && ui.button("开启记录").clicked() {
                    self.settings.clipboard_history = true;
                    self.settings.save();
                    ui.close_menu();
                }
                if ui.button("新建常用语").clicked() {
                    self.transfer_ui.edit = Some((0, String::new()));
                    ui.close_menu();
                }
                if ui.button("清空临时").clicked() {
                    self.clipboard.lock().clear_recent();
                    ui.close_menu();
                }

                self.shelf_menu_rect = Some(ui.min_rect());
            });
        });

        self.transfer_clips(&mut ui, cmds);

        if let Some(error) = &self.clipboard.lock().error {
            ui.colored_label(Color32::LIGHT_RED, error);
        }
    }

    fn transfer_clips(&mut self, ui: &mut egui::Ui, cmds: &mut Vec<Cmd>) {
        let left = ui;
        let groups = self.clipboard.lock().groups.clone(); // 只构建轻量预览，图片字节通过 Arc 共享。
        let entries: Vec<_> = {
            let h = self.clipboard.lock();
            h.items
                .iter()
                .enumerate()
                .filter(|(_, clip)| {
                    self.transfer_ui.clip_group.is_none_or(|id| {
                        if id == FAVORITES {
                            clip.pinned || clip.manual || clip.categories.contains(&id)
                        } else {
                            clip.categories.contains(&id)
                        }
                    })
                })
                .map(|(i, c)| {
                    (
                        i,
                        c.id,
                        c.kind,
                        c.preview(),
                        c.pinned,
                        if c.files.is_empty() {
                            c.source.clone()
                        } else {
                            format!("{}\n{}", c.source, c.text)
                        },
                        c.at.clone(),
                        c.picture.clone(),
                        c.files.iter().take(2).cloned().collect::<Vec<_>>(),
                    )
                })
                .collect()
        };
        let mut visible = std::collections::HashSet::new();
        egui::ScrollArea::vertical()
            .id_salt("transfer-clips")
            .max_height(left.available_height())
            .show_rows(left, 18., entries.len(), |ui, range| {
                for row in range {
                    let (_index, id, kind, preview, pinned, source, at, picture, files) = &entries[row];
                    visible.insert(*id);
                    let is_dir = if files.len() == 1 {
                        self.transfer_ui.file_types.get(ui.ctx(), &files[0])
                    } else { None };
                    let icon = crate::clipboard_icon::icon_for(*kind, files, is_dir);
                    let r = egui::Frame::new()
                        .fill(Color32::from_gray(25))
                        .corner_radius(7.)
                        .inner_margin(egui::Margin::symmetric(4, 1))
                        .show(ui, |ui| {
                            ui.set_min_size(Vec2::new(ui.available_width(), 16.));
                            ui.horizontal(|ui| {
                                if let Some(p) = picture {
                                    photo(
                                        ui,
                                        &mut self.transfer_ui.textures,
                                        *id,
                                        p,
                                        Vec2::splat(14.),
                                    );
                                } else if *kind == transfer::Kind::Color {
                                    if let Some(rgb) = transfer::color(preview) {
                                        let (rect, _) = ui
                                            .allocate_exact_size(Vec2::splat(12.), Sense::hover());
                                        ui.painter().rect_filled(
                                            rect,
                                            4.,
                                            Color32::from_rgb(rgb[0], rgb[1], rgb[2]),
                                        );
                                    }
                                } else {
                                    crate::clipboard_icon::show(ui, icon);
                                }
                                if *pinned {
                                    ui.label(RichText::new("★").color(Color32::from_rgb(255, 205, 107)).size(9.));
                                }
                                let text = RichText::new(preview).size(11.5);
                                ui.add(
                                    egui::Label::new(if *kind == transfer::Kind::Code {
                                        text.monospace()
                                    } else {
                                        text
                                    })
                                    .truncate(),
                                );
                            });
                        })
                        .response;
                    let hit = ui.interact(
                        r.rect,
                        ui.id().with(("clip-entry", id)),
                        Sense::click_and_drag(),
                    );
                    let hit = hit.on_hover_text(format!("{} · {preview}\n{source} · {at}",icon.label()));
                    if hit.clicked() {
                        if let Some(clip) = self.clipboard.lock().get_id(*id).cloned() {
                            cmds.push(Cmd::PasteClip(clip, false));
                        }
                    }
                    if hit.drag_started() {
                        if let Some(clip) = self.clipboard.lock().get_id(*id).cloned() {
                            cmds.push(Cmd::DragClip(clip));
                        }
                    }
                    hit.context_menu(|ui| {
                        let clip = self.clipboard.lock().get_id(*id).cloned();
                        if let Some(clip) = clip {
                            for (label, plain, copy) in [
                                ("粘贴", false, false),
                                ("粘贴为纯文本", true, false),
                                ("只复制", false, true),
                            ] {
                                if ui.button(label).clicked() {
                                    cmds.push(if copy {
                                        Cmd::CopyClip(clip.clone(), plain)
                                    } else {
                                        Cmd::PasteClip(clip.clone(), plain)
                                    });
                                    ui.close_menu();
                                }
                            }
                            if ui.button("查看详情").clicked() {
                                self.transfer_ui.detail = Some(clip.clone());
                                ui.close_menu();
                            }
                            if clip.picture.is_none()
                                && clip.files.is_empty()
                                && ui.button("编辑文字").clicked()
                            {
                                self.transfer_ui.edit = Some((clip.id, clip.text.clone()));
                                ui.close_menu();
                            }
                            if ui
                                .button(if clip.pinned {
                                    "移出常用"
                                } else {
                                    "加入常用"
                                })
                                .clicked()
                            {
                                self.clipboard.lock().pin_id(*id);
                                ui.close_menu();
                            }
                            ui.menu_button("加入分类", |ui| {
                                for category in groups.entries.iter().filter(|c| c.id != FAVORITES)
                                {
                                    let mut on = clip.categories.contains(&category.id);
                                    if ui.checkbox(&mut on, &category.name).changed() {
                                        let mut h = self.clipboard.lock();
                                        if let Some(c) =
                                            h.items.iter_mut().find(|c| c.id == clip.id)
                                        {
                                            if on {
                                                c.categories.push(category.id);
                                            } else {
                                                c.categories.retain(|id| *id != category.id);
                                            }
                                        }
                                        h.changed();
                                    }
                                }
                            });
                            if ui.button("删除记录").clicked() {
                                self.clipboard.lock().remove_id(*id);
                                ui.close_menu();
                            }
                        }
                        self.shelf_menu_rect = Some(ui.min_rect());
                    });
                }
            });
        self.transfer_ui
            .textures
            .retain(|id, _| visible.contains(id));
    }

    pub(crate) fn transfer_settings(&mut self, ui: &mut egui::Ui) {
        use crate::extras::{
            setting_number, setting_row, settings_card, settings_control_row_right as row,
            settings_hint, settings_label,
        };

        settings_label(ui, "记录内容");
        settings_card(ui, |ui| {
            setting_row(
                ui,
                "记录剪贴板",
                "记录复制的文本、链接、代码、颜色和文件",
                &mut self.settings.clipboard_history,
            );
            ui.separator();
            ui.add_enabled_ui(self.settings.clipboard_history, |ui| {
                setting_row(
                    ui,
                    "记录图片",
                    "保存图片预览与原始图像内容",
                    &mut self.settings.clipboard_images,
                );
                ui.separator();
                setting_number(
                    ui,
                    "最近记录上限",
                    &mut self.settings.clipboard_limit,
                    30,
                    500,
                    " 条",
                    10.,
                );
            });
            settings_hint(
                ui,
                "普通记录退出清除；常用和一直保留的分类持久保存，使用当前 Windows 用户加密。",
            );
        });
        settings_label(ui, "排除来源");
        settings_card(ui, |ui| {
            let mut excluded = self.settings.clipboard_excluded.join(", ");
            row(ui, "不记录的应用", |ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut excluded)
                        .desired_width(220.)
                        .hint_text("Private.exe, Other.exe"),
                )
            });
            self.settings.clipboard_excluded = excluded
                .split([',', '，', ';'])
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect();
            settings_hint(
                ui,
                "填写进程名，使用逗号分隔；密码管理器的禁止记录标记会自动识别。",
            );
        });
        settings_label(ui, "常用语");
        settings_card(ui, |ui| {
            if ui.button("新建常用语").clicked() {
                self.settings_open = false;
                self.show_page(3);
                self.transfer_ui.edit = Some((0, String::new()));
            }
            let phrases: Vec<_> = self
                .clipboard
                .lock()
                .items
                .iter()
                .filter(|c| c.manual)
                .map(|c| (c.id, c.preview()))
                .collect();
            if phrases.is_empty() {
                settings_hint(ui, "保存常用回复，在剪贴板中点击即可粘贴。");
            }
            for (id, text) in phrases {
                ui.horizontal(|ui| {
                    ui.add_sized(
                        [ui.available_width() - 60., 25.],
                        egui::Label::new(text).truncate(),
                    );
                    let r = ui.button("编辑");
                    if r.clicked() {
                        let clip = self.clipboard.lock().get_id(id).cloned();
                        if let Some(c) = clip {
                            self.settings_open = false;
                            self.show_page(3);
                            self.transfer_ui.edit = Some((c.id, c.text));
                        }
                    }
                });
            }
        });

        settings_label(ui, "分类与自动归类");
        settings_card(ui, |ui| {
            let mut groups = self.clipboard.lock().groups.clone();
            let before = groups.clone();
            for c in groups.entries.clone() {
                ui.horizontal(|ui| {
                    ui.label(&c.name);

                    ui.label(
                        RichText::new(if c.keep {
                            "一直保留"
                        } else {
                            "退出清除"
                        })
                        .small()
                        .weak(),
                    );

                    if ui.small_button("设置规则").clicked() {
                        self.transfer_ui.category = Some((false, c.clone()));
                    }
                    if c.id != FAVORITES {
                        if ui.small_button("↑").clicked() {
                            groups.move_by(c.id, -1);
                        }
                        if ui.small_button("↓").clicked() {
                            groups.move_by(c.id, 1);
                        }
                        if ui.small_button("删除").clicked() {
                            groups.remove(c.id);
                        }
                    }
                });
            }
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut self.transfer_ui.new_category)
                        .hint_text("新分类名称")
                        .desired_width(160.),
                );
                if ui.button("添加分类").clicked() {
                    match groups.add(&self.transfer_ui.new_category) {
                        Ok(_) => {
                            self.transfer_ui.new_category.clear();
                            self.transfer_ui.error.clear();
                        }
                        Err(e) => self.transfer_ui.error = e,
                    }
                }
            });
            if !self.transfer_ui.error.is_empty() {
                ui.colored_label(Color32::LIGHT_RED, &self.transfer_ui.error);
            }
            if groups != before {
                let mut h = self.clipboard.lock();
                for clip in &mut h.items {
                    clip.categories
                        .retain(|id| groups.entries.iter().any(|c| c.id == *id));
                }
                if self
                    .transfer_ui
                    .clip_group
                    .is_some_and(|id| !groups.entries.iter().any(|c| c.id == id))
                {
                    self.transfer_ui.clip_group = None;
                }
                h.groups = groups;
                h.changed();
            }
        });
        if let Some((side, mut category)) = self.transfer_ui.category.clone() {
            let mut open = true;
            let mut save = false;
            egui::Window::new("分类设置与规则")
                .id(egui::Id::new("category-editor"))
                .collapsible(false)
                .resizable(true)
                .open(&mut open)
                .show(ui.ctx(), |ui| {
                    if side || category.id != FAVORITES {
                        ui.horizontal(|ui| {
                            ui.label("名称");
                            ui.text_edit_singleline(&mut category.name);
                        });
                    } else {
                        ui.label("常用分类始终保留，不能改名或删除。");
                    }
                    let mut rgb = transfer::color(&category.color).unwrap_or([156, 185, 255]);
                    ui.horizontal(|ui| {
                        ui.label("颜色");
                        if ui.color_edit_button_srgb(&mut rgb).changed() {
                            category.color = format!("#{:02X}{:02X}{:02X}", rgb[0], rgb[1], rgb[2]);
                        }
                    });
                    if !side && category.id != FAVORITES {
                        ui.checkbox(&mut category.keep, "一直保留");
                    }
                    ui.horizontal(|ui| {
                        ui.label("自动放入条件");
                        ui.selectable_value(&mut category.rule.all, false, "符合任一");
                        ui.selectable_value(&mut category.rule.all, true, "符合全部");
                    });
                    let mut remove = None;
                    for (i, condition) in category.rule.conditions.iter_mut().enumerate() {
                        ui.push_id(i, |ui| {
                            ui.horizontal(|ui| {
                                egui::ComboBox::from_id_salt("field")
                                    .selected_text(condition.field.label())
                                    .width(92.)
                                    .show_ui(ui, |ui| {
                                        for field in transfer::Field::ALL {
                                            ui.selectable_value(
                                                &mut condition.field,
                                                field,
                                                field.label(),
                                            );
                                        }
                                    });
                                egui::ComboBox::from_id_salt("test")
                                    .selected_text(condition.op.label())
                                    .width(70.)
                                    .show_ui(ui, |ui| {
                                        for op in transfer::Op::ALL {
                                            ui.selectable_value(&mut condition.op, op, op.label());
                                        }
                                    });
                                ui.add(
                                    egui::TextEdit::singleline(&mut condition.value)
                                        .desired_width(135.),
                                );
                                if ui.small_button("×").clicked() {
                                    remove = Some(i);
                                }
                            })
                        });
                    }
                    if let Some(i) = remove {
                        category.rule.conditions.remove(i);
                    }
                    if ui.button("添加条件").clicked() {
                        category.rule.conditions.push(Default::default());
                    }
                    let problem = category.rule.problem();
                    if let Some(error) = &problem {
                        ui.colored_label(Color32::LIGHT_RED, error);
                    }
                    save = ui
                        .add_enabled(
                            problem.is_none() && !category.name.trim().is_empty(),
                            egui::Button::new("保存分类"),
                        )
                        .clicked();
                });
            self.transfer_ui.category = if open && !save {
                Some((side, category.clone()))
            } else {
                None
            };
            if save {
                let mut groups = self.clipboard.lock().groups.clone();
                if let Some(i) = groups.entries.iter().position(|c| c.id == category.id) {
                    if category.id == FAVORITES && !side
                        || groups.rename(category.id, &category.name)
                    {
                        groups.entries[i] = category;

                        let mut h = self.clipboard.lock();
                        h.groups = groups;
                        h.changed();
                    } else {
                        self.transfer_ui.error = "已有同名分类".into();
                    }
                }
            }
        }
    }
}
