use super::{dsh, Reader, Request, Source, Update};
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sources {
    pub codex: bool,
    pub dsh: Option<PathBuf>,
}
impl Default for Sources {
    fn default() -> Self {
        Self {
            codex: true,
            dsh: None,
        }
    }
}
pub fn merge_updates(codex: Update, dsh: Update, request: &Request) -> Update {
    let wanted = request.pinned.as_ref().or(request.hold.as_ref());
    let mut recent = codex
        .recent
        .iter()
        .chain(&dsh.recent)
        .cloned()
        .collect::<Vec<_>>();
    recent.sort_by_key(|s| std::cmp::Reverse(s.activity_ms));
    recent.dedup_by(|a, b| a.source == b.source && a.id == b.id);
    recent.truncate(32);
    let latest = recent.first().map(|s| s.path.clone());
    let selected = if let Some(path) = wanted {
        recent
            .iter()
            .chain(codex.selected.iter())
            .chain(dsh.selected.iter())
            .find(|s| &s.path == path)
            .cloned()
    } else {
        recent.first().cloned()
    };
    let ready = codex.ready || dsh.ready;
    let error = if let Some(path) = wanted {
        if dsh::session_id(path).is_some() {
            dsh.error.clone()
        } else {
            codex.error.clone()
        }
    } else if selected.is_none() && !ready {
        codex.error.clone().or(dsh.error.clone())
    } else {
        None
    };
    Update {
        recent,
        latest,
        selected,
        error,
        dsh_status: dsh.dsh_status,
        ready,
    }
}
pub(super) struct ReaderSet {
    codex: Reader,
    dsh: Option<dsh::Client>,
    sources: Sources,
}
impl ReaderSet {
    pub fn new(root: PathBuf, sources: Sources) -> Self {
        Self {
            codex: Reader::new(root),
            dsh: sources.dsh.clone().map(dsh::Client::new),
            sources,
        }
    }
    pub fn observe(&mut self, path: PathBuf) {
        self.codex.observe(path);
    }
    pub fn poll(&mut self, request: &Request) -> Update {
        let mut meta = request.clone();
        meta.full = request.full
            && request
                .pinned
                .as_ref()
                .or(request.hold.as_ref())
                .is_some_and(|p| dsh::session_id(p).is_none());
        if meta
            .pinned
            .as_ref()
            .is_some_and(|p| dsh::session_id(p).is_some())
        {
            meta.pinned = None;
        }
        if meta
            .hold
            .as_ref()
            .is_some_and(|p| dsh::session_id(p).is_some())
        {
            meta.hold = None;
        }
        let codex = if self.sources.codex {
            self.codex.poll(&meta)
        } else {
            Update::default()
        };
        let wanted = request
            .pinned
            .as_deref()
            .or(request.hold.as_deref())
            .filter(|p| dsh::session_id(p).is_some());
        let dsh = if let Some(client) = &self.dsh {
            match client.poll(false, wanted) {
                Ok(update) => update,
                Err(error) => Update {
                    dsh_status: error.clone(),
                    error: Some(error),
                    ..Default::default()
                },
            }
        } else {
            Update::default()
        };
        let mut combined = merge_updates(codex, dsh, request);
        if request.full {
            if let Some(snapshot) = &combined.selected {
                let path = snapshot.path.clone();
                match snapshot.source {
                    Source::Codex => {
                        let full = Request {
                            full: true,
                            pinned: Some(path),
                            hold: None,
                            ..request.clone()
                        };
                        let body = self.codex.poll(&full);
                        combined.selected = body.selected;
                        combined.error = body.error;
                    }
                    Source::Dsh => {
                        if let Some(client) = &self.dsh {
                            match client.poll(true, Some(&path)) {
                                Ok(body) => {
                                    combined.selected = body.selected;
                                    combined.error = body.error;
                                }
                                Err(error) => {
                                    combined.selected = None;
                                    combined.error = Some(error);
                                }
                            }
                        }
                    }
                }
            }
        }
        combined
    }
}
