//! Оркестратор: держит все подсистемы, следит за авторизацией, поднимает и
//! гасит рантайм (EventSub, планировщик, shoutout, зрители), сервер оверлеев,
//! шлёт события в UI.

use crate::config::migrate::MigrationReport;
use crate::config::{store, Config, SharedConfig};
use crate::engine::periodic::Scheduler;
use crate::engine::{Changed, Engine, Ids};
use crate::logging::LogHub;
use crate::overlay::hub::OverlayHub;
use crate::overlay::server::{self, ServerState};
use crate::paths::AppPaths;
use crate::secrets::{AccountKind, Secrets};
use crate::twitch::accounts::{refresh_loop, AuthEvent, AuthManager};
use crate::twitch::eventsub::{run_session, SessionParams, TwitchEvent};
use crate::twitch::helix::Helix;
use parking_lot::Mutex;
use serde::Serialize;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};
use tokio_util::sync::CancellationToken;

const OVERLAY_STARTUP_CHECK: Duration = Duration::from_secs(15);
const OBS_RETRY_INTERVAL: Duration = Duration::from_secs(30);
const OBS_MAX_ATTEMPTS: u32 = 5;

struct ServerHandle {
    addr: SocketAddr,
    cancel: CancellationToken,
}

#[derive(Default)]
struct ObsAuto {
    attempts: u32,
    done: bool,
    warned_missing: bool,
}

pub struct Core {
    pub paths: AppPaths,
    pub config: SharedConfig,
    pub secrets: Secrets,
    pub logs: LogHub,
    pub auth: Arc<AuthManager>,
    pub helix: Arc<Helix>,
    pub hub: OverlayHub,
    pub engine: Arc<Engine>,
    pub scheduler: Arc<Scheduler>,
    pub start_time: i64,
    app: AppHandle,
    server: Mutex<Option<ServerHandle>>,
    runtime: Mutex<Option<CancellationToken>>,
    migration: Mutex<Option<MigrationReport>>,
    obs_auto: Mutex<ObsAuto>,
    server_error: Mutex<Option<String>>,
    /// Текущее предупреждение «оверлеи не подключены» (для панели и трея).
    overlay_alert: Mutex<Option<String>>,
    /// Результат последней проверки обновлений (для плашки в панели).
    update: Mutex<Option<crate::updates::UpdateInfo>>,
}

#[derive(Debug, Clone, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "api.ts")]
pub struct ServerStatus {
    pub running: bool,
    pub address: Option<String>,
    pub port: u16,
    pub allow_lan: bool,
    pub lan_ip: String,
    pub error: Option<String>,
    pub overlay_key: String,
}

#[derive(Debug, Clone, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "api.ts")]
pub struct OverlayStatusItem {
    pub id: String,
    pub name: String,
    pub path: String,
    pub url: String,
    pub connected: bool,
    pub connections: usize,
    pub pending: usize,
}

#[derive(Debug, Clone, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "api.ts")]
pub struct CoreStatus {
    pub broadcaster: crate::twitch::accounts::AccountStatus,
    pub bot: crate::twitch::accounts::AccountStatus,
    /// Рантайм (EventSub, таймеры) запущен.
    pub running: bool,
    pub eventsub: crate::engine::EventSubStatus,
    pub server: ServerStatus,
    pub overlays: Vec<OverlayStatusItem>,
    pub secrets_backend: String,
    pub data_dir: String,
    pub migration: Option<MigrationReport>,
    pub version: String,
    /// Бот работает, а часть оверлеев не открыта ни одним Browser Source.
    pub overlay_alert: Option<String>,
    /// Последняя проверка обновлений (при запуске, раз в 12 часов и по кнопке).
    pub update: Option<crate::updates::UpdateInfo>,
}

