//! Реестр подключённых оверлеев и очередь недоставленных сообщений.
//!
//! Идентичность оверлея — его `path` из URL (`/overlay/<path>`); никаких
//! `register`-сообщений от клиента.

use parking_lot::Mutex;
use serde::Serialize;
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{broadcast, mpsc};

const PENDING_TTL: Duration = Duration::from_secs(30);
const PENDING_MAX: usize = 20;

#[derive(Debug, Clone, Serialize, ts_rs::TS, PartialEq)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "api.ts")]
pub struct OverlayConnection {
    pub path: String,
    pub remote: String,
    /// Unix-время подключения, мс.
    #[ts(type = "number")]
    pub since: i64,
}

struct Client {
    info: OverlayConnection,
    tx: mpsc::Sender<String>,
}

#[derive(Default)]
struct Inner {
    clients: HashMap<u64, Client>,
    pending: HashMap<String, VecDeque<(Instant, String)>>,
}

#[derive(Clone)]
pub struct OverlayHub {
    inner: Arc<Mutex<Inner>>,
    next_id: Arc<AtomicU64>,
    changed: broadcast::Sender<()>,
}

impl Default for OverlayHub {
    fn default() -> Self {
        Self::new()
    }
}

impl OverlayHub {
    pub fn new() -> Self {
        let (changed, _) = broadcast::channel(32);
        Self { inner: Arc::new(Mutex::new(Inner::default())), next_id: Arc::new(AtomicU64::new(1)), changed }
    }

    /// Уведомления «состав подключений изменился».
    pub fn subscribe_changes(&self) -> broadcast::Receiver<()> {
        self.changed.subscribe()
    }

    /// Зарегистрировать соединение; возвращает id и приёмник исходящих сообщений.
    /// Недоставленные ранее сообщения для этого path сразу кладутся в канал.
    pub fn connect(&self, path: &str, remote: String) -> (u64, mpsc::Receiver<String>) {
        let (tx, rx) = mpsc::channel(64);
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let info = OverlayConnection { path: path.to_string(), remote, since: chrono::Utc::now().timestamp_millis() };
        let mut flushed = 0;
        {
            let mut inner = self.inner.lock();
            if let Some(q) = inner.pending.remove(path) {
                let now = Instant::now();
                for (t, m) in q {
                    if now.duration_since(t) <= PENDING_TTL && tx.try_send(m).is_ok() {
                        flushed += 1;
                    }
                }
            }
            inner.clients.insert(id, Client { info, tx });
        }
        tracing::info!(target: "signorebot::overlay", "Оверлей «{path}» подключился{}",
            if flushed > 0 { format!(", доставлено отложенных: {flushed}") } else { String::new() });
        let _ = self.changed.send(());
        (id, rx)
    }

    pub fn disconnect(&self, id: u64) {
        let removed = self.inner.lock().clients.remove(&id);
        if let Some(c) = removed {
            tracing::info!(target: "signorebot::overlay", "Оверлей «{}» отключился", c.info.path);
            let _ = self.changed.send(());
        }
    }

    pub fn connections(&self) -> Vec<OverlayConnection> {
        let mut v: Vec<_> = self.inner.lock().clients.values().map(|c| c.info.clone()).collect();
        v.sort_by(|a, b| a.path.cmp(&b.path).then(a.since.cmp(&b.since)));
        v
    }

    pub fn is_connected(&self, path: &str) -> bool {
        self.inner.lock().clients.values().any(|c| c.info.path == path)
    }

    /// Отправить всем оверлеям с данным path. Если никого нет — в очередь.
    /// Возвращает `true`, если доставлено хотя бы одному.
    pub fn send_to_path(&self, path: &str, message: &str) -> bool {
        self.send_to_path_opt(path, message, true)
    }

    /// `queue = false` — не ждать в отложенной очереди, если оверлей не подключён.
    pub fn send_to_path_opt(&self, path: &str, message: &str, queue: bool) -> bool {
        let mut inner = self.inner.lock();
        let mut sent = false;
        for c in inner.clients.values() {
            if c.info.path == path && c.tx.try_send(message.to_string()).is_ok() {
                sent = true;
            }
        }
        if !sent && queue {
            let q = inner.pending.entry(path.to_string()).or_default();
            let now = Instant::now();
            q.retain(|(t, _)| now.duration_since(*t) <= PENDING_TTL);
            q.push_back((now, message.to_string()));
            while q.len() > PENDING_MAX {
                q.pop_front();
            }
        }
        sent
    }

    /// Отправить всем подключённым оверлеям (без очереди).
    pub fn broadcast(&self, message: &str) -> usize {
        let inner = self.inner.lock();
        inner.clients.values().filter(|c| c.tx.try_send(message.to_string()).is_ok()).count()
    }

    pub fn pending_count(&self, path: &str) -> usize {
        self.inner.lock().pending.get(path).map(|q| q.len()).unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn queue_then_flush() {
        let hub = OverlayHub::new();
        assert!(!hub.send_to_path("audio", "m1"));
        assert_eq!(hub.pending_count("audio"), 1);
        let (id, mut rx) = hub.connect("audio", "127.0.0.1".into());
        assert_eq!(rx.recv().await.unwrap(), "m1");
        assert!(hub.send_to_path("audio", "m2"));
        assert_eq!(rx.recv().await.unwrap(), "m2");
        assert!(!hub.send_to_path("video", "m3"));
        assert_eq!(hub.connections().len(), 1);
        hub.disconnect(id);
        assert!(hub.connections().is_empty());
    }
}
