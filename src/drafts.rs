//! Bounded background draft writes. Closing a composer never sends its body to GitHub.
use crate::storage::{Cache, Scope};
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::mpsc::{self, Receiver, SyncSender, TrySendError},
    thread,
};

enum Message {
    Save(Scope, String, String),
    SaveAndWait(Scope, String, String, SyncSender<Result<(), String>>),
    Remove(Scope, String, SyncSender<Result<(), String>>),
    Shutdown,
}
pub struct DraftAck {
    pub scope: Scope,
    pub resource: String,
    pub result: Result<(), String>,
}
#[derive(Clone)]
pub struct DraftClient {
    sender: SyncSender<Message>,
}
impl DraftClient {
    /// Publication workers use the same ordered queue as editing saves, and wait
    /// for durable attempt metadata before sending any remote write.
    pub fn save_and_wait(
        &self,
        scope: Scope,
        resource: String,
        body: String,
    ) -> Result<(), String> {
        let (sender, reply) = mpsc::sync_channel(1);
        self.sender
            .send(Message::SaveAndWait(scope, resource, body, sender))
            .map_err(|_| "Draft writer closed".to_owned())?;
        reply.recv().map_err(|_| "Draft writer closed".to_owned())?
    }
    pub fn save_pending(&self, pending: Vec<(Scope, String, String)>) -> Result<(), String> {
        for (scope, resource, body) in pending {
            self.sender
                .send(Message::Save(scope, resource, body))
                .map_err(|_| "Draft writer closed".to_owned())?;
        }
        Ok(())
    }
    /// Called by the publication worker only after confirmed remote success.
    pub fn remove(&self, scope: Scope, resource: String) -> Result<(), String> {
        let (sender, reply) = mpsc::sync_channel(1);
        self.sender
            .send(Message::Remove(scope, resource, sender))
            .map_err(|_| "Draft writer closed".to_owned())?;
        reply.recv().map_err(|_| "Draft writer closed".to_owned())?
    }
}
pub struct DraftWriter {
    sender: SyncSender<Message>,
    acks: Receiver<DraftAck>,
    worker: Option<thread::JoinHandle<()>>,
    pending: BTreeMap<(String, String, String), String>,
}
impl DraftWriter {
    pub fn new(path: PathBuf) -> Self {
        let (sender, requests) = mpsc::sync_channel(8);
        let (acks_tx, acks) = mpsc::channel();
        let worker = thread::spawn(move || {
            let cache = Cache::open(&path).map_err(|error| error.to_string());
            while let Ok(message) = requests.recv() {
                match message {
                    Message::Save(scope, resource, body) => {
                        let result = cache.as_ref().map_err(Clone::clone).and_then(|cache| {
                            cache
                                .put_draft(&scope, &resource, &body)
                                .map_err(|error| error.to_string())
                        });
                        let _ = acks_tx.send(DraftAck {
                            scope,
                            resource,
                            result,
                        });
                    }
                    Message::Remove(scope, resource, reply) => {
                        let result = cache.as_ref().map_err(Clone::clone).and_then(|cache| {
                            cache
                                .remove_draft(&scope, &resource)
                                .map_err(|error| error.to_string())
                        });
                        let _ = reply.send(result);
                    }
                    Message::SaveAndWait(scope, resource, body, reply) => {
                        let result = cache.as_ref().map_err(Clone::clone).and_then(|cache| {
                            cache
                                .put_draft(&scope, &resource, &body)
                                .map_err(|error| error.to_string())
                        });
                        let _ = reply.send(result);
                    }
                    Message::Shutdown => break,
                }
            }
        });
        Self {
            sender,
            acks,
            worker: Some(worker),
            pending: BTreeMap::new(),
        }
    }
    pub fn client(&self) -> DraftClient {
        DraftClient {
            sender: self.sender.clone(),
        }
    }
    pub fn take_pending(&mut self) -> Vec<(Scope, String, String)> {
        std::mem::take(&mut self.pending)
            .into_iter()
            .map(|((host, login, resource), body)| (Scope { host, login }, resource, body))
            .collect()
    }
    pub fn save(&mut self, scope: Scope, resource: String, body: String) -> Result<(), String> {
        match self
            .sender
            .try_send(Message::Save(scope.clone(), resource.clone(), body))
        {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(Message::Save(_, _, body))) => {
                let key = (scope.host, scope.login, resource);
                if self.pending.len() >= 16 && !self.pending.contains_key(&key) {
                    return Err("Draft queue is full; keep this composer open".into());
                }
                self.pending.insert(key, body);
                Ok(())
            }
            Err(_) => Err("Draft writer is unavailable; keep this composer open".into()),
        }
    }
    pub fn poll(&mut self) -> Vec<DraftAck> {
        let pending = std::mem::take(&mut self.pending);
        for ((host, login, resource), body) in pending {
            let _ = self.save(Scope { host, login }, resource, body);
        }
        self.acks.try_iter().collect()
    }
}
impl Drop for DraftWriter {
    fn drop(&mut self) {
        for ((host, login, resource), body) in std::mem::take(&mut self.pending) {
            let _ = self
                .sender
                .send(Message::Save(Scope { host, login }, resource, body));
        }
        let _ = self.sender.send(Message::Shutdown);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn composer_publication_metadata_waits_for_ordered_durable_save_and_reports_storage_failure() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("private/drafts.sqlite3");
        let scope = Scope::github("github.invalid", Some(42)).unwrap();
        let mut writer = DraftWriter::new(path.clone());
        for body in ["old body-only draft", "latest body-only draft"] {
            writer
                .save(scope.clone(), "exact-pr-action".into(), body.into())
                .unwrap();
        }
        let publication = r#"{"version":2,"values":{"body":"latest"},"publication":{"attempt_id":7,"payload_sha256":"saved-before-any-post"}}"#;
        writer
            .client()
            .save_and_wait(scope.clone(), "exact-pr-action".into(), publication.into())
            .unwrap();
        let cache = Cache::open(&path).unwrap();
        assert_eq!(
            cache.draft(&scope, "exact-pr-action").unwrap().as_deref(),
            Some(publication)
        );
        drop(writer);
        assert_eq!(
            cache.draft(&scope, "exact-pr-action").unwrap().as_deref(),
            Some(publication)
        );

        // Opening a directory as a database must report the failure synchronously.
        let failed = DraftWriter::new(directory.path().to_path_buf());
        assert!(failed
            .client()
            .save_and_wait(scope.clone(), "exact-pr-action".into(), publication.into())
            .is_err());
        assert_eq!(
            cache.draft(&scope, "exact-pr-action").unwrap().as_deref(),
            Some(publication)
        );
        assert!(cache
            .draft(
                &Scope::github("github.invalid", Some(43)).unwrap(),
                "exact-pr-action"
            )
            .unwrap()
            .is_none());
    }
    #[test]
    fn composer_background_drafts_flush_remove_and_account_isolation() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("private/drafts.sqlite3");
        let alice = Scope::new("github.com", "alice").unwrap();
        let bob = Scope::new("github.com", "bob").unwrap();
        let mut writer = DraftWriter::new(path.clone());
        for value in ["old", "new 👩🏽‍💻\nline"] {
            writer
                .save(alice.clone(), "review".into(), value.into())
                .unwrap();
        }
        writer
            .client()
            .remove(bob.clone(), "review".into())
            .unwrap();
        drop(writer);
        let cache = Cache::open(&path).unwrap();
        assert_eq!(
            cache.draft(&alice, "review").unwrap().as_deref(),
            Some("new 👩🏽‍💻\nline")
        );
        assert!(cache.draft(&bob, "review").unwrap().is_none());
        let writer = DraftWriter::new(path.clone());
        writer
            .client()
            .remove(alice.clone(), "review".into())
            .unwrap();
        drop(writer);
        assert!(cache.draft(&alice, "review").unwrap().is_none());
    }
}