impl Core {
    pub fn start(app: AppHandle, paths: AppPaths) -> anyhow::Result<Arc<Self>> {
        let loaded = store::load_or_create(&paths)?;
        if let Some(r) = &loaded.report {
            tracing::info!(target: "signorebot::config", "Конфиг мигрирован с версии {}", r.from_version);
            for n in &r.notes {
                tracing::info!(target: "signorebot::config", "  · {n}");
            }
        }
        let config: SharedConfig = Arc::new(parking_lot::RwLock::new(loaded.config));
        let secrets = Secrets::open(&paths);
        let auth = AuthManager::new(config.read().twitch.client_id.clone(), secrets.clone());
        {
            let c = config.read();
            auth.set_shared(c.accounts.same_account);
            auth.seed_info(AccountKind::Broadcaster, c.accounts.broadcaster.clone());
            auth.seed_info(AccountKind::Bot, c.accounts.bot.clone());
        }
        let helix = Arc::new(Helix::new(Arc::clone(&auth)));
        let hub = OverlayHub::new();
        let engine = Engine::new(Arc::clone(&config), Arc::clone(&auth), Arc::clone(&helix), hub.clone(), paths.deleted_messages_log());
        let logs = crate::logging::hub().clone();

        let core = Arc::new(Self {
            paths,
            config,
            secrets,
            logs,
            auth,
            helix,
            hub,
            engine,
            scheduler: Arc::new(Scheduler::new()),
            start_time: chrono::Utc::now().timestamp_millis(),
            app,
            server: Mutex::new(None),
            runtime: Mutex::new(None),
            migration: Mutex::new(loaded.report),
            obs_auto: Mutex::new(ObsAuto::default()),
            server_error: Mutex::new(None),
            overlay_alert: Mutex::new(None),
            update: Mutex::new(None),
        });

        core.spawn_forwarders();
        let c = Arc::clone(&core);
        tauri::async_runtime::spawn(async move {
            c.start_server().await;
            c.supervise().await;
        });
        Ok(core)
    }

    pub fn emit_changed(&self, what: &str) {
        let _ = self.app.emit("changed", what);
    }

