use island_events::{ActivityRequest, Kind, Store};
use std::time::{Duration, Instant};

fn request(json: &str) -> ActivityRequest {
    serde_json::from_str(json).unwrap()
}

#[test]
fn updates_replace_the_same_task_and_keep_other_tasks() {
    let now = Instant::now();
    let mut store = Store::default();
    store
        .upsert(
            request(r#"{"id":"build","title":"构建","progress":0.2}"#),
            Kind::Activity,
            now,
        )
        .unwrap();
    store
        .upsert(
            request(r#"{"id":"download","title":"下载"}"#),
            Kind::Activity,
            now,
        )
        .unwrap();
    store
        .upsert(
            request(r#"{"id":"build","title":"构建","progress":0.8}"#),
            Kind::Activity,
            now,
        )
        .unwrap();
    assert_eq!(store.list(now).len(), 2);
    assert_eq!(store.get("build").unwrap().progress, Some(0.8));
    assert!(store.remove("build"));
    assert!(!store.remove("build"));
    assert_eq!(store.list(now)[0].id, "download");
}

#[test]
fn priority_expiry_and_notification_defaults_choose_the_visible_activity() {
    let now = Instant::now();
    let mut store = Store::default();
    store
        .upsert(
            request(r#"{"id":"long","title":"下载","priority":10}"#),
            Kind::Activity,
            now,
        )
        .unwrap();
    store
        .upsert(
            request(r#"{"id":"alert","title":"部署失败","priority":90}"#),
            Kind::Notification,
            now,
        )
        .unwrap();
    assert_eq!(store.selected(now).unwrap().id, "alert");
    assert_eq!(
        store.selected(now + Duration::from_secs(5)).unwrap().id,
        "long"
    );
    assert_eq!(
        store.list(now + Duration::from_secs(86401)).len(),
        1,
        "持续活动须显式撤销"
    );
}

#[test]
fn updating_a_task_refreshes_its_expiry_and_generated_ids_are_distinct() {
    let now = Instant::now();
    let mut store = Store::default();
    let a = store
        .upsert(request(r#"{"title":"通知"}"#), Kind::Notification, now)
        .unwrap();
    let b = store
        .upsert(request(r#"{"title":"通知"}"#), Kind::Notification, now)
        .unwrap();
    assert_ne!(a, b);
    store
        .upsert(
            request(r#"{"id":"job","title":"任务","duration":3}"#),
            Kind::Activity,
            now,
        )
        .unwrap();
    store
        .upsert(
            request(r#"{"id":"job","title":"更新","duration":3}"#),
            Kind::Activity,
            now + Duration::from_secs(2),
        )
        .unwrap();
    assert!(store
        .list(now + Duration::from_secs(4))
        .iter()
        .any(|a| a.id == "job"));
    assert!(!store
        .list(now + Duration::from_secs(5))
        .iter()
        .any(|a| a.id == "job"));
}

#[test]
fn malformed_payloads_and_capacity_are_rejected_without_losing_tasks() {
    let now = Instant::now();
    let mut store = Store::default();
    for json in [
        r#"{}"#,
        r#"{"title":"  "}"#,
        r#"{"title":"x","progress":1.1}"#,
        r#"{"title":"x","progress":-0.1}"#,
        r#"{"title":"x","priority":100}"#,
        r#"{"title":"x","duration":0}"#,
        r#"{"title":"x","duration":86401}"#,
        r#"{"id":"../bad","title":"x"}"#,
    ] {
        assert!(
            store.upsert(request(json), Kind::Activity, now).is_err(),
            "{json}"
        );
    }
    for i in 0..64 {
        store
            .upsert(
                request(&format!(r#"{{"id":"job-{i}","title":"任务"}}"#)),
                Kind::Activity,
                now,
            )
            .unwrap();
    }
    assert!(store
        .upsert(
            request(r#"{"id":"overflow","title":"x"}"#),
            Kind::Activity,
            now
        )
        .is_err());
    store
        .upsert(
            request(r#"{"id":"job-0","title":"可更新"}"#),
            Kind::Activity,
            now,
        )
        .unwrap();
    assert_eq!(store.list(now).len(), 64);
}

#[test]
fn generated_ids_do_not_overwrite_a_clients_task() {
    let now = Instant::now();
    let mut store = Store::default();
    store
        .upsert(
            request(r#"{"id":"event-2","title":"用户任务"}"#),
            Kind::Activity,
            now,
        )
        .unwrap();
    let generated = store
        .upsert(request(r#"{"title":"临时通知"}"#), Kind::Notification, now)
        .unwrap();
    assert_ne!(generated, "event-2");
    assert_eq!(store.list(now).len(), 2);
}
