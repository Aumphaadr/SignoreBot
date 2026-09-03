//! Логирование: `tracing` → файл на запуск + кольцевой буфер + рассылка в UI.
//!
//! Уровни: `info` — обычная работа, `warn` — требует внимания, `error` —
//! сбой. Категория (`target`) — модуль ядра (`twitch`, `eventsub`, `overlay`,
//! `engine`, `obs`, `auth`, …); UI фильтрует по ней, а не по эмодзи.

use crate::paths::AppPaths;
use parking_lot::Mutex;
use serde::Serialize;
use std::collections::VecDeque;
use std::io::Write;
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::field::{Field, Visit};
use tracing::Subscriber;
use tracing_subscriber::layer::{Context, Layer};
use tracing_subscriber::prelude::*;
use tracing_subscriber::registry::LookupSpan;
use ts_rs::TS;

pub const RING_CAPACITY: usize = 2000;

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "api.ts")]
pub struct LogEntry {
    /// Unix-время, мс.
    #[ts(type = "number")]
    pub ts: i64,
    /// `info` | `warn` | `error` | `debug`
    pub level: String,
    /// Категория (модуль).
    pub target: String,
    pub message: String,
}

#[derive(Clone)]
pub struct LogHub {
    ring: Arc<Mutex<VecDeque<LogEntry>>>,
    tx: broadcast::Sender<LogEntry>,
}

impl LogHub {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(512);
        Self { ring: Arc::new(Mutex::new(VecDeque::with_capacity(RING_CAPACITY))), tx }
    }
    pub fn subscribe(&self) -> broadcast::Receiver<LogEntry> {
        self.tx.subscribe()
    }
    pub fn history(&self) -> Vec<LogEntry> {
        self.ring.lock().iter().cloned().collect()
    }
    fn push(&self, e: LogEntry) {
        {
            let mut r = self.ring.lock();
            if r.len() >= RING_CAPACITY {
                r.pop_front();
            }
            r.push_back(e.clone());
        }
        let _ = self.tx.send(e);
    }
}

impl Default for LogHub {
    fn default() -> Self {
        Self::new()
    }
}

struct MessageVisitor(String);
impl Visit for MessageVisitor {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.0 = format!("{value:?}");
        }
    }
    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "message" {
            self.0 = value.to_string();
        }
    }
}

struct HubLayer {
    hub: LogHub,
    file: Option<Arc<Mutex<std::fs::File>>>,
}

impl<S: Subscriber + for<'a> LookupSpan<'a>> Layer<S> for HubLayer {
    fn on_event(&self, event: &tracing::Event<'_>, _ctx: Context<'_, S>) {
        let meta = event.metadata();
        // Чужие крейты шумят на debug/trace — берём только наше и warn+ чужих.
        let ours = meta.target().starts_with("signorebot");
        let level = *meta.level();
        if !ours && level > tracing::Level::WARN {
            return;
        }
        let mut v = MessageVisitor(String::new());
        event.record(&mut v);
        let target = if ours {
            meta.target().trim_start_matches("signorebot_lib::").trim_start_matches("signorebot::").to_string()
        } else {
            meta.target().to_string()
        };
        let entry = LogEntry {
            ts: chrono::Utc::now().timestamp_millis(),
            level: match level {
                tracing::Level::ERROR => "error",
                tracing::Level::WARN => "warn",
                tracing::Level::INFO => "info",
                _ => "debug",
            }
            .into(),
            target,
            message: v.0,
        };
        if let Some(f) = &self.file {
            let line = format!(
                "[{}] [{}] [{}] {}\n",
                chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f"),
                entry.level.to_uppercase(),
                entry.target,
                entry.message
            );
            let _ = f.lock().write_all(line.as_bytes());
        }
        self.hub.push(entry);
    }
}

/// Имя лог-файла `YYYY-MM-DD_logNNN.txt` — первый свободный номер за день.
fn new_log_file(paths: &AppPaths) -> std::io::Result<std::fs::File> {
    let dir = paths.logs_dir();
    std::fs::create_dir_all(&dir)?;
    let date = chrono::Local::now().format("%Y-%m-%d").to_string();
    for n in 1..1000 {
        let p = dir.join(format!("{date}_log{n:03}.txt"));
        if !p.exists() {
            let mut f = std::fs::File::create(&p)?;
            writeln!(f, "{}", "=".repeat(60))?;
            writeln!(f, "  SignoreBot — лог сервера")?;
            writeln!(f, "  Запуск: {}", chrono::Local::now().to_rfc3339())?;
            writeln!(f, "{}\n", "=".repeat(60))?;
            return Ok(f);
        }
    }
    Err(std::io::Error::other("нет свободного имени лог-файла"))
}

static HUB: std::sync::OnceLock<LogHub> = std::sync::OnceLock::new();

/// Глобальный хаб логов (создаётся в `init`; в тестах — пустой).
pub fn hub() -> &'static LogHub {
    HUB.get_or_init(LogHub::new)
}

/// Установить глобальный подписчик. Вызывать один раз.
pub fn init(paths: &AppPaths) -> LogHub {
    let hub = hub().clone();
    let file = match new_log_file(paths) {
        Ok(f) => Some(Arc::new(Mutex::new(f))),
        Err(e) => {
            eprintln!("[logging] не удалось создать лог-файл: {e}");
            None
        }
    };
    let filter = tracing_subscriber::EnvFilter::try_from_env("SIGNOREBOT_LOG")
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info,signorebot_lib=info,signorebot=info"));
    let fmt_layer = tracing_subscriber::fmt::layer().with_target(true).compact();
    let _ = tracing_subscriber::registry()
        .with(filter)
        .with(fmt_layer)
        .with(HubLayer { hub: hub.clone(), file })
        .try_init();
    hub
}

/// Удалить лог-файлы старше `days` дней.
pub fn prune_old_logs(paths: &AppPaths, days: u64) {
    let Ok(rd) = std::fs::read_dir(paths.logs_dir()) else { return };
    let cutoff = std::time::SystemTime::now() - std::time::Duration::from_secs(days * 86400);
    for e in rd.flatten() {
        let p = e.path();
        if p.extension().and_then(|s| s.to_str()) != Some("txt") {
            continue;
        }
        if let Ok(m) = e.metadata() {
            if m.modified().map(|t| t < cutoff).unwrap_or(false) {
                let _ = std::fs::remove_file(p);
            }
        }
    }
}