    /// Пробрасывает логи и «что-то изменилось» в окно.
    fn spawn_forwarders(self: &Arc<Self>) {
        let c = Arc::clone(self);
        tauri::async_runtime::spawn(async move {
            let mut rx = c.logs.subscribe();
            loop {
                match rx.recv().await {
                    Ok(e) => {
                        let _ = c.app.emit("log", &e);
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(_) => break,
                }
            }
        });
        let c = Arc::clone(self);
        tauri::async_runtime::spawn(async move {
            let mut rx = c.engine.subscribe_changes();
            while let Ok(ch) = rx.recv().await {
                if matches!(ch, Changed::Rewards) {
                    // движок мог поправить реакции (название с Twitch, снятая пометка копии)
                    let _ = c.save_config();
                    c.emit_changed("config");
                }
                c.emit_changed(match ch {
                    Changed::Shoutout => "shoutout",
                    Changed::EventSub => "eventsub",
                    Changed::Viewers => "viewers",
                    Changed::Media => "media",
                    Changed::Rewards => "rewards",
                });
            }
        });
        let c = Arc::clone(self);
        tauri::async_runtime::spawn(async move {
            let mut rx = c.hub.subscribe_changes();
            while rx.recv().await.is_ok() {
                c.emit_changed("overlays");
                c.check_overlays_ready();
            }
        });
        let c = Arc::clone(self);
        tauri::async_runtime::spawn(async move {
            let mut rx = c.auth.subscribe();
            while let Ok(AuthEvent::Changed(kind)) = rx.recv().await {
                // Сохраняем несекретную информацию об аккаунте в конфиг.
                let info = c.auth.info(kind);
                let has = c.auth.has_tokens(kind);
                let mut changed = false;
                {
                    let mut cfg = c.config.write();
                    let slot = match kind {
                        AccountKind::Broadcaster => &mut cfg.accounts.broadcaster,
                        AccountKind::Bot => &mut cfg.accounts.bot,
                    };
                    let new_val = if has { info } else { None };
                    if *slot != new_val {
                        *slot = new_val;
                        changed = true;
                    }
                }
                if changed {
                    let _ = c.save_config();
                }
                c.emit_changed("auth");
            }
        });
    }

    // ------------------------------------------------------------------
    // Сервер оверлеев
    // ------------------------------------------------------------------

    pub async fn start_server(self: &Arc<Self>) {
        self.stop_server().await;
        let (port, lan) = {
            let c = self.config.read();
            (c.network.http_port, c.network.allow_lan)
        };
        let state = ServerState {
            config: Arc::clone(&self.config),
            hub: self.hub.clone(),
            media_dir: self.paths.media_dir(),
            start_time: self.start_time,
        };
        let cancel = CancellationToken::new();
        match server::serve(state, port, lan, cancel.clone()).await {
            Ok((addr, _handle)) => {
                tracing::info!(target: "signorebot::server", "Сервер оверлеев: http://{}:{} ({})",
                    if lan { server::local_ip() } else { "127.0.0.1".into() }, addr.port(),
                    if lan { "доступен из локальной сети" } else { "только этот компьютер" });
                *self.server.lock() = Some(ServerHandle { addr, cancel });
                *self.server_error.lock() = None;
                let cfg = self.config.read();
                let key = cfg.network.overlay_key.clone();
                let host = if lan { server::local_ip() } else { "127.0.0.1".into() };
                for o in &cfg.overlays {
                    tracing::info!(target: "signorebot::server", "  «{}» → {}", o.name, server::overlay_url(&host, addr.port(), &o.path, &key));
                }
            }
            Err(e) => {
                tracing::error!(target: "signorebot::server", "{e}. Измените порт в настройках сети.");
                *self.server_error.lock() = Some(e.to_string());
            }
        }
        self.emit_changed("server");
    }

    pub async fn stop_server(&self) {
        let h = self.server.lock().take();
        if let Some(h) = h {
            h.cancel.cancel();
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    pub fn server_status(&self) -> ServerStatus {
        let cfg = self.config.read();
        let s = self.server.lock();
        let lan_ip = server::local_ip();
        ServerStatus {
            running: s.is_some(),
            address: s.as_ref().map(|h| format!("{}:{}", if cfg.network.allow_lan { lan_ip.clone() } else { "127.0.0.1".into() }, h.addr.port())),
            port: cfg.network.http_port,
            allow_lan: cfg.network.allow_lan,
            lan_ip,
            error: self.server_error.lock().clone(),
            overlay_key: cfg.network.overlay_key.clone(),
        }
    }

    pub fn overlay_status(&self) -> Vec<OverlayStatusItem> {
        let cfg = self.config.read();
        let conns = self.hub.connections();
        let port = self.server.lock().as_ref().map(|h| h.addr.port()).unwrap_or(cfg.network.http_port);
        let host = if cfg.network.allow_lan && !cfg.network.prefer_localhost_urls { server::local_ip() } else { "127.0.0.1".into() };
        cfg.overlays
            .iter()
            .map(|o| {
                let n = conns.iter().filter(|c| c.path == o.path).count();
                OverlayStatusItem {
                    id: o.id.clone(),
                    name: o.name.clone(),
                    path: o.path.clone(),
                    url: server::overlay_url(&host, port, &o.path, &cfg.network.overlay_key),
                    connected: n > 0,
                    connections: n,
                    pending: self.hub.pending_count(&o.path),
                }
            })
            .collect()
    }

    fn check_overlays_ready(&self) {
        let st = self.overlay_status();
        if !st.is_empty() && st.iter().all(|o| o.connected) {
            if self.overlay_alert.lock().take().is_some() {
                self.emit_changed("overlays");
            }
            let mut a = self.obs_auto.lock();
            if a.warned_missing {
                tracing::info!(target: "signorebot::overlay", "Все оверлеи подключены: {}", st.iter().map(|o| o.name.as_str()).collect::<Vec<_>>().join(", "));
                a.warned_missing = false;
            }
            a.done = true;
        }
    }

    // ------------------------------------------------------------------
    // Конфиг
    // ------------------------------------------------------------------

    /// Применить изменение к конфигу и сохранить. Подсистемы уведомляются.
    pub fn update_config<F: FnOnce(&mut Config)>(self: &Arc<Self>, f: F) -> Result<(), String> {
        let (net_before, client_before) = {
            let c = self.config.read();
            (c.network.clone(), c.twitch.client_id.clone())
        };
        {
            let mut c = self.config.write();
            f(&mut c);
            c.normalize();
        }
        self.save_config()?;
        self.engine.on_config_changed();
        self.scheduler.reload();
        let (net_after, client_after) = {
            let c = self.config.read();
            (c.network.clone(), c.twitch.client_id.clone())
        };
        if client_after != client_before {
            self.auth.set_client_id(client_after);
        }
        if net_after.http_port != net_before.http_port || net_after.allow_lan != net_before.allow_lan {
            let c = Arc::clone(self);
            tauri::async_runtime::spawn(async move { c.start_server().await });
        }
        self.emit_changed("config");
        Ok(())
    }

    pub fn save_config(&self) -> Result<(), String> {
        let cfg = self.config.read().clone();
        store::save(&self.paths, &cfg).map_err(|e| e.to_string())
    }

    pub fn take_migration_report(&self) -> Option<MigrationReport> {
        self.migration.lock().clone()
    }

    // ------------------------------------------------------------------
    // Рантайм
    // ------------------------------------------------------------------

    async fn supervise(self: Arc<Self>) {
        // Проверка токенов при старте.
        let a = Arc::clone(&self.auth);
        let (b, o) = tokio::join!(a.validate_or_refresh(AccountKind::Broadcaster), a.validate_or_refresh(AccountKind::Bot));
        if !b || !o {
            let missing: Vec<&str> = [(b, "стример"), (o, "бот")].iter().filter(|(ok, _)| !ok).map(|(_, n)| *n).collect();
            tracing::warn!(target: "signorebot::core", "Не авторизован: {}. Откройте вкладку «Авторизация».", missing.join(", "));
        }
        tauri::async_runtime::spawn(refresh_loop(Arc::clone(&self.auth)));
        tauri::async_runtime::spawn(Arc::clone(&self).update_check_loop());
        self.reconcile().await;
        let mut rx = self.auth.subscribe();
        loop {
            match rx.recv().await {
                Ok(_) => self.reconcile().await,
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => self.reconcile().await,
                Err(_) => break,
            }
        }
    }

    async fn reconcile(self: &Arc<Self>) {
        let ready = self.auth.both_ready();
        let running = self.runtime.lock().is_some();
        if ready && !running {
            self.start_runtime().await;
        } else if !ready && running {
            self.stop_runtime().await;
        } else if ready && running {
            // Возможно, сменились id (переавторизация другим аккаунтом).
            let ids = self.current_ids();
            let cur = self.engine.ids();
            if let (Some(a), Some(b)) = (&ids, &cur) {
                if a.broadcaster_id != b.broadcaster_id || a.bot_id != b.bot_id {
                    tracing::info!(target: "signorebot::core", "Аккаунты изменились — перезапуск");
                    self.stop_runtime().await;
                    self.start_runtime().await;
                }
            }
        }
    }

    fn current_ids(&self) -> Option<Ids> {
        let b = self.auth.info(AccountKind::Broadcaster)?;
        let o = self.auth.info(AccountKind::Bot)?;
        Some(Ids { broadcaster_id: b.user_id, bot_id: o.user_id })
    }

    pub fn is_running(&self) -> bool {
        self.runtime.lock().is_some()
    }

    async fn start_runtime(self: &Arc<Self>) {
        let Some(ids) = self.current_ids() else { return };
        let cancel = CancellationToken::new();
        *self.runtime.lock() = Some(cancel.clone());
        self.engine.set_ids(Some(ids.clone()));
        {
            // названия наград могли поменять, пока бот был выключен
            let e = Arc::clone(&self.engine);
            tauri::async_runtime::spawn(async move { e.sync_rewards_from_twitch().await });
        }
        let b = self.auth.info(AccountKind::Broadcaster).map(|i| i.login).unwrap_or_default();
        let o = self.auth.info(AccountKind::Bot).map(|i| i.login).unwrap_or_default();
        if self.auth.is_shared() {
            tracing::info!(target: "signorebot::core", "Запуск: канал {b}, бот — тот же аккаунт");
        } else {
            tracing::info!(target: "signorebot::core", "Запуск: канал {b}, бот {o}");
        }

        // EventSub → движок
        let (tx, mut rx) = tokio::sync::mpsc::channel::<TwitchEvent>(256);
        tauri::async_runtime::spawn(run_session(SessionParams {
            helix: Arc::clone(&self.helix),
            auth: Arc::clone(&self.auth),
            broadcaster_id: ids.broadcaster_id.clone(),
            tx,
            cancel: cancel.clone(),
        }));
        let eng = Arc::clone(&self.engine);
        let c2 = cancel.clone();
        tauri::async_runtime::spawn(async move {
            loop {
                tokio::select! {
                    _ = c2.cancelled() => break,
                    ev = rx.recv() => match ev {
                        Some(ev) => eng.dispatch(ev).await,
                        None => break,
                    }
                }
            }
        });

        // Планировщик периодики
        tauri::async_runtime::spawn(Arc::clone(&self.scheduler).run(Arc::clone(&self.engine), cancel.clone()));

        // Shoutout-воркер
        let eng = Arc::clone(&self.engine);
        let c3 = cancel.clone();
        let notify = self.engine.shoutout.notify_handle();
        tauri::async_runtime::spawn(async move {
            loop {
                tokio::select! {
                    _ = c3.cancelled() => break,
                    _ = notify.notified() => {
                        while eng.shoutout_step().await {
                            if c3.is_cancelled() { break; }
                        }
                    }
                }
            }
        });
        self.engine.shoutout.notify_handle().notify_one();

        // Зрители
        let eng = Arc::clone(&self.engine);
        let c4 = cancel.clone();
        tauri::async_runtime::spawn(async move {
            loop {
                eng.refresh_viewers().await;
                tokio::select! {
                    _ = c4.cancelled() => break,
                    _ = tokio::time::sleep(Duration::from_secs(60)) => {}
                }
            }
        });

        // Монитор оверлеев + OBS
        let c = Arc::clone(self);
        let c5 = cancel.clone();
        tauri::async_runtime::spawn(async move {
            tokio::select! {
                _ = c5.cancelled() => return,
                _ = tokio::time::sleep(OVERLAY_STARTUP_CHECK) => {}
            }
            c.overlay_startup_check(c5).await;
        });
        // Сторож: пока бот работает, а оверлеи не подключены — держим
        // предупреждение в статусе и напоминаем системным уведомлением.
        let c = Arc::clone(self);
        let c6 = cancel.clone();
        tauri::async_runtime::spawn(async move {
            c.overlay_watchdog(c6).await;
        });

        let auto = self.config.read().shoutout.auto_list.clone();
        if !auto.is_empty() {
            tracing::info!(target: "signorebot::core", "Auto-shoutout для: {}", auto.join(", "));
        }
        tracing::info!(target: "signorebot::core", "Ядро бота готово: EventSub, таймеры, shoutout.");
        self.emit_changed("runtime");
    }

    async fn stop_runtime(&self) {
        let c = self.runtime.lock().take();
        if let Some(c) = c {
            c.cancel();
            self.engine.set_ids(None);
            self.engine.reset_eventsub_status();
            tracing::info!(target: "signorebot::core", "Рантайм остановлен (аккаунты не готовы)");
            self.emit_changed("runtime");
        }
    }

    /// Сторож оверлеев: раз в 30 с (первый раз через 15 с) сверяет, все ли
    /// настроенные оверлеи открыты Browser Source'ами. Пока нет — в статусе
    /// висит предупреждение (баннер в панели), а системное уведомление
    /// приходит при появлении проблемы и затем каждые 10 минут — чтобы
    /// «стримлю полчаса, а бот не работает» не случилось незаметно.
    /// Уведомление не шлётся, если окно панели видно и в фокусе: баннер там уже есть.
    async fn overlay_watchdog(self: Arc<Self>, cancel: CancellationToken) {
        const FIRST: Duration = Duration::from_secs(15);
        const PERIOD: Duration = Duration::from_secs(30);
        const REMIND: Duration = Duration::from_secs(600);
        let mut last_notified: Option<std::time::Instant> = None;
        let mut wait = FIRST;
        loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    if self.overlay_alert.lock().take().is_some() {
                        self.emit_changed("overlays");
                    }
                    return;
                }
                _ = tokio::time::sleep(wait) => {}
            }
            wait = PERIOD;
            let st = self.overlay_status();
            let missing: Vec<String> = st.iter().filter(|o| !o.connected).map(|o| o.name.clone()).collect();
            let obs_on = { let o = self.config.read().obs.clone(); o.enabled && o.auto_refresh };
            let alert = overlay_alert_text(&st, obs_on);
            let changed = *self.overlay_alert.lock() != alert;
            if changed {
                *self.overlay_alert.lock() = alert.clone();
                self.emit_changed("overlays");
            }
            match &alert {
                None => last_notified = None,
                Some(text) => {
                    let due = last_notified.map(|t| t.elapsed() >= REMIND).unwrap_or(true);
                    if due {
                        last_notified = Some(std::time::Instant::now());
                        tracing::warn!(target: "signorebot::overlay", "{text}");
                        let visible = self.app.get_webview_window("main").map(|w| w.is_visible().unwrap_or(false) && w.is_focused().unwrap_or(false)).unwrap_or(false);
                        if !visible {
                            use tauri_plugin_notification::NotificationExt;
                            let _ = self.app.notification().builder().title("SignoreBot: оверлеи не подключены").body(format!("Не подключены: {}. Обновите Browser Source в OBS.", missing.join(", "))).show();
                        }
                    }
                }
            }
        }
    }

