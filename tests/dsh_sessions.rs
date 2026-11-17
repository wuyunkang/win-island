use island::agent_sessions::{dsh, AgentState, MessageKind, Source};
use serde_json::json;

#[test]
fn dsh_public_snapshot_is_namespaced_and_waiting_is_an_active_state() {
    let raw = json!({"id":"one","cwd":"F:\\Projects\\demo","title":"修复任务","model":"deepseek-test","state":"waiting","updatedAt":1700000000123i64,"messages":[{"kind":"user","text":"公开请求"},{"kind":"reasoning","text":"内部推理"},{"kind":"answer","text":"公开答复"}],"tools":["read_file"],"context":{"used":2000,"total":10000},"plan":{"completed":1,"total":3}});
    let snapshot = dsh::parse_snapshot(&raw, true).unwrap();
    assert_eq!(snapshot.source, Source::Dsh);
    assert_eq!(snapshot.id, "dsh:one");
    assert_eq!(snapshot.project, "demo");
    assert_eq!(snapshot.state, AgentState::Waiting);
    assert!(snapshot.state.active());
    assert_eq!(snapshot.messages.len(), 2);
    assert_eq!(snapshot.messages[1].kind, MessageKind::Answer);
    assert_eq!(snapshot.activity_ms, 1700000000123);
    assert!(dsh::parse_snapshot(&raw, false)
        .unwrap()
        .messages
        .is_empty());
    assert!(dsh::parse_snapshot(&json!({"id":"../../private"}), true).is_err());
}

#[test]
fn dsh_reader_bounds_body_and_does_not_invent_unknown_metrics() {
    let messages = (0..100)
        .map(|_| json!({"kind":"progress","text":"正文".repeat(10000)}))
        .collect::<Vec<_>>();
    let raw = json!({"id":"big","state":"idle","updatedAt":1700000000000i64,"messages":messages});
    let s = dsh::parse_snapshot(&raw, true).unwrap();
    assert!(s.truncated);
    assert!(s.messages.len() <= 64);
    assert!(s.messages.iter().map(|m| m.text.len()).sum::<usize>() <= 256 * 1024);
    assert!(s.context.is_none());
    assert!(s.plan.is_none());
    assert_eq!(s.state, AgentState::Unknown);
}

#[test]
fn mixed_sources_follow_latest_and_pin_keeps_its_source() {
    use island::agent_sessions::{merge_updates, Request, Snapshot, Update};
    let codex = Snapshot {
        id: "same".into(),
        path: "F:/codex/a.jsonl".into(),
        activity_ms: 100,
        ..Default::default()
    };
    let dsh = dsh::parse_snapshot(
        &json!({"id":"same","state":"working","updatedAt":200}),
        false,
    )
    .unwrap();
    let req = Request {
        root: "F:/codex".into(),
        full: false,
        pinned: None,
        hold: None,
    };
    let c = || Update {
        selected: Some(codex.clone()),
        recent: vec![codex.clone()],
        ready: true,
        ..Default::default()
    };
    let d = || Update {
        selected: Some(dsh.clone()),
        recent: vec![dsh.clone()],
        ready: true,
        ..Default::default()
    };
    let out = merge_updates(c(), d(), &req);
    assert_eq!(out.selected.unwrap().source, Source::Dsh);
    assert_eq!(out.recent.len(), 2);
    let fixed = Request {
        pinned: Some(codex.path.clone()),
        ..req.clone()
    };
    assert_eq!(
        merge_updates(c(), d(), &fixed).selected.unwrap().source,
        Source::Codex
    );
    let failed = Update {
        error: Some("DSH 已关闭".into()),
        ..Default::default()
    };
    assert!(merge_updates(c(), failed, &req).error.is_none());
    let unavailable = Update {
        error: Some("Codex 未安装".into()),
        ..Default::default()
    };
    let empty = Update {
        ready: true,
        dsh_status: "DSH 桥接已连接".into(),
        ..Default::default()
    };
    assert!(merge_updates(unavailable, empty, &req).error.is_none());
}

#[test]
fn dsh_http_client_authenticates_and_rejects_nonlocal_discovery() {
    use std::{
        io::{Read, Write},
        net::TcpListener,
    };
    let root = std::env::temp_dir().join(format!("island-dsh-client-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("bridge.json");
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let token = "a".repeat(64);
    std::fs::write(
        &path,
        json!({"version":1,"endpoint":format!("http://{address}"),"token":token}).to_string(),
    )
    .unwrap();
    let worker = std::thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        socket
            .set_read_timeout(Some(std::time::Duration::from_secs(2)))
            .unwrap();
        let mut bytes = Vec::new();
        loop {
            let mut chunk = [0; 1024];
            let n = socket.read(&mut chunk).unwrap();
            assert!(n > 0);
            bytes.extend_from_slice(&chunk[..n]);
            if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                let headers = String::from_utf8_lossy(&bytes[..end]);
                let length = headers
                    .lines()
                    .find_map(|line| {
                        line.split_once(':')
                            .filter(|(key, _)| key.eq_ignore_ascii_case("content-length"))
                            .and_then(|(_, v)| v.trim().parse::<usize>().ok())
                    })
                    .unwrap_or(0);
                if bytes.len() >= end + 4 + length {
                    break;
                }
            }
        }
        let headers = String::from_utf8_lossy(&bytes);
        assert!(headers.contains(&format!("Bearer {}", "a".repeat(64))));
        assert!(headers.starts_with("POST /snapshot"));
        let s = json!({"id":"main","state":"completed","updatedAt":1000,"messages":[{"kind":"answer","text":"公开答复"}]});
        let body = json!({"version":1,"sessions":[s.clone()],"selected":s}).to_string();
        write!(socket,"HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
    });
    let client = dsh::Client::new(path.clone());
    let out = client.poll(true, None).unwrap();
    assert!(out.ready);
    assert_eq!(out.selected.unwrap().answer(), Some("公开答复"));
    worker.join().unwrap();
    std::fs::write(
        &path,
        json!({"version":1,"endpoint":"http://192.0.2.1:80","token":"a".repeat(64)}).to_string(),
    )
    .unwrap();
    assert!(client.poll(false, None).unwrap_err().contains("本机"));
    std::fs::write(&path, vec![b' '; 8193]).unwrap();
    assert!(client.poll(false, None).is_err());
    std::fs::remove_dir_all(root).unwrap();
}
