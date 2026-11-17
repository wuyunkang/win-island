//! 本机只读诊断：不输出会话正文或凭据。
use island::agent_sessions::{Config, Reader, Request};
fn main() {
    if std::env::args().nth(1).as_deref() == Some("--dsh") {
        let path = Config::default().sources().dsh.unwrap();
        let client = island::agent_sessions::dsh::Client::new(path);
        for full in [false, true, false] {
            let at = std::time::Instant::now();
            match client.poll(full, None) {
                Ok(out) => {
                    let s = out.selected.as_ref();
                    println!(
                        "{}",
                        serde_json::json!({"source":"dsh","full":full,"recent":out.recent.len(),"read_receipts":out.read_receipts.len(),"state":s.map(|s|s.state.label()),"model":s.map(|s|s.model.as_str()),"messages":s.map(|s|s.messages.len()),"body_bytes":s.map(|s|s.messages.iter().map(|m|m.text.len()).sum::<usize>()),"elapsed_ms":at.elapsed().as_millis(),"desktop_available":island::agent_sessions::desktop_available_for(island::agent_sessions::Source::Dsh)})
                    );
                }
                Err(error) => {
                    eprintln!("{error}");
                    std::process::exit(1);
                }
            }
        }
        return;
    }
    let root = std::env::args_os()
        .nth(1)
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| Config::default().root());
    let mut reader = Reader::new(root.clone());
    for full in [false, true, false] {
        let started = std::time::Instant::now();
        let out = reader.poll(&Request {
            root: root.clone(),
            full,
            pinned: None,
            hold: None,
        });
        let selected = out.selected.as_ref();
        let usage = island::resources::snapshot().unwrap_or_default();
        println!(
            "{}",
            serde_json::json!({"full":full,"recent":out.recent.len(),"read_receipts":out.read_receipts.len(),"project":selected.map(|s|s.project.as_str()),"model":selected.map(|s|s.model.as_str()),"state":selected.map(|s|s.state.label()),"messages":selected.map(|s|s.messages.len()),"body_bytes":reader.retained_body_bytes(),"elapsed_ms":started.elapsed().as_millis(),"private_mib":usage.private_bytes/1048576,"error":out.error})
        );
    }
}