    /// Через 15 с после старта: если оверлеи не подключились — предупредить,
    /// при включённом OBS — перезагружать Browser Source до 5 раз.
    async fn overlay_startup_check(self: Arc<Self>, cancel: CancellationToken) {
        let st = self.overlay_status();
        if st.is_empty() {
            return;
        }
        let missing: Vec<&OverlayStatusItem> = st.iter().filter(|o| !o.connected).collect();
        if missing.is_empty() {
            tracing::info!(target: "signorebot::overlay", "Все оверлеи подключены: {}", st.iter().map(|o| o.name.as_str()).collect::<Vec<_>>().join(", "));
            return;
        }
        self.obs_auto.lock().warned_missing = true;
        tracing::warn!(target: "signorebot::overlay", "Оверлеи не подключены: {}. Медиа для них будет ждать в очереди 30 с.",
            missing.iter().map(|o| format!("{} ({})", o.name, o.path)).collect::<Vec<_>>().join(", "));
        let obs = self.config.read().obs.clone();
        if !obs.enabled || !obs.auto_refresh {
            tracing::warn!(target: "signorebot::overlay", "Если OBS был запущен раньше бота — обновите Browser Source вручную или включите интеграцию с OBS на вкладке «Оверлеи».");
            return;
        }
        for attempt in 1..=OBS_MAX_ATTEMPTS {
            if cancel.is_cancelled() {
                return;
            }
            let st = self.overlay_status();
            let missing: Vec<String> = st.iter().filter(|o| !o.connected).map(|o| o.path.clone()).collect();
            if missing.is_empty() {
                self.obs_auto.lock().done = true;
                return;
            }
            self.obs_auto.lock().attempts = attempt;
            tracing::info!(target: "signorebot::obs", "Попытка перезагрузить Browser Source ({attempt}/{OBS_MAX_ATTEMPTS})…");
            let obs = self.config.read().obs.clone();
            let names: Vec<String> = obs.browser_sources.iter().filter(|b| missing.contains(&b.overlay_path)).map(|b| b.input_name.clone()).collect();
            if names.is_empty() {
                tracing::warn!(target: "signorebot::obs", "Нет привязок Browser Source для неподключённых оверлеев — настройте их на вкладке «Оверлеи».");
                return;
            }
            match crate::overlay::obs::refresh_browser_sources(&obs, &names).await {
                Ok(r) if !r.is_empty() => tracing::info!(target: "signorebot::obs", "Обновлено источников: {}. Ждём подключения…", r.len()),
                Ok(_) => tracing::warn!(target: "signorebot::obs", "Ни один источник не обновлён"),
                Err(e) => tracing::warn!(target: "signorebot::obs", "{e}"),
            }
            tokio::select! {
                _ = cancel.cancelled() => return,
                _ = tokio::time::sleep(OBS_RETRY_INTERVAL) => {}
            }
        }
        tracing::warn!(target: "signorebot::obs", "Исчерпаны попытки перезагрузки Browser Source. Обновите источники в OBS вручную.");
    }

