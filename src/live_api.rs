//! 本机活动接口的桌面生命周期和临时访问凭据。
use island_events::{Server, Shared};
use ring::rand::{SecureRandom, SystemRandom};
use serde::{Deserialize, Serialize};
use std::{io, path::PathBuf, sync::Arc};

#[derive(Serialize, Deserialize)]
pub struct Discovery {
    pub endpoint: String,
    pub token: String,
}

pub struct Service {
    _server: Server,
    path: PathBuf,
    token: String,
}
impl Service {
    pub fn start(port: u16, store: Shared, ctx: eframe::egui::Context) -> io::Result<Self> {
        let mut random = [0u8; 32];
        SystemRandom::new()
            .fill(&mut random)
            .map_err(|_| io::Error::other("无法生成接口访问令牌"))?;
        let token = random
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        let server = Server::bind(
            ([127, 0, 0, 1], port).into(),
            token.clone(),
            store,
            Arc::new(move || ctx.request_repaint()),
        )?;
        let path = crate::model::config_dir().join("live-api.json");
        std::fs::create_dir_all(path.parent().unwrap())?;
        let discovery = Discovery {
            endpoint: format!("http://{}", server.address()),
            token: token.clone(),
        };
        let temporary = path.with_extension("json.tmp");
        std::fs::write(&temporary, serde_json::to_vec_pretty(&discovery)?)?;
        if let Err(error) = std::fs::rename(&temporary, &path) {
            let _ = std::fs::remove_file(&temporary);
            return Err(error);
        }
        Ok(Self {
            _server: server,
            path,
            token,
        })
    }
}
impl Drop for Service {
    fn drop(&mut self) {
        let ours = std::fs::read(&self.path)
            .ok()
            .and_then(|data| serde_json::from_slice::<Discovery>(&data).ok())
            .is_some_and(|discovery| discovery.token == self.token);
        if ours {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}
