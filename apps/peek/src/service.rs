//! A resident Git actor. Application declarations only read snapshots/send values.
use crate::git::{Error, Mode, Repository, Snapshot};
use plico_dev::services::{HostService, Wake};
use serde::{Deserialize, Serialize};
use std::{
    path::Path,
    sync::{Arc, Mutex, mpsc},
    thread,
    time::{Duration, Instant},
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum GitCommand {
    Open {
        path: String,
    },
    Browse {
        revision: String,
        mode: Mode,
        file: String,
    },
    Refresh,
}
struct Job {
    generation: u64,
    command: GitCommand,
}
struct Update {
    generation: u64,
    snapshot: Snapshot,
}

pub struct GitService {
    snapshot: Snapshot,
    generation: u64,
    revision: u64,
    jobs: mpsc::Sender<Job>,
    updates: mpsc::Receiver<Update>,
    wake: Arc<Mutex<Option<Wake>>>,
    avatars: crate::avatars::Avatars,
}
impl GitService {
    pub fn new(path: &Path) -> Self {
        let (jobs, requests) = mpsc::channel::<Job>();
        let (results, updates) = mpsc::channel();
        let wake: Arc<Mutex<Option<Wake>>> = Arc::default();
        let notify = wake.clone();
        thread::spawn(move || {
            let mut state = Snapshot::default();
            while let Ok(job) = requests.recv() {
                apply(&mut state, job.command);
                if results
                    .send(Update {
                        generation: job.generation,
                        snapshot: state.clone(),
                    })
                    .is_err()
                {
                    break;
                }
                let callback = notify.lock().unwrap().clone();
                if let Some(wake) = callback {
                    wake();
                }
            }
        });
        let mut service = Self {
            snapshot: Snapshot::default(),
            generation: 0,
            revision: 0,
            jobs,
            updates,
            avatars: crate::avatars::Avatars::new(wake.clone()),
            wake,
        };
        service.send(GitCommand::Open {
            path: path.to_string_lossy().into_owned(),
        });
        service
    }
    fn accept(&mut self, update: Update) -> bool {
        if update.generation != self.generation {
            return false;
        }
        let images = self.snapshot.avatars.clone();
        self.snapshot = update.snapshot;
        self.snapshot.avatars = images;
        for commit in self.snapshot.commits.iter() {
            self.avatars.request(&commit.email);
        }
        self.revision += 1;
        true
    }
    /// Capture-only bounded wait; interactive windows never wait for network IO.
    pub fn wait_avatar(&mut self, email: &str, timeout: Duration) {
        let target = crate::avatars::normalize(email);
        let deadline = Instant::now() + timeout;
        while !self.snapshot.avatars.contains_key(&target) {
            let Ok((email, asset)) = self
                .avatars
                .results
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            else {
                break;
            };
            if let Some(asset) = asset {
                Arc::make_mut(&mut self.snapshot.avatars).insert(email.clone(), asset);
                self.revision += 1;
            }
            if email == target {
                break;
            }
        }
    }
    /// CLI capture/testing only: wait for the worker without sleeps or a UI loop.
    pub fn wait_ready(&mut self, timeout: Duration) -> Result<(), String> {
        let deadline = Instant::now() + timeout;
        while self.snapshot.loading {
            let remaining = deadline.saturating_duration_since(Instant::now());
            let update = self
                .updates
                .recv_timeout(remaining)
                .map_err(|e| e.to_string())?;
            self.accept(update);
        }
        self.snapshot.error.clone().map_or(Ok(()), Err)
    }
}
impl HostService for GitService {
    type Snapshot = Snapshot;
    type Command = GitCommand;
    fn snapshot(&self) -> Snapshot {
        self.snapshot.clone()
    }
    fn snapshot_revision(&self) -> Option<u64> {
        Some(self.revision)
    }
    fn send(&mut self, command: GitCommand) {
        self.generation += 1;
        self.revision += 1;
        self.snapshot.loading = true;
        if self
            .jobs
            .send(Job {
                generation: self.generation,
                command,
            })
            .is_err()
        {
            self.snapshot.loading = false;
            self.snapshot.error = Some("Git worker stopped".into());
        }
    }
    fn poll(&mut self) -> bool {
        let updates: Vec<_> = self.updates.try_iter().collect();
        let mut changed = false;
        for update in updates {
            changed |= self.accept(update);
        }
        while let Ok((email, asset)) = self.avatars.results.try_recv() {
            if let Some(asset) = asset {
                Arc::make_mut(&mut self.snapshot.avatars).insert(email, asset);
                self.revision += 1;
                changed = true;
            }
        }
        changed
    }
    fn set_wake_callback(&mut self, wake: Wake) {
        *self.wake.lock().unwrap() = Some(wake);
    }
}
fn apply(state: &mut Snapshot, command: GitCommand) {
    let result = (|| -> Result<(), Error> {
        match command {
            GitCommand::Open { path } => {
                let repo = Repository::open(Path::new(&path))?;
                let mut next = repo.history()?;
                repo.browse(&mut next, "", Mode::Changes, "")?;
                *state = next;
            }
            GitCommand::Browse {
                revision,
                mode,
                file,
            } => {
                Repository::open(Path::new(&state.repo))?.browse(state, &revision, mode, &file)?;
            }
            GitCommand::Refresh => {
                let repo = Repository::open(Path::new(&state.repo))?;
                let mut next = repo.history()?;
                let revision = if next.commits.iter().any(|c| c.hash == state.revision) {
                    state.revision.clone()
                } else {
                    String::new()
                };
                repo.browse(&mut next, &revision, state.mode, &state.opened_file)?;
                *state = next;
            }
        }
        Ok(())
    })();
    state.loading = false;
    state.error = result.err().map(|error| error.to_string());
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stale_completion_cannot_replace_newer_selection() {
        let (jobs, _) = mpsc::channel();
        let (_, updates) = mpsc::channel();
        let mut host = GitService {
            snapshot: Snapshot {
                name: "current".into(),
                ..Default::default()
            },
            generation: 2,
            revision: 0,
            jobs,
            updates,
            avatars: crate::avatars::Avatars::new(Arc::default()),
            wake: Arc::default(),
        };
        assert!(!host.accept(Update {
            generation: 1,
            snapshot: Snapshot {
                name: "old".into(),
                ..Default::default()
            }
        }));
        assert_eq!(host.snapshot.name, "current");
        assert!(host.accept(Update {
            generation: 2,
            snapshot: Snapshot {
                name: "new".into(),
                ..Default::default()
            }
        }));
        assert_eq!(host.snapshot.name, "new");
    }
}