    // ------------------------------------------------------------------
    // Статус для UI
    // ------------------------------------------------------------------

    pub fn status(&self) -> CoreStatus {
        CoreStatus {
            broadcaster: self.auth.status(AccountKind::Broadcaster),
            bot: self.auth.status(AccountKind::Bot),
            running: self.is_running(),
            eventsub: self.engine.eventsub_status(),
            server: self.server_status(),
            overlays: self.overlay_status(),
            secrets_backend: self.secrets.backend_name().into(),
            data_dir: self.paths.root.display().to_string(),
            migration: self.take_migration_report(),
            version: env!("CARGO_PKG_VERSION").into(),
            overlay_alert: self.overlay_alert.lock().clone(),
            update: self.update.lock().clone(),
        }
    }

    /// Проверить релизы на GitHub, запомнить результат, сообщить панели.
    pub async fn check_updates(&self) -> Result<crate::updates::UpdateInfo, String> {
        let repo = self.config.read().updates.repo_url.clone();
        let info = crate::updates::check(&repo).await?;
        if info.is_newer {
            tracing::info!(target: "signorebot::updates", "Доступно обновление {} (текущая {})", info.latest.clone().unwrap_or_default(), info.current);
        } else {
            tracing::info!(target: "signorebot::updates", "Обновлений нет (текущая {}{})", info.current, info.latest.as_ref().map(|l| format!(", последний релиз {l}")).unwrap_or_default());
        }
        *self.update.lock() = Some(info.clone());
        self.emit_changed("updates");
        Ok(info)
    }

