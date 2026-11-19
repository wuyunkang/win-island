use island::agent_sessions::{AgentState, MessageKind, Parser};
use serde_json::json;

fn event(p: &mut Parser, at: i64, payload: serde_json::Value) {
    p.ingest(&json!({"timestamp":chrono::DateTime::from_timestamp(at,0).unwrap().to_rfc3339(),"type":"event_msg","payload":payload}));
}
#[test]
fn agent_question_tool_waits_for_its_own_reply_without_retaining_arguments() {
    for full in [false,true] {
        let mut p=Parser::new(full);
        event(&mut p,100,json!({"type":"task_started","turn_id":"question-turn"}));
        p.ingest(&json!({"timestamp":"1970-01-01T00:01:41Z","type":"response_item","payload":{"type":"function_call","name":"functions.request_user_input","call_id":"question","arguments":"私密询问参数"}}));
        assert_eq!(p.snapshot().state,AgentState::Waiting,"明确的询问工具调用应进入待确认状态");
        p.ingest(&json!({"timestamp":"1970-01-01T00:01:42Z","type":"response_item","payload":{"type":"function_call_output","call_id":"other","output":"其他工具结果"}}));
        assert_eq!(p.snapshot().state,AgentState::Waiting);
        assert!(p.snapshot().messages.is_empty(),"通知无需保存工具参数或结果");
        event(&mut p,102,json!({"type":"agent_message","phase":"commentary","message":"请确认"}));
        assert_eq!(p.snapshot().state,AgentState::Waiting,"公开进展不能覆盖尚未答复的询问");
        p.ingest(&json!({"timestamp":"1970-01-01T00:01:43Z","type":"response_item","payload":{"type":"function_call_output","call_id":"question","output":"私密用户选项"}}));
        assert_eq!(p.snapshot().state,AgentState::Working,"用户答复后退出等待状态");
        assert!(p.snapshot().messages.iter().all(|m|m.text=="请确认"));
        event(&mut p,104,json!({"type":"task_complete"}));
        p.ingest(&json!({"timestamp":"1970-01-01T00:01:45Z","type":"response_item","payload":{"type":"function_call","name":"request_user_input","call_id":"late","arguments":"{}"}}));
        assert_eq!(p.snapshot().state,AgentState::Completed,"完成后镜像工具不能覆盖结束状态");
    }
}
#[test]
fn agent_async_question_keeps_waiting_after_ack_until_the_user_replies() {
    let mut p=Parser::new(false);
    event(&mut p,100,json!({"type":"task_started","turn_id":"async-question"}));
    p.ingest(&json!({"timestamp":"1970-01-01T00:01:41Z","type":"response_item","payload":{"type":"function_call","name":"functions.request_user_input_async","call_id":"question","arguments":"私密询问参数"}}));
    assert_eq!(p.snapshot().state,AgentState::Waiting);
    p.ingest(&json!({"timestamp":"1970-01-01T00:01:42Z","type":"response_item","payload":{"type":"function_call_output","call_id":"question","output":"{\"accepted\":true}"}}));
    assert_eq!(p.snapshot().state,AgentState::Waiting,"异步受理回执不是用户答复");
    event(&mut p,103,json!({"type":"agent_reasoning"}));
    assert_eq!(p.snapshot().state,AgentState::Waiting,"继续处理其他工作时仍需保留询问提醒");
    event(&mut p,104,json!({"type":"task_complete"}));
    assert_eq!(p.snapshot().state,AgentState::Waiting,"仍有询问时不能误报完成");
    event(&mut p,105,json!({"type":"user_message","message":"选择第一项"}));
    assert_eq!(p.snapshot().state,AgentState::Thinking);
    event(&mut p,106,json!({"type":"task_started","turn_id":"after-reply"}));
    event(&mut p,107,json!({"type":"task_complete"}));
    assert_eq!(p.snapshot().state,AgentState::Completed);
    assert!(p.snapshot().messages.is_empty());
}
#[test]
fn agent_cli_latest_turn_deduplicates_mirrors_and_keeps_terminal_state() {
    let mut p = Parser::new(true);
    event(
        &mut p,
        100,
        json!({"type":"user_message","message":"检查内存"}),
    );
    event(
        &mut p,
        101,
        json!({"type":"task_started","turn_id":"turn-1"}),
    );
    event(
        &mut p,
        102,
        json!({"type":"agent_message","message":"正在检查","phase":"commentary"}),
    );
    p.ingest(&json!({"timestamp":"1970-01-01T00:01:42Z","type":"response_item","payload":{"type":"message","role":"assistant","phase":"commentary","content":[{"type":"output_text","text":"正在检查"}]}}));
    event(
        &mut p,
        103,
        json!({"type":"task_complete","last_agent_message":"已释放资源"}),
    );
    event(
        &mut p,
        104,
        json!({"type":"agent_message","message":"已释放资源","phase":"final"}),
    );
    let s = p.snapshot();
    assert_eq!(s.state, AgentState::Completed);
    assert_eq!(s.messages.len(), 3);
    assert_eq!(s.messages[0].kind, MessageKind::User);
    assert_eq!(s.messages[0].text, "检查内存");
    assert_eq!(s.messages[2].text, "已释放资源");
    let activity = s.updated_at;
    event(
        &mut p,
        200,
        json!({"type":"token_count","info":{"total_token_usage":123}}),
    );
    assert_eq!(p.snapshot().updated_at, activity, "额度更新不能抢占会话");
}
#[test]
fn agent_desktop_public_messages_only_and_new_turn_clears_old_body() {
    let mut p = Parser::new(true);
    p.ingest(&json!({"type":"session_meta","payload":{"id":"desktop","cwd":"F:\\Projects\\demo","originator":"Codex Desktop","source":"vscode"}}));
    p.ingest(&json!({"type":"turn_context","payload":{"model":"gpt-test"}}));
    event(&mut p, 100, json!({"type":"task_started","turn_id":"a"}));
    event(
        &mut p,
        101,
        json!({"type":"item_completed","item":{"type":"UserMessage","content":[{"type":"input_text","text":"真正的请求"}]}}),
    );
    event(
        &mut p,
        102,
        json!({"type":"item_completed","item":{"type":"Reasoning","text":"不能显示的内部推理"}}),
    );
    p.ingest(&json!({"timestamp":"1970-01-01T00:01:43Z","type":"response_item","payload":{"type":"message","role":"developer","content":[{"text":"内部指令"}]}}));
    event(
        &mut p,
        104,
        json!({"type":"item_completed","item":{"type":"AgentMessage","text":"公开答复","phase":"final"}}),
    );
    event(&mut p, 105, json!({"type":"turn_complete"}));
    let s = p.snapshot();
    assert_eq!(s.project, "demo");
    assert_eq!(s.model, "gpt-test");
    assert_eq!(s.started_at, Some(100));
    assert_eq!(s.turn_id, "a");
    assert_eq!(
        s.messages
            .iter()
            .map(|m| m.text.as_str())
            .collect::<Vec<_>>(),
        ["真正的请求", "公开答复"]
    );
    event(
        &mut p,
        110,
        json!({"type":"user_message","message":"下一轮"}),
    );
    event(&mut p, 111, json!({"type":"task_started","turn_id":"b"}));
    assert_eq!(p.snapshot().messages.len(), 1);
    assert_eq!(p.snapshot().messages[0].text, "下一轮");
    assert_eq!(p.snapshot().state, AgentState::Thinking);
}
#[test]
fn agent_metadata_mode_retains_no_body_and_body_has_a_hard_limit() {
    let mut lite = Parser::new(false);
    event(
        &mut lite,
        100,
        json!({"type":"user_message","message":"摘要"}),
    );
    assert!(lite.snapshot().messages.is_empty());
    assert_eq!(lite.snapshot().title, "摘要");
    let mut full = Parser::new(true);
    event(&mut full, 100, json!({"type":"task_started"}));
    for i in 0..100 {
        event(
            &mut full,
            101 + i,
            json!({"type":"agent_message","message":format!("{i}{}","正文".repeat(6000)),"phase":"commentary"}),
        );
    }
    let s = full.snapshot();
    assert!(s.truncated);
    assert!(s.messages.len() <= 64);
    assert!(s.messages.iter().map(|m| m.text.len()).sum::<usize>() <= 256 * 1024);
}
#[test]
fn agent_environment_injections_and_encrypted_reasoning_are_never_messages() {
    let mut p = Parser::new(true);
    for text in [
        "# AGENTS.md instructions for project",
        "<environment_context>private</environment_context>",
        "<turn_aborted>aborted</turn_aborted>",
    ] {
        event(&mut p, 100, json!({"type":"user_message","message":text}));
    }
    p.ingest(&json!({"type":"response_item","payload":{"type":"reasoning","encrypted_content":"secret","summary":[{"text":"secret"}]}}));
    assert!(p.snapshot().messages.is_empty());
}

