//! 只读解析 Codex 桌面项目名称，不保存或修改桌面配置。
use super::clip;
use serde::Deserialize;
use std::{
    collections::{BTreeMap, HashMap},
    fs::{self, File},
    io::Read,
    path::Path,
    time::SystemTime,
};

const MAX_STATE: u64 = 4 * 1024 * 1024;

#[derive(Default, Deserialize)]
#[serde(default)]
struct ProjectState {
    #[serde(rename = "local-projects")]
    projects: BTreeMap<String, Project>,
    #[serde(rename = "thread-project-assignments")]
    assignments: HashMap<String, Assignment>,
}

#[derive(Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct Project {
    name: String,
    root_paths: Vec<String>,
    updated_at: u64,
}

#[derive(Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct Assignment {
    project_kind: String,
    project_id: String,
}

#[derive(Default)]
pub(super) struct ProjectNames {
    stamp: Option<(u64, SystemTime)>,
    state: ProjectState,
}

impl ProjectNames {
    pub fn refresh(&mut self, root: &Path) {
        let path = root.join(".codex-global-state.json");
        let Ok(meta) = fs::metadata(&path) else {
            self.state = ProjectState::default();
            self.stamp = None;
            return;
        };
        let stamp = meta.modified().ok().map(|modified| (meta.len(), modified));
        if stamp.is_some() && self.stamp == stamp {
            return;
        }
        if meta.len() > MAX_STATE {
            return;
        }
        let Ok(file) = File::open(path) else { return };
        let mut bytes = Vec::new();
        if file.take(MAX_STATE + 1).read_to_end(&mut bytes).is_err()
            || bytes.len() as u64 > MAX_STATE
        {
            return;
        }
        // 仅保留项目与归属字段；写入未完成时保留上次成功结果，下轮重试。
        let Ok(mut state) = serde_json::from_slice::<ProjectState>(&bytes) else {
            return;
        };
        state.projects.retain(|_, project| {
            project.name = clip(project.name.trim(), 240);
            project.root_paths = project
                .root_paths
                .iter()
                .map(|path| path_key(path))
                .filter(|path| !path.is_empty())
                .collect();
            !project.name.is_empty()
        });
        self.state = state;
        self.stamp = stamp;
    }

    pub fn name(&self, id: &str, cwd: &str) -> Option<&str> {
        // 明确的会话归属优先，兼容同目录多个项目和项目的独立工作树。
        if let Some(project) = self
            .state
            .assignments
            .get(id)
            .filter(|assignment| assignment.project_kind == "local")
            .and_then(|assignment| self.state.projects.get(&assignment.project_id))
        {
            return Some(&project.name);
        }
        let cwd = path_key(cwd);
        self.state
            .projects
            .values()
            .flat_map(|project| project.root_paths.iter().map(move |root| (root, project)))
            .filter(|(root, _)| {
                cwd == **root
                    || cwd
                        .strip_prefix(root.as_str())
                        .is_some_and(|tail| root.ends_with('/') || tail.starts_with('/'))
            })
            .max_by_key(|(root, project)| (root.len(), project.updated_at))
            .map(|(_, project)| project.name.as_str())
    }
}

fn path_key(path: &str) -> String {
    let mut path = path.replace('\\', "/");
    if path
        .get(..8)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("//?/UNC/"))
    {
        path = format!("//{}", &path[8..]);
    } else if let Some(short) = path.strip_prefix("//?/") {
        path = short.to_owned();
    }
    // Windows 盘符和 UNC 路径忽略 ASCII 大小写；POSIX 路径保留大小写。
    if path.as_bytes().get(1) == Some(&b':') || path.starts_with("//") {
        path.make_ascii_lowercase();
    }
    if path != "/" {
        path.truncate(path.trim_end_matches('/').len());
    }
    path
}