    /// Проверка обновлений сама по себе: через 20 с после запуска (если
    /// включено в настройках) и дальше раз в 12 часов, пока приложение
    /// живёт в трее. Проверка при закрытии не имеет смысла: «закрыть» обычно
    /// прячет окно в трей, а результат всё равно нужен на следующем запуске.
    async fn update_check_loop(self: Arc<Self>) {
        tokio::time::sleep(Duration::from_secs(20)).await;
        loop {
            if self.config.read().updates.check_on_start {
                if let Err(e) = self.check_updates().await {
                    tracing::warn!(target: "signorebot::updates", "Проверка обновлений не удалась: {e}");
                }
            }
            tokio::time::sleep(Duration::from_secs(12 * 3600)).await;
        }
    }

    /// Включить/выключить режим «бот — тот же аккаунт».
    pub fn set_same_account(self: &Arc<Self>, on: bool) -> Result<(), String> {
        self.update_config(|cfg| cfg.accounts.same_account = on)?;
        self.auth.set_shared(on);
        self.emit_changed("config");
        Ok(())
    }

    pub fn dismiss_migration(&self) {
        *self.migration.lock() = None;
    }

    pub async fn shutdown(&self) {
        if let Some(c) = self.runtime.lock().take() {
            c.cancel();
        }
        self.stop_server().await;
    }
}