struct Fixture {
    root: std::path::PathBuf,
}
impl Fixture {
    fn new(name: &str) -> Self {
        let root = std::env::temp_dir().join(format!("island-agent-{name}-{}", std::process::id()));
        std::fs::create_dir_all(root.join("sessions/2026/11/16")).unwrap();
        Self { root }
    }
    fn path(&self, name: &str) -> std::path::PathBuf {
        self.root
            .join("sessions/2026/11/16")
            .join(format!("rollout-{name}.jsonl"))
    }
    fn write(&self, name: &str, at: i64) -> std::path::PathBuf {
        self.write_in(name, at, "F:\\demo")
    }
    fn write_in(&self, name: &str, at: i64, cwd: &str) -> std::path::PathBuf {
        let path = self.path(name);
        let text = format!(
            "{}\n{}\n{}\n",
            json!({"type":"session_meta","payload":{"id":name,"cwd":cwd}}),
            json!({"timestamp":chrono::DateTime::from_timestamp(at,0).unwrap().to_rfc3339(),"type":"event_msg","payload":{"type":"task_started","turn_id":name}}),
            json!({"timestamp":chrono::DateTime::from_timestamp(at+1,0).unwrap().to_rfc3339(),"type":"event_msg","payload":{"type":"user_message","message":format!("请求{name}")}})
        );
        std::fs::write(&path, text).unwrap();
        path
    }
    fn project_state(&self, state: serde_json::Value) {
        let path = self.root.join(".codex-global-state.json");
        // 固定递增修改时间，确保相同字节数的改名不依赖文件系统时钟精度。
        let modified = std::fs::metadata(&path)
            .and_then(|m| m.modified())
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
            + std::time::Duration::from_secs(1);
        std::fs::write(&path, state.to_string()).unwrap();
        std::fs::OpenOptions::new()
            .write(true)
            .open(path)
            .unwrap()
            .set_times(std::fs::FileTimes::new().set_modified(modified))
            .unwrap();
    }
    fn request(&self, full: bool) -> island::agent_sessions::Request {
        island::agent_sessions::Request {
            root: self.root.clone(),
            full,
            pinned: None,
            hold: None,
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        assert!(self.root.starts_with(std::env::temp_dir()));
        let _ = std::fs::remove_dir_all(&self.root);
    }
}
fn append(path: &std::path::Path, bytes: &[u8]) {
    use std::io::Write;
    std::fs::OpenOptions::new()
        .append(true)
        .open(path)
        .unwrap()
        .write_all(bytes)
        .unwrap();
}
fn record(at: i64, payload: serde_json::Value) -> String {
    format!(
        "{}\n",
        json!({"timestamp":chrono::DateTime::from_timestamp(at,0).unwrap().to_rfc3339(),"type":"event_msg","payload":payload})
    )
}

#[test]
fn agent_reader_excludes_subagents_in_either_metadata_field() {
    let fixture = Fixture::new("subagent-metadata");
    let parent = fixture.write("parent", 100);
    let mut request = fixture.request(true);
    let mut reader = island::agent_sessions::Reader::new(fixture.root.clone());
    for (index, (source, thread_source)) in [
        (json!("vscode"), json!("subagent")),
        (json!("subagent"), json!("user")),
        (json!({"subagent":{"thread_spawn":{}}}), json!("subagent")),
        (json!("vscode"), json!({"subagent":{"thread_spawn":{}}})),
    ].into_iter().enumerate() {
        let id = format!("child-{index}");
        let path = fixture.path(&id);
        let meta = json!({"type":"session_meta","payload":{"id":id,"source":source,"thread_source":thread_source}});
        std::fs::write(&path, format!("{meta}\n{}", record(200, json!({"type":"task_started","turn_id":"child-turn"})))).unwrap();
        let running = reader.poll(&request);
        assert_eq!(running.latest.as_ref(), Some(&parent), "子代理开始执行时不能抢占主会话");
        assert_eq!(running.recent.len(), 1);

        append(&path, record(201, json!({"type":"task_complete","turn_id":"child-turn"})).as_bytes());
        let finished = reader.poll(&request);
        assert_eq!(finished.latest.as_ref(), Some(&parent));
        assert_eq!(finished.recent.len(), 1);

        request.pinned = Some(path);
        assert!(reader.poll(&request).selected.is_none(), "旧配置固定的子代理也不能选中");
        request.pinned = None;
    }
}

#[test]
fn agent_reader_uses_semantic_activity_retries_utf8_partial_and_releases_body() {
    let f = Fixture::new("incremental");
    let a = f.write("a", 100);
    let b = f.write("b", 200);
    let mut r = island::agent_sessions::Reader::new(f.root.clone());
    let mut req = f.request(true);
    assert_eq!(r.poll(&req).selected.unwrap().id, "b");
    append(&a, record(300, json!({"type":"token_count"})).as_bytes());
    assert_eq!(r.poll(&req).selected.unwrap().id, "b", "额度写入不抢占");
    let line = record(
        400,
        json!({"type":"agent_message","message":"新增中文答复","phase":"final"}),
    );
    let cut = line.find("文").unwrap() + 1;
    append(&b, &line.as_bytes()[..cut]);
    assert!(r.poll(&req).selected.unwrap().answer().is_none());
    append(&b, &line.as_bytes()[cut..]);
    assert_eq!(
        r.poll(&req).selected.unwrap().answer(),
        Some("新增中文答复")
    );
    assert!(r.retained_body_bytes() > 0);
    req.full = false;
    assert!(r.poll(&req).selected.unwrap().messages.is_empty());
    assert_eq!(r.retained_body_bytes(), 0);
    req.full = true;
    assert_eq!(
        r.poll(&req).selected.unwrap().answer(),
        Some("新增中文答复")
    );
    f.write("b", 500);
    let s = r.poll(&req).selected.unwrap();
    assert_eq!(s.messages.len(), 1);
    assert!(s.answer().is_none(), "重写后不能混入旧答复");
    req.pinned = Some(a);
    assert_eq!(r.poll(&req).selected.unwrap().id, "a");
}
#[test]
fn agent_reader_pin_survives_recent_candidate_limit_and_missing_source_is_honest() {
    let f = Fixture::new("bounded");
    let pinned = f.write("000", 100);
    for i in 1..=40 {
        f.write(&format!("{i:03}"), 1000 + i);
    }
    let mut req = f.request(true);
    req.pinned = Some(pinned.clone());
    let mut r = island::agent_sessions::Reader::new(f.root.clone());
    let out = r.poll(&req);
    assert_eq!(out.recent.len(), 32);
    assert_eq!(out.selected.unwrap().id, "000");
    std::fs::remove_file(pinned).unwrap();
    assert!(r.poll(&req).error.is_some());
    let mut missing = island::agent_sessions::Reader::new(f.root.join("missing"));
    assert!(missing.poll(&req).selected.is_none());
}
#[test]
fn agent_duplicate_user_and_private_analysis_do_not_reset_or_leak() {
    let mut p = Parser::new(true);
    event(&mut p, 100, json!({"type":"task_started","turn_id":"a"}));
    event(&mut p, 101, json!({"type":"user_message","message":"请求"}));
    event(
        &mut p,
        102,
        json!({"type":"agent_message","message":"公开进展","phase":"commentary"}),
    );
    event(
        &mut p,
        103,
        json!({"type":"item_completed","item":{"type":"UserMessage","content":[{"text":"请求"}]}}),
    );
    event(
        &mut p,
        104,
        json!({"type":"agent_message","message":"内部分析","phase":"analysis"}),
    );
    assert_eq!(p.snapshot().messages.len(), 2);
    assert_eq!(p.snapshot().state, AgentState::Working);
}
#[test]
fn agent_worker_is_single_and_drop_releases_lane_and_reopen_catches_up() {
    let f = Fixture::new("worker");
    let path = f.write("a", 100);
    let ctx = eframe::egui::Context::default();
    for _ in 0..3 {
        let service = island::agent_sessions::Service::start(f.request(true), ctx.clone()).unwrap();
        assert_eq!(island::resources::AGENT_SESSION.active(), 1);
        assert!(island::agent_sessions::Service::start(f.request(false), ctx.clone()).is_none());
        let start = std::time::Instant::now();
        let out = loop {
            if let Some(out) = service.poll() {
                break out;
            }
            assert!(start.elapsed().as_secs() < 3);
            std::thread::sleep(std::time::Duration::from_millis(10));
        };
        assert_eq!(out.selected.unwrap().id, "a");
        drop(service);
        assert_eq!(island::resources::AGENT_SESSION.active(), 0);
        append(
            &path,
            record(
                300,
                json!({"type":"agent_message","message":"后台更新","phase":"final"}),
            )
            .as_bytes(),
        );
    }
    #[cfg(windows)]
    {
        for i in 1..=40 {
            f.write(&format!("{i:03}"), 1000 + i);
        }
        let old = f.write("000", 50);
        let service = island::agent_sessions::Service::start(f.request(false), ctx).unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        while service.poll().is_none() {
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        append(
            &old,
            record(
                9000,
                json!({"type":"agent_message","message":"恢复旧会话","phase":"commentary"}),
            )
            .as_bytes(),
        );
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(1800);
        loop {
            if service
                .poll()
                .is_some_and(|out| out.selected.as_ref().is_some_and(|s| s.id == "000"))
            {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "旧会话恢复时必须根据文件通知进入候选"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        drop(service);
        assert_eq!(island::resources::AGENT_SESSION.active(), 0);
    }
}

#[test]
fn agent_large_metadata_keeps_project_and_identity_without_replaying_history() {
    let f = Fixture::new("large-meta");
    let path = f.path("large");
    let metadata = json!({"type":"session_meta","payload":{"id":"large","cwd":"F:\\Projects\\win-island-rust","originator":"Codex Desktop","base_instructions":"内部指令".repeat(8000)}});
    std::fs::write(
        &path,
        format!(
            "{metadata}\n{}",
            record(100, json!({"type":"user_message","message":"公开请求"}))
        ),
    )
    .unwrap();
    let mut reader = island::agent_sessions::Reader::new(f.root.clone());
    let out = reader.poll(&f.request(true));
    let s = out.selected.unwrap();
    assert_eq!(s.id, "large");
    assert_eq!(s.project, "win-island-rust");
    assert_eq!(s.messages.len(), 1);
    assert_eq!(s.messages[0].text, "公开请求");
}

#[test]
fn agent_follow_uses_milliseconds_and_hold_does_not_hide_new_session() {
    let f = Fixture::new("milliseconds");
    let a = f.write("a", 100);
    let b = f.write("b", 100);
    let line = |stamp: &str, text: &str| {
        format!(
            "{}\n",
            json!({"timestamp":stamp,"type":"event_msg","payload":{"type":"agent_message","message":text,"phase":"commentary"}})
        )
    };
    append(
        &a,
        line("1970-01-01T00:01:50.900Z", "稍后的进展").as_bytes(),
    );
    append(
        &b,
        line("1970-01-01T00:01:50.100Z", "稍早的进展").as_bytes(),
    );
    let mut reader = island::agent_sessions::Reader::new(f.root.clone());
    let mut req = f.request(true);
    let out = reader.poll(&req);
    assert_eq!(out.selected.unwrap().id, "a");
    req.hold = Some(b.clone());
    let out = reader.poll(&req);
    assert_eq!(out.selected.unwrap().id, "b");
    assert_eq!(out.latest, Some(a));
    req.pinned = Some(b);
    assert_eq!(reader.poll(&req).selected.unwrap().id, "b");
}

#[test]
fn agent_removing_all_sessions_releases_the_selected_body() {
    let f = Fixture::new("remove-all");
    let path = f.write("a", 100);
    let mut reader = island::agent_sessions::Reader::new(f.root.clone());
    assert!(reader.poll(&f.request(true)).selected.is_some());
    assert!(reader.retained_body_bytes() > 0);
    std::fs::remove_file(path).unwrap();
    assert!(reader.poll(&f.request(true)).selected.is_none());
    assert_eq!(reader.retained_body_bytes(), 0);
}

#[test]
fn agent_project_name_replaces_folder_in_summary_body_and_recent_sessions() {
    let f = Fixture::new("project-name");
    f.write_in("named", 200, "f:/projects/win-island-rust/src/");
    f.write_in("unrelated", 100, "E:\\Other\\win-island-rust");
    f.project_state(json!({"local-projects":{
        "island":{"name":"灵动岛","rootPaths":["F:\\Projects\\win-island-rust\\"]}
    }}));
    std::fs::write(
        f.root.join("session_index.jsonl"),
        format!("{}\n", json!({"id":"named","thread_name":"修复项目名称"})),
    )
    .unwrap();
    let mut reader = island::agent_sessions::Reader::new(f.root.clone());
    for full in [false, true, false] {
        let out = reader.poll(&f.request(full));
        let selected = out.selected.unwrap();
        assert_eq!(selected.project, "灵动岛");
        assert_eq!(selected.title, "修复项目名称");
        assert_eq!(selected.messages.is_empty(), !full);
        assert_eq!(out.recent[0].project, "灵动岛");
        assert_eq!(out.recent[1].project, "win-island-rust");
    }
}

#[test]
fn agent_project_assignment_handles_shared_folders_and_worktrees() {
    let f = Fixture::new("project-assignment");
    f.write_in("shared", 100, "F:\\Projects\\demo");
    f.write_in("worktree", 200, "C:\\Codex\\worktrees\\abc\\demo");
    f.project_state(json!({
        "local-projects":{
            "a":{"name":"第一个项目","rootPaths":["F:\\Projects\\demo"]},
            "b":{"name":"第二个项目","rootPaths":["F:\\Projects\\demo"]}
        },
        "thread-project-assignments":{
            "shared":{"projectKind":"local","projectId":"b"},
            "worktree":{"projectKind":"local","projectId":"a"}
        }
    }));
    let mut reader = island::agent_sessions::Reader::new(f.root.clone());
    let out = reader.poll(&f.request(true));
    assert_eq!(out.selected.unwrap().project, "第一个项目");
    assert_eq!(out.recent[0].project, "第一个项目");
    assert_eq!(out.recent[1].project, "第二个项目");
}

#[test]
fn agent_project_paths_use_all_roots_and_the_closest_parent_directory() {
    let f = Fixture::new("project-roots");
    f.write_in("parent", 100, "F:\\Projects\\demo");
    f.write_in("second", 200, "E:\\Documents\\notes");
    f.write_in("nested", 300, "F:\\Projects\\demo\\child\\src");
    f.write_in("neighbor", 400, "F:\\Projects\\demo-other");
    f.project_state(json!({"local-projects":{
        "parent":{"name":"制造系统","rootPaths":["F:\\Projects\\demo","E:\\Documents"]},
        "child":{"name":"子项目","rootPaths":["F:\\Projects\\demo\\child"]}
    }}));
    let mut reader = island::agent_sessions::Reader::new(f.root.clone());
    let out = reader.poll(&f.request(false));
    assert_eq!(out.selected.unwrap().project, "demo-other");
    let projects: Vec<_> = out.recent.iter().map(|s| s.project.as_str()).collect();
    assert_eq!(projects, ["demo-other", "子项目", "制造系统", "制造系统"]);
}

#[test]
fn agent_project_rename_and_removal_refresh_without_new_session_events() {
    let f = Fixture::new("project-rename");
    f.write_in("named", 100, "F:\\Projects\\win-island-rust");
    let state = |name| {
        json!({"local-projects":{
            "island":{"name":name,"rootPaths":["F:\\Projects\\win-island-rust"]}
        }})
    };
    f.project_state(state("灵动岛"));
    let mut reader = island::agent_sessions::Reader::new(f.root.clone());
    assert_eq!(
        reader.poll(&f.request(true)).selected.unwrap().project,
        "灵动岛"
    );
    let before = std::fs::metadata(f.root.join(".codex-global-state.json"))
        .unwrap()
        .len();
    f.project_state(state("浮动岛"));
    assert_eq!(
        std::fs::metadata(f.root.join(".codex-global-state.json"))
            .unwrap()
            .len(),
        before
    );
    let out = reader.poll(&f.request(true));
    assert_eq!(out.selected.unwrap().project, "浮动岛");
    assert_eq!(out.recent[0].project, "浮动岛");
    f.project_state(state("  "));
    assert_eq!(
        reader.poll(&f.request(false)).selected.unwrap().project,
        "win-island-rust"
    );
    f.project_state(state("灵动岛"));
    assert_eq!(
        reader.poll(&f.request(false)).selected.unwrap().project,
        "灵动岛"
    );
    std::fs::remove_file(f.root.join(".codex-global-state.json")).unwrap();
    assert_eq!(
        reader.poll(&f.request(true)).selected.unwrap().project,
        "win-island-rust"
    );
}

#[test]
fn agent_missing_or_unreadable_project_metadata_keeps_folder_fallback() {
    let f = Fixture::new("project-fallback");
    f.write_in("cli", 100, "F:\\Projects\\cli-project");
    let mut reader = island::agent_sessions::Reader::new(f.root.clone());
    for bytes in [
        None,
        Some("{broken"),
        Some("{}"),
        Some("{\"local-projects\":null}"),
    ] {
        if let Some(bytes) = bytes {
            std::fs::write(f.root.join(".codex-global-state.json"), bytes).unwrap();
        }
        let out = reader.poll(&f.request(true));
        assert!(out.error.is_none());
        assert_eq!(out.selected.unwrap().project, "cli-project");
    }
    f.project_state(json!({"local-projects":{
        "cli":{"name":"命令行项目","rootPaths":["F:\\Projects\\cli-project"]}
    }}));
    assert_eq!(
        reader.poll(&f.request(true)).selected.unwrap().project,
        "命令行项目"
    );
}
