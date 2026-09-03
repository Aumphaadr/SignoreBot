//! Очередь shoutout: авто по первому сообщению, по рейдам, вручную.

use crate::config::RaidShoutoutMode;
use parking_lot::Mutex;
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Notify;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "api.ts")]
pub enum ShoutoutSource {
    Message,
    Raid,
    Manual,
}

impl ShoutoutSource {
    pub fn label(self) -> &'static str {
        match self {
            ShoutoutSource::Message => "сообщение",
            ShoutoutSource::Raid => "рейд",
            ShoutoutSource::Manual => "ручной запуск",
        }
    }
}

#[derive(Debug, Clone, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "api.ts")]
pub struct QueueItem {
    #[ts(type = "number")]
    pub id: u64,
    pub username: String,
    pub source: ShoutoutSource,
    pub attempts: u32,
}

#[derive(Debug, Clone, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "api.ts")]
pub struct DoneItem {
    pub username: String,
    pub sources: Vec<ShoutoutSource>,
}

#[derive(Debug, Clone, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "api.ts")]
pub struct ShoutoutStatus {
    pub queue: Vec<QueueItem>,
    pub done: Vec<DoneItem>,
    /// Осталось до конца кулдауна, мс.
    #[ts(type = "number")]
    pub cooldown_remaining_ms: u64,
    pub processing: bool,
    #[ts(type = "number")]
    pub current_id: Option<u64>,
}

#[derive(Default)]
struct State {
    queue: Vec<QueueItem>,
    done: HashMap<String, HashSet<ShoutoutSource>>,
    next_id: u64,
    cooldown_until: Option<Instant>,
    processing: bool,
    current: Option<u64>,
}

/// Результат отправки от исполнителя (Helix).
pub enum SendOutcome {
    Ok,
    /// Повторить через указанное время.
    Retry(Duration),
    /// Не повторять.
    Fail,
    /// Twitch уже считает шатаут сделанным (например, модератор сделал его
    /// вручную) — снимаем из очереди как выполненный, без кулдауна.
    AlreadyDone,
}

#[derive(Clone)]
pub struct ShoutoutQueue {
    state: Arc<Mutex<State>>,
    notify: Arc<Notify>,
}

impl Default for ShoutoutQueue {
    fn default() -> Self {
        Self::new()
    }
}

impl ShoutoutQueue {
    pub fn new() -> Self {
        Self { state: Arc::new(Mutex::new(State { next_id: 1, ..Default::default() })), notify: Arc::new(Notify::new()) }
    }

    pub fn notify_handle(&self) -> Arc<Notify> {
        Arc::clone(&self.notify)
    }

    pub fn status(&self) -> ShoutoutStatus {
        let s = self.state.lock();
        let now = Instant::now();
        let mut done: Vec<DoneItem> = s
            .done
            .iter()
            .map(|(u, src)| {
                let mut v: Vec<_> = src.iter().copied().collect();
                v.sort_by_key(|x| *x as u8);
                DoneItem { username: u.clone(), sources: v }
            })
            .collect();
        done.sort_by(|a, b| a.username.cmp(&b.username));
        ShoutoutStatus {
            queue: s.queue.clone(),
            done,
            cooldown_remaining_ms: s.cooldown_until.map(|t| t.saturating_duration_since(now).as_millis() as u64).unwrap_or(0),
            processing: s.processing,
            current_id: s.current,
        }
    }

    fn norm(u: &str) -> String {
        u.trim().trim_start_matches('@').to_lowercase()
    }

    fn push(&self, username: &str, source: ShoutoutSource) -> bool {
        let mut s = self.state.lock();
        let id = s.next_id;
        s.next_id += 1;
        s.queue.push(QueueItem { id, username: username.trim().trim_start_matches('@').to_string(), source, attempts: 0 });
        let pos = s.queue.len();
        drop(s);
        tracing::info!(target: "signorebot::shoutout", "{username} добавлен в очередь шатаутов ({}, позиция {pos})", source.label());
        self.notify.notify_one();
        true
    }

    /// Авто-shoutout по сообщению: только из списка и только раз за сессию.
    pub fn enqueue_message(&self, username: &str, auto_list: &[String]) -> bool {
        let lower = Self::norm(username);
        if lower.is_empty() || !auto_list.contains(&lower) {
            return false;
        }
        {
            let s = self.state.lock();
            if s.done.contains_key(&lower) {
                return false;
            }
            if s.queue.iter().any(|q| Self::norm(&q.username) == lower && matches!(q.source, ShoutoutSource::Message | ShoutoutSource::Raid)) {
                return false;
            }
        }
        self.push(username, ShoutoutSource::Message)
    }

