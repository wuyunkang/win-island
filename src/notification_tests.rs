//! 首轮通知功能的无窗口回归：不会连接私人账号或读取聊天内容。
use crate::{
    app::{App, Messenger, Options},
    extras::AlertSource,
};
use island_events::{ActivityRequest, Kind};
use std::time::Instant;

struct TestApp {
    app: App,
    previous: Option<std::ffi::OsString>,
}
impl std::ops::Deref for TestApp {
    type Target = App;
    fn deref(&self) -> &App {
        &self.app
    }
}
impl std::ops::DerefMut for TestApp {
    fn deref_mut(&mut self) -> &mut App {
        &mut self.app
    }
}
impl Drop for TestApp {
    fn drop(&mut self) {
        match &self.previous {
            Some(value) => std::env::set_var("WIN_ISLAND_DATA_DIR", value),
            None => std::env::remove_var("WIN_ISLAND_DATA_DIR"),
        }
    }
}
fn app() -> TestApp {
    let root = std::env::temp_dir().join(format!(
        "win-island-notification-tests-{}",
        std::process::id()
    ));
    let previous = std::env::var_os("WIN_ISLAND_DATA_DIR");
    std::env::set_var("WIN_ISLAND_DATA_DIR", &root);
    let mut app = App::new(&eframe::egui::Context::default(), Options::default());
    app.settings.notifications = true;
    app.settings.live_api = true;
    app.settings.dingtalk_notifications = true;
    TestApp { app, previous }
}

#[test]
fn win_island_dingtalk_attention_merges_and_clears_when_viewed() {
    let mut app = app();
    app.receive_dingtalk_reminder_from(123, 456);
    app.receive_dingtalk_reminder_from(123, 456);
    assert_eq!(app.pending_messengers.len(), 1);
    app.tick_alerts();
    assert_eq!(app.notice.as_ref().unwrap().title, "钉钉");
    app.viewed_client(Messenger::Dingtalk, 123);
    assert!(app.notice.is_none());
    assert!(app.pending_messengers.is_empty());
    app.settings.dingtalk_notifications = false;
    app.receive_dingtalk_reminder_from(123, 456);
    assert!(app.pending_messengers.is_empty());
}

#[test]
fn win_island_live_activity_is_updated_interrupted_and_restored() {
    let mut app = app();
    let task = |json| serde_json::from_str::<ActivityRequest>(json).unwrap();
    app.activities
        .lock()
        .unwrap()
        .upsert(
            task(r#"{"id":"build","title":"构建","progress":0.2}"#),
            Kind::Activity,
            Instant::now(),
        )
        .unwrap();
    app.tick_alerts();
    assert!(
        matches!(&app.notice.as_ref().unwrap().source, AlertSource::Activity(id) if id=="build")
    );
    app.activities
        .lock()
        .unwrap()
        .upsert(
            task(r#"{"id":"build","title":"构建更新","progress":0.8}"#),
            Kind::Activity,
            Instant::now(),
        )
        .unwrap();
    app.tick_alerts();
    assert_eq!(app.notice.as_ref().unwrap().title, "构建更新");
    app.receive_dingtalk_reminder_from(123, 456);
    app.tick_alerts();
    assert_eq!(app.notice.as_ref().unwrap().title, "钉钉");
    app.viewed_client(Messenger::Dingtalk, 123);
    app.tick_alerts();
    assert_eq!(app.notice.as_ref().unwrap().title, "构建更新");
    app.activities.lock().unwrap().remove("build");
    app.tick_alerts();
    assert!(app.notice.is_none());
}

#[test]
fn win_island_disabling_notifications_clears_live_and_messenger_notices() {
    let mut app = app();
    app.receive_dingtalk_reminder_from(123, 456);
    app.tick_alerts();
    app.settings.notifications = false;
    app.tick_alerts();
    assert!(app.notice.is_none());
    assert!(app.pending_messengers.is_empty());
}

#[test]
fn win_island_dingtalk_toast_and_taskbar_signal_share_one_unread_and_respect_privacy() {
    let mut app = app();
    app.settings.read_notifications = true;
    app.settings.qq_only = false;
    app.settings.notification_preview = false;
    app.receive_notification("钉钉".into(), "DingTalk_9.0".into(), "私人正文".into());
    app.receive_dingtalk_reminder_from(123, 456);
    app.tick_alerts();
    assert_eq!(app.pending_messengers.len(), 1);
    assert_eq!(app.pending_messengers[0].pid, 123);
    assert!(!app.notice.as_ref().unwrap().body.contains("私人正文"));
    app.settings.dingtalk_notifications = false;
    app.tick_alerts();
    assert!(app.notice.is_none());
    assert!(app.pending_messengers.is_empty());
}

#[test]
fn win_island_settings_round_trip_preserves_notifications_and_api_port() {
    let mut app = app();
    app.settings.live_api_port = 17839;
    app.settings.save();
    let loaded = crate::model::Settings::load();
    assert!(loaded.live_api && loaded.dingtalk_notifications);
    assert_eq!(loaded.live_api_port, 17839);
}

#[test]
fn win_island_service_discovery_is_removed_on_shutdown_and_stale_files_are_replaced() {
    let app = app();
    let path = crate::model::config_dir().join("live-api.json");
    let service =
        crate::live_api::Service::start(0, app.activities.clone(), Default::default()).unwrap();
    let first: crate::live_api::Discovery =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert!(first.endpoint.starts_with("http://127.0.0.1:"));
    assert_eq!(first.token.len(), 64);
    drop(service);
    assert!(!path.exists());
    std::fs::write(&path, "stale").unwrap();
    let service =
        crate::live_api::Service::start(0, app.activities.clone(), Default::default()).unwrap();
    let next: crate::live_api::Discovery =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_ne!(first.token, next.token);
    drop(service);
    assert!(!path.exists());
}
