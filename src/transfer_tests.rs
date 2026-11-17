use crate::transfer::*;

#[test]
fn clipboard_actions_keep_the_chosen_id_when_history_changes() {
    let mut h = crate::clipboard::History::default();
    h.push("已选内容", "10:00");
    let id = h.items[0].id;
    h.push("随后复制的新内容", "10:01");
    assert_eq!(h.get_id(id).unwrap().text, "已选内容");
    h.pin_id(id);
    assert!(h.get_id(id).unwrap().pinned);
    h.remove_id(id);
    assert_eq!(h.items.len(), 1);
    assert_eq!(h.items[0].text, "随后复制的新内容");
    assert!(h.get_id(id).is_none());
}

#[test]
fn clipboard_common_phrase_promotes_an_existing_record_and_keeps_favorite_rules() {
    let mut h = crate::clipboard::History::default();
    h.push("常用回复", "10:00");
    assert!(h.add_phrase("常用回复"));
    assert_eq!(h.items.len(), 1);
    assert!(h.items[0].manual && h.items[0].pinned);
    h.groups.entries[0].color = "#ABCDEF".into();
    h.groups.entries[0].rule = Rule {
        all: false,
        conditions: vec![Condition {
            field: Field::Text,
            op: Op::Contains,
            value: "固定".into(),
        }],
    };
    let reopened = crate::clipboard::History::from_json(&h.to_json());
    assert_eq!(reopened.groups, h.groups);
}

#[test]
fn transfer_classifies_links_code_colors_and_plain_text() {
    assert_eq!(classify("https://example.com/path"), Kind::Link);
    assert_eq!(classify("#9CB9FF"), Kind::Color);
    assert_eq!(
        classify("fn main() {\n println!(\"hello\");\n}"),
        Kind::Code
    );
    assert_eq!(classify("今天的会议"), Kind::Text);
}

#[test]
fn clipboard_icons_distinguish_text_office_folders_and_archives() {
    use crate::clipboard_icon::{icon_for,IconKind};
    use std::path::PathBuf;
    assert_eq!(icon_for(Kind::Text,&[],None),IconKind::Text);
    assert_eq!(icon_for(Kind::Link,&[],None),IconKind::Link);
    assert_eq!(icon_for(Kind::Code,&[],None),IconKind::Code);
    for (name,expected) in [
        ("项目计划.DOCX",IconKind::Word),("budget.xlsx",IconKind::Excel),
        ("汇报.pptx",IconKind::PowerPoint),("backup.tar.gz",IconKind::Archive),
        ("backup.7Z",IconKind::Archive),("report.pdf",IconKind::File),
    ] {assert_eq!(icon_for(Kind::Files,&[PathBuf::from(name)],Some(false)),expected,"{name}");}
    assert_eq!(icon_for(Kind::Files,&[PathBuf::from("看起来像.zip")],Some(true)),IconKind::Folder,"文件夹扩展名不能覆盖实际类型");
    assert_eq!(icon_for(Kind::Files,&[PathBuf::from("无扩展名")],Some(false)),IconKind::File,"无扩展名的文件不是文件夹");
    assert_eq!(icon_for(Kind::Files,&[PathBuf::from("one.docx"),PathBuf::from("two.zip")],None),IconKind::Files,"多文件记录应显示组合图标");
    assert_eq!(icon_for(Kind::Files,&[PathBuf::from("离线.docx")],None),IconKind::Word,"文件暂时不可用时仍可按扩展名显示图标");
}