    pub fn enqueue_raid(&self, username: &str, mode: RaidShoutoutMode, auto_list: &[String]) -> bool {
        let lower = Self::norm(username);
        if lower.is_empty() {
            return false;
        }
        let listed = auto_list.contains(&lower);
        let allowed = match mode {
            RaidShoutoutMode::None => false,
            RaidShoutoutMode::Listed => listed,
            RaidShoutoutMode::Unlisted => !listed,
            RaidShoutoutMode::All => true,
        };
        if !allowed {
            if mode != RaidShoutoutMode::None {
                tracing::info!(target: "signorebot::shoutout", "Шатаут рейдеру {username} пропущен (режим {mode:?})");
            }
            return false;
        }
        {
            let s = self.state.lock();
            if s.done.get(&lower).map(|d| d.contains(&ShoutoutSource::Raid)).unwrap_or(false) {
                return false;
            }
            if s.queue.iter().any(|q| Self::norm(&q.username) == lower && q.source == ShoutoutSource::Raid) {
                return false;
            }
        }
        self.push(username, ShoutoutSource::Raid)
    }

    /// Ручной: отказ, если уже в очереди (любой источник).
    pub fn enqueue_manual(&self, username: &str) -> Result<(), String> {
        let lower = Self::norm(username);
        if lower.is_empty() {
            return Err("Укажите имя пользователя".into());
        }
        if self.state.lock().queue.iter().any(|q| Self::norm(&q.username) == lower) {
            return Err(format!("{username} уже находится в очереди шатаутов"));
        }
        self.push(username, ShoutoutSource::Manual);
        Ok(())
    }

    pub fn remove(&self, id: u64) -> Result<QueueItem, String> {
        let mut s = self.state.lock();
        if s.current == Some(id) {
            return Err("Этот shoutout уже отправляется в Twitch, его нельзя отменить".into());
        }
        let Some(idx) = s.queue.iter().position(|q| q.id == id) else {
            return Err("Запись в очереди не найдена".into());
        };
        let item = s.queue.remove(idx);
        tracing::info!(target: "signorebot::shoutout", "{} ({}) удалён из очереди", item.username, item.source.label());
        Ok(item)
    }

    pub fn reset_done(&self) {
        self.state.lock().done.clear();
        tracing::info!(target: "signorebot::shoutout", "Список выполненных шатаутов сброшен");
    }

    /// Вернуть следующий элемент и время ожидания до него (из-за кулдауна).
    pub fn next_ready(&self) -> Option<(QueueItem, Duration)> {
        let mut s = self.state.lock();
        let item = s.queue.first()?.clone();
        let wait = s.cooldown_until.map(|t| t.saturating_duration_since(Instant::now())).unwrap_or(Duration::ZERO);
        s.processing = true;
        Some((item, wait))
    }

    pub fn begin(&self, id: u64) -> bool {
        let mut s = self.state.lock();
        if s.queue.first().map(|q| q.id == id).unwrap_or(false) {
            s.current = Some(id);
            true
        } else {
            false
        }
    }

    pub fn finish(&self, id: u64, outcome: SendOutcome, cooldown: Duration) {
        let mut s = self.state.lock();
        s.current = None;
        let Some(idx) = s.queue.iter().position(|q| q.id == id) else { return };
        match outcome {
            SendOutcome::Ok => {
                let item = s.queue.remove(idx);
                let key = Self::norm(&item.username);
                if matches!(item.source, ShoutoutSource::Message | ShoutoutSource::Raid) {
                    s.done.entry(key).or_default().insert(item.source);
                }
                s.cooldown_until = Some(Instant::now() + cooldown);
                tracing::info!(target: "signorebot::shoutout", "Шатаут для {} выполнен. В очереди: {}", item.username, s.queue.len());
            }
            SendOutcome::Retry(after) => {
                let item = &mut s.queue[idx];
                item.attempts += 1;
                if item.attempts >= 3 {
                    let item = s.queue.remove(idx);
                    tracing::warn!(target: "signorebot::shoutout", "Шатаут для {} удалён из очереди после 3 неудач", item.username);
                } else {
                    tracing::warn!(target: "signorebot::shoutout", "Шатаут для {} не удался, повтор через {} с", item.username, after.as_secs());
                    s.cooldown_until = Some(Instant::now() + after);
                }
            }
            SendOutcome::Fail => {
                let item = s.queue.remove(idx);
                tracing::warn!(target: "signorebot::shoutout", "Шатаут для {} отменён (неустранимая ошибка)", item.username);
            }
            SendOutcome::AlreadyDone => {
                let item = s.queue.remove(idx);
                let key = Self::norm(&item.username);
                if matches!(item.source, ShoutoutSource::Message | ShoutoutSource::Raid) {
                    s.done.entry(key).or_default().insert(item.source);
                }
                tracing::info!(target: "signorebot::shoutout", "Шатаут для {} уже сделан (вручную или ранее) — снят из очереди", item.username);
            }
        }
        s.processing = !s.queue.is_empty();
    }