/// Текст предупреждения для сторожа оверлеев (None — всё подключено или
/// оверлеев нет).
pub fn overlay_alert_text(st: &[OverlayStatusItem], obs_on: bool) -> Option<String> {
    let missing: Vec<&str> = st.iter().filter(|o| !o.connected).map(|o| o.name.as_str()).collect();
    if st.is_empty() || missing.is_empty() {
        return None;
    }
    let hint = if obs_on {
        "Бот пробовал перезагрузить Browser Source через OBS — проверьте, что OBS запущен и подключение к нему работает."
    } else {
        "Если OBS был запущен раньше бота — нажмите «Обновить» у Browser Source в OBS (или включите интеграцию с OBS на вкладке «Оверлеи», тогда бот будет делать это сам)."
    };
    Some(format!("Не подключены оверлеи: {}. Медиа и алерты на них не дойдут. {hint}", missing.join(", ")))
}

#[cfg(test)]
mod overlay_alert_tests {
    use super::*;
    fn item(name: &str, connected: bool) -> OverlayStatusItem {
        OverlayStatusItem { id: name.into(), name: name.into(), path: name.to_lowercase(), url: String::new(), connected, connections: connected as usize, pending: 0 }
    }
    #[test]
    fn alert_only_when_some_overlay_is_missing() {
        assert_eq!(overlay_alert_text(&[], false), None);
        assert_eq!(overlay_alert_text(&[item("Аудио", true), item("Видео", true)], false), None);
        let t = overlay_alert_text(&[item("Аудио", true), item("Видео", false), item("VIPS", false)], false).unwrap();
        assert!(t.starts_with("Не подключены оверлеи: Видео, VIPS."), "{t}");
        assert!(t.contains("нажмите «Обновить»"));
        let t = overlay_alert_text(&[item("Видео", false)], true).unwrap();
        assert!(t.contains("через OBS"));
    }
}