#[test]
fn clipboard_icon_directory_lookup_uses_real_file_type() {
    use std::time::{Duration,Instant};
    let root=std::env::temp_dir().join(format!("island-clip-icon-{}",std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let folder=root.join("folder.zip");let file=root.join("no-extension");
    std::fs::create_dir_all(&folder).unwrap();std::fs::write(&file,b"hello").unwrap();
    let ctx=eframe::egui::Context::default();let mut cache=crate::clipboard_icon::DirectoryCache::default();
    for (path,expected) in [(&folder,true),(&file,false)] {
        let deadline=Instant::now()+Duration::from_secs(2);
        loop {if let Some(actual)=cache.get(&ctx,path){assert_eq!(actual,expected);break;}
            assert!(Instant::now()<deadline,"异步目录检查未完成");std::thread::sleep(Duration::from_millis(2));}
    }
    std::fs::remove_file(file).unwrap();std::fs::remove_dir(folder).unwrap();std::fs::remove_dir(root).unwrap();
}
#[test]
fn transfer_rules_support_all_any_regex_domain_and_source() {
    let value = ItemInfo {
        kind: "链接".into(),
        text: "https://docs.example.com/api".into(),
        source: "Code.exe".into(),
        path: String::new(),
    };
    let mut rule = Rule {
        all: true,
        conditions: vec![
            Condition {
                field: Field::Domain,
                op: Op::Contains,
                value: "example.com".into(),
            },
            Condition {
                field: Field::Source,
                op: Op::Contains,
                value: "code".into(),
            },
        ],
    };
    assert!(rule.matches(&value));
    rule.conditions[1].value = "wechat".into();
    assert!(!rule.matches(&value));
    rule.all = false;
    assert!(rule.matches(&value));
    rule.conditions = vec![Condition {
        field: Field::Text,
        op: Op::Regex,
        value: "[".into(),
    }];
    assert!(rule.problem().is_some());
    assert!(!rule.matches(&value));
    assert!(!Rule::default().matches(&value));
}
#[test]
fn transfer_categories_keep_favorites_and_allow_multiple_memberships() {
    let mut groups = Groups::clipboard();
    let a = groups.add("工作").unwrap();
    let b = groups.add("代码").unwrap();
    assert!(groups.rename(a, "项目"));
    assert!(!groups.remove(FAVORITES));
    assert!(!groups.rename(FAVORITES, "错误"));
    assert!(!groups.add("项目").is_ok());
    groups.entries.iter_mut().find(|g| g.id == a).unwrap().keep = true;
    assert!(groups.kept(&[a, b]));
    assert!(!groups.kept(&[b]));
    assert!(groups.kept(&[FAVORITES]));
    assert!(groups.move_by(b, -1));
    assert_eq!(groups.entries[0].id, FAVORITES);
}
#[test]
fn transfer_multiselect_supports_control_shift_and_visible_order() {
    let mut selection = Selection::default();
    let ids = vec!["a".into(), "b".into(), "c".into(), "d".into()];
    selection.click(1, &ids, false, false);
    selection.click(3, &ids, true, false);
    assert_eq!(selection.ordered(&ids), vec!["b", "d"]);
    selection.click(0, &ids, false, true);
    assert_eq!(selection.ordered(&ids), vec!["a", "b", "c", "d"]);
    selection.click(1, &ids, true, false);
    assert_eq!(selection.ordered(&ids), vec!["a", "c", "d"]);
}

#[test]
fn clipboard_v2_favorites_and_kept_groups_survive_session_cleanup() {
    let mut history = crate::clipboard::History::default();
    history.push("临时", "10:00");
    history.push("保留", "10:01");
    let category = history.groups.add("项目").unwrap();
    history
        .groups
        .entries
        .iter_mut()
        .find(|c| c.id == category)
        .unwrap()
        .keep = true;
    history.items[0].categories.push(category);
    history.add_phrase("常用语");
    history.clear_session();
    assert_eq!(history.items.len(), 2);
    assert!(history.items.iter().any(|c| c.manual && c.pinned));
    assert!(!history.items.iter().any(|c| c.text == "临时"));
}

#[test]
fn clipboard_v2_source_exclusions_and_rules_are_applied_before_recording() {
    let shared = crate::clipboard::Shared {
        history: Default::default(),
        enabled: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true)),
        options: Default::default(),
    };
    shared.configure(vec!["Weixin.exe".into()], true, 100);
    assert!(!shared.record_source("秘密", "WEIXIN.EXE", "", ""));
    let id = shared.lock().groups.add("代码").unwrap();
    shared
        .lock()
        .groups
        .entries
        .iter_mut()
        .find(|c| c.id == id)
        .unwrap()
        .rule = Rule {
        all: false,
        conditions: vec![Condition {
            field: Field::Kind,
            op: Op::Equals,
            value: "代码".into(),
        }],
    };
    assert!(shared.record_source("fn main() {}", "Code.exe", "<pre>fn main() {}</pre>", ""));
    let history = shared.lock();
    assert_eq!(history.items[0].source, "Code.exe");
    assert_eq!(history.items[0].categories, vec![id]);
    assert!(!history.items[0].html.is_empty());
}