    pub fn set_idle(&self) {
        let mut s = self.state.lock();
        s.processing = false;
        s.current = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message_rules() {
        let q = ShoutoutQueue::new();
        let list = vec!["alice".to_string()];
        assert!(!q.enqueue_message("Bob", &list));
        assert!(q.enqueue_message("Alice", &list));
        assert!(!q.enqueue_message("alice", &list)); // уже в очереди
        let (item, wait) = q.next_ready().unwrap();
        assert_eq!(wait, Duration::ZERO);
        assert!(q.begin(item.id));
        q.finish(item.id, SendOutcome::Ok, Duration::from_secs(125));
        assert!(!q.enqueue_message("Alice", &list)); // уже выполнено
        assert!(q.status().cooldown_remaining_ms > 100_000);
        assert_eq!(q.status().done[0].sources, vec![ShoutoutSource::Message]);
        // рейд после сообщения — разрешён
        assert!(q.enqueue_raid("alice", RaidShoutoutMode::All, &list));
        // а сообщение после рейда — нет
        q.reset_done();
        assert!(!q.enqueue_message("alice", &list)); // в очереди как raid
    }

    #[test]
    fn raid_modes_and_manual() {
        let q = ShoutoutQueue::new();
        let list = vec!["alice".to_string()];
        assert!(!q.enqueue_raid("alice", RaidShoutoutMode::None, &list));
        assert!(!q.enqueue_raid("bob", RaidShoutoutMode::Listed, &list));
        assert!(q.enqueue_raid("bob", RaidShoutoutMode::Unlisted, &list));
        assert!(q.enqueue_manual("bob").is_err());
        assert!(q.enqueue_manual("carol").is_ok());
        let id = q.status().queue[1].id;
        assert!(q.remove(id).is_ok());
        assert!(q.remove(999).is_err());
        let (item, _) = q.next_ready().unwrap();
        q.begin(item.id);
        assert!(q.remove(item.id).is_err());
        q.finish(item.id, SendOutcome::Retry(Duration::from_secs(30)), Duration::ZERO);
        assert_eq!(q.status().queue[0].attempts, 1);
        q.finish(item.id, SendOutcome::Fail, Duration::ZERO);
        assert!(q.status().queue.is_empty());
    }

    #[test]
    fn manual_by_moderator_does_not_block_queue() {
        // Улов владельца: пользователь из auto-списка стоял в очереди, модератор
        // отшатаутил его вручную → Twitch отвечал «уже в течение часа», старый
        // бот крутил ретраи и очередь вставала до конца стрима.
        let q = ShoutoutQueue::new();
        let list = vec!["alice".to_string(), "bob".to_string()];
        assert!(q.enqueue_message("alice", &list));
        assert!(q.enqueue_message("bob", &list));
        let (a, _) = q.next_ready().unwrap();
        q.begin(a.id);
        q.finish(a.id, SendOutcome::AlreadyDone, Duration::from_secs(125));
        // alice считается выполненной, кулдауна нет, bob — следующий сразу
        assert!(q.status().done.iter().any(|d| d.username == "alice"));
        assert_eq!(q.status().cooldown_remaining_ms, 0);
        let (b, wait) = q.next_ready().unwrap();
        assert_eq!(b.username, "bob");
        assert_eq!(wait, Duration::ZERO);
        assert!(!q.enqueue_message("alice", &list));
    }
}