#[test]
fn clipboard_images_and_rich_text_round_trip_without_losing_metadata() {
    use crate::clipboard::{Clip, History, Picture};
    let mut png = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
        2,
        3,
        image::Rgba([10, 20, 30, 255]),
    ))
    .write_to(&mut png, image::ImageFormat::Png)
    .unwrap();
    let mut h = History::default();
    assert!(h.push_clip(Clip {
        picture: Some(Picture {
            width: 2,
            height: 3,
            png: std::sync::Arc::new(png.into_inner())
        }),
        source: "Paint.exe".into(),
        pinned: true,
        ..Default::default()
    }));
    assert!(h.push_clip(Clip {
        text: "文字".into(),
        html: "<b>文字</b>".into(),
        rtf: "{\\rtf1 example}".into(),
        ..Default::default()
    }));
    let reopened = History::from_json(&h.to_json());
    assert_eq!(reopened.items, h.items);
    assert_eq!(reopened.items[1].kind, Kind::Image);
}
#[test]
fn copied_files_are_typed_history_records_and_round_trip_as_a_group() {
    use crate::clipboard::{Clip,History};
    let paths=vec![std::path::PathBuf::from(r"C:\资料\设计方案.pdf"),std::path::PathBuf::from(r"C:\资料\项目文件夹")];
    let mut history=History::default();
    assert!(history.push_clip(Clip{files:paths.clone(),source:"explorer.exe".into(),..Default::default()}));
    assert_eq!(history.items[0].kind,crate::transfer::Kind::Files);
    assert!(history.items[0].preview().contains("设计方案.pdf"));assert!(history.items[0].preview().contains("2"));
    assert!(!history.push_clip(Clip{files:paths.clone(),source:"explorer.exe".into(),..Default::default()}));
    history.push("文字","10:00");history.push_clip(Clip{files:paths.clone(),source:"explorer.exe".into(),..Default::default()});
    assert_eq!(history.items.len(),2);assert_eq!(history.items[0].files,paths);
    history.items[0].pinned=true;
    let restored=History::from_json(&history.to_json());assert_eq!(restored.items[0].files,paths);assert_eq!(restored.items[0].kind,crate::transfer::Kind::Files);
    assert!(history.push_clip(Clip{text:r"C:\资料\设计方案.pdf".into(),..Default::default()}),"文件记录与同内容的路径文字必须分开");
}

#[test]
fn copied_files_follow_clipboard_switch_and_source_exclusions() {
    let shared=crate::clipboard::Shared{history:Default::default(),enabled:std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true)),options:Default::default()};
    let paths=vec![std::path::PathBuf::from(r"C:\项目\a.txt")];
    shared.configure(vec!["Private.exe".into()],true,100);
    assert!(!shared.record_files(paths.clone(),"Private.exe"));assert!(shared.record_files(paths.clone(),"explorer.exe"));
    shared.set_enabled(false);assert!(!shared.record_files(paths,"explorer.exe"));assert_eq!(shared.lock().items.len(),1);
}
