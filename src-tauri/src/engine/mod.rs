//! Движок: команды, награды, события, периодика, shoutout, банворды,
//! исполнение реакций (чат + медиа).

pub mod banwords;
pub mod message;
pub mod periodic;
pub mod shoutout;

use crate::config::{Config, QueueMode, Response, SharedConfig};
use crate::overlay::hub::OverlayHub;
use crate::secrets::AccountKind;
use crate::twitch::accounts::AuthManager;
use crate::twitch::eventsub::{ChatMessage, TwitchEvent};
use crate::twitch::helix::{Helix, HelixError};
use message::{render, RenderCtx};
use parking_lot::Mutex;
use serde::Serialize;
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::broadcast;

const REWARD_EVENT_TTL: Duration = Duration::from_secs(30);
const REWARD_CROSS_SOURCE_TTL: Duration = Duration::from_secs(5);
const VIEWERS_TTL: Duration = Duration::from_secs(60);
const MAX_RECENT_CHATTERS: usize = 200;

/// Что изменилось (UI перезапрашивает соответствующий статус).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Changed {
    Shoutout,
    EventSub,
    Viewers,
    Media,
    /// Награды канала или список погашений изменились.
    Rewards,
}

/// Погашение награды, которое бот не смог выполнить (оверлей был выключен),
/// либо закрыл сам. Показывается на «Баллах канала».
#[derive(Debug, Clone, Serialize, serde::Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "api.ts")]
pub struct PendingRedemption {
    pub redemption_id: String,
    pub reward_id: String,
    pub reward_title: String,
    pub user: String,
    #[ts(type = "number")]
    pub at: i64,
    /// Почему не выполнено (какой оверлей был недоступен).
    pub reason: String,
    /// pending | refunded (бот вернул) | canceled (вернули в Twitch) | fulfilled | dismissed
    pub status: String,
}

/// Итог отправки медиа на оверлеи.
#[derive(Debug, Default)]
struct MediaSend {
    sent: bool,
    /// В настройках нет ни одного целевого оверлея.
    no_target: bool,
    /// Оверлеи (имя, путь), которые не были подключены.
    unavailable: Vec<(String, String)>,
    /// Резервные реакции недоступных оверлеев (имя оверлея, реакция).
    fallbacks: Vec<(String, Response)>,
}

fn load_redemptions(path: &std::path::Path) -> Vec<PendingRedemption> {
    std::fs::read(path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

/// Итог выполнения реакции.
#[derive(Debug, Default, Clone)]
pub struct ExecOutcome {
    pub chat_sent: bool,
    pub media_sent: bool,
    /// Медиа было включено, но ни один целевой оверлей его не получил.
    pub media_unavailable: bool,
    pub unavailable_overlays: Vec<String>,
}

#[derive(Debug, Clone, Serialize, ts_rs::TS, Default)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "api.ts")]
pub struct EventSubStatus {
    pub connected: bool,
    pub session_id: Option<String>,
    pub subscriptions: usize,
}

#[derive(Debug, Clone)]
pub struct Ids {
    pub broadcaster_id: String,
    pub bot_id: String,
}

/// Источник награды — для кросс-дедупа.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RewardSource {
    EventSub,
    Chat,
}

#[derive(Default)]
struct RewardDedup {
    by_event: HashMap<String, Instant>,
    /// Отпечаток «награда + зритель + текст» → когда, через какие источники,
    /// чем закончилось выполнение (нужно, если чат опередил EventSub).
    by_fingerprint: HashMap<String, (Instant, HashSet<RewardSource>, Option<RewardOutcome>)>,
}

/// Есть что показать на оверлее: файл или хотя бы текст (алерт без файла).
fn media_has_payload(m: &crate::config::MediaResponse) -> bool {
    !m.file.is_empty() || (m.text.enabled && !m.text.content.trim().is_empty())
}

/// Итог выполнения реакции на награду — для учёта погашения.
#[derive(Debug, Clone)]
struct RewardOutcome {
    media_unavailable: bool,
    unavailable_overlays: Vec<String>,
    media_sent: bool,
}

/// Результат проверки на дубликат награды.
enum RewardDup {
    No,
    /// Тот же id погашения уже обработан.
    SameEvent,
    /// Та же награда уже пришла через другой источник; исход — если известен.
    CrossSource { via: &'static str, outcome: Option<RewardOutcome> },
}

#[derive(Default)]
struct Viewers {
    cache: Vec<String>,
    cache_at: Option<Instant>,
    recent: VecDeque<String>,
    scope_warned: bool,
}

/// Контекст исполнения реакции.
#[derive(Debug, Clone, Default)]
pub struct ActionCtx {
    pub author: String,
    pub target: Option<String>,
    pub vars: BTreeMap<String, String>,
    /// Подпись для логов («!кусь», «Награда: …», «Событие: follow»).
    pub label: String,
    /// Логин для антиспама медиа (None — не применять).
    pub antispam_user: Option<String>,
    /// Id сообщения чата, на которое отвечать реплаем (None — обычное сообщение).
    pub reply_to: Option<String>,
}

pub struct Engine {
    pub config: SharedConfig,
    pub auth: Arc<AuthManager>,
    pub helix: Arc<Helix>,
    pub hub: OverlayHub,
    pub shoutout: shoutout::ShoutoutQueue,
    deleted_log: std::path::PathBuf,
    ids: Mutex<Option<Ids>>,
    banwords: Mutex<Arc<banwords::Matcher>>,
    rewards: Mutex<RewardDedup>,
    viewers: Mutex<Viewers>,
    cooldowns: Mutex<HashMap<String, Instant>>,
    /// Кулдаун на зрителя: «id команды:логин» → последнее срабатывание.
    user_cooldowns: Mutex<HashMap<String, Instant>>,
    antispam: Mutex<HashMap<(String, String), Instant>>,
    eventsub: Mutex<EventSubStatus>,
    changed_tx: broadcast::Sender<Changed>,
    /// Идентификаторы сообщений, отправленных ботом (последние 64): по ним
    /// отсеиваем собственное эхо из EventSub, не завися от того, совпадает ли
    /// аккаунт бота с аккаунтом стримера.
    sent_ids: Mutex<std::collections::VecDeque<String>>,
    /// Сколько отправок ещё не получили ответ Helix (гонка с EventSub).
    in_flight: std::sync::atomic::AtomicUsize,
    /// Невыполненные/закрытые погашения (последние 200), файл `redemptions.json`.
    redemptions: Mutex<Vec<PendingRedemption>>,
    redemptions_file: std::path::PathBuf,
}

impl Engine {
    pub fn new(config: SharedConfig, auth: Arc<AuthManager>, helix: Arc<Helix>, hub: OverlayHub, deleted_log: std::path::PathBuf) -> Arc<Self> {
        let (changed_tx, _) = broadcast::channel(64);
        // redemptions.json — рядом с логами, в корне каталога данных
        let redemptions_file = deleted_log.parent().and_then(|p| p.parent()).map(|p| p.join("redemptions.json")).unwrap_or_else(|| std::path::PathBuf::from("redemptions.json"));
        let matcher = banwords::Matcher::compile(&config.read().banwords);
        Arc::new(Self {
            config,
            auth,
            helix,
            hub,
            shoutout: shoutout::ShoutoutQueue::new(),
            deleted_log,
            ids: Mutex::new(None),
            banwords: Mutex::new(Arc::new(matcher)),
            rewards: Mutex::new(RewardDedup::default()),
            viewers: Mutex::new(Viewers::default()),
            cooldowns: Mutex::new(HashMap::new()),
            user_cooldowns: Mutex::new(HashMap::new()),
            antispam: Mutex::new(HashMap::new()),
            sent_ids: Mutex::new(std::collections::VecDeque::with_capacity(64)),
            in_flight: std::sync::atomic::AtomicUsize::new(0),
            redemptions: Mutex::new(load_redemptions(&redemptions_file)),
            redemptions_file,
            eventsub: Mutex::new(EventSubStatus::default()),
            changed_tx,
        })
    }

    pub fn subscribe_changes(&self) -> broadcast::Receiver<Changed> {
        self.changed_tx.subscribe()
    }
    fn changed(&self, what: Changed) {
        let _ = self.changed_tx.send(what);
    }

    /// Сверка реакций со списком наград Twitch при запуске: пока бот был
    /// выключен, награду могли переименовать или удалить — событий об этом
    /// он не видел. Названия подтягиваются, об отсутствующих — предупреждение.
    pub async fn sync_rewards_from_twitch(&self) {
        let Some(ids) = self.ids() else { return };
        if self.config.read().rewards.is_empty() {
            return;
        }
        let channel = match self.helix.custom_rewards(AccountKind::Broadcaster, &ids.broadcaster_id).await {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!(target: "signorebot::rewards", "Не удалось сверить награды с Twitch: {e}");
                return;
            }
        };
        let mut renamed: Vec<(String, String)> = Vec::new();
        let mut missing: Vec<String> = Vec::new();
        {
            let mut cfg = self.config.write();
            for r in cfg.rewards.iter_mut() {
                match channel.iter().find(|c| c.id == r.reward_id) {
                    Some(c) if c.title != r.reward_title => {
                        renamed.push((r.reward_title.clone(), c.title.clone()));
                        r.reward_title = c.title.clone();
                    }
                    Some(_) => {}
                    None => missing.push(r.reward_title.clone()),
                }
            }
        }
        for (old, new) in &renamed {
            tracing::info!(target: "signorebot::rewards", "Награда «{old}» переименована на Twitch, пока бот был выключен: теперь «{new}»");
        }
        if !missing.is_empty() {
            tracing::warn!(target: "signorebot::rewards", "Наград нет на канале (удалены на Twitch): {}; реакции сохранены, на вкладке «Баллы канала» они помечены «нет на канале»", missing.iter().map(|m| format!("«{m}»")).collect::<Vec<_>>().join(", "));
        }
        if !renamed.is_empty() {
            self.changed(Changed::Rewards);
        }
    }

    pub fn set_ids(&self, ids: Option<Ids>) {
        *self.ids.lock() = ids;
    }
    pub fn ids(&self) -> Option<Ids> {
        self.ids.lock().clone()
    }
    pub fn eventsub_status(&self) -> EventSubStatus {
        self.eventsub.lock().clone()
    }
    pub fn reset_eventsub_status(&self) {
        *self.eventsub.lock() = EventSubStatus::default();
        self.changed(Changed::EventSub);
    }

    /// Конфиг изменился: пересобрать производные структуры.
    pub fn on_config_changed(&self) {
        let matcher = banwords::Matcher::compile(&self.config.read().banwords);
        *self.banwords.lock() = Arc::new(matcher);
    }

    // ------------------------------------------------------------------
    // Зрители
    // ------------------------------------------------------------------

    pub fn note_chatter(&self, name: &str) {
        let mut v = self.viewers.lock();
        if v.recent.iter().any(|x| x == name) {
            return;
        }
        v.recent.push_back(name.to_string());
        while v.recent.len() > MAX_RECENT_CHATTERS {
            v.recent.pop_front();
        }
    }

    /// Обновить кэш зрителей (раз в минуту из фоновой задачи).
    pub async fn refresh_viewers(&self) {
        let Some(ids) = self.ids() else { return };
        let bot_login = self.auth.info(AccountKind::Bot).map(|i| i.login).unwrap_or_default();
        match self.helix.chatters(AccountKind::Broadcaster, &ids.broadcaster_id, &ids.broadcaster_id).await {
            Ok(list) => {
                let names: Vec<String> = list.into_iter().filter(|c| c.user_login != bot_login).map(|c| c.user_name).collect();
                let mut v = self.viewers.lock();
                v.cache = names;
                v.cache_at = Some(Instant::now());
                v.scope_warned = false;
            }
            Err(e) => {
                let mut v = self.viewers.lock();
                if !v.scope_warned {
                    v.scope_warned = true;
                    tracing::warn!(target: "signorebot::viewers", "Не удалось получить список зрителей: {e} (сообщение показывается один раз)");
                }
            }
        }
        self.changed(Changed::Viewers);
    }

    pub fn viewers_snapshot(&self) -> (Vec<String>, Vec<String>) {
        let v = self.viewers.lock();
        (v.cache.clone(), v.recent.iter().cloned().collect())
    }

    pub async fn random_viewer(&self) -> String {
        use rand::seq::SliceRandom;
        let stale = self.viewers.lock().cache_at.map(|t| t.elapsed() > VIEWERS_TTL).unwrap_or(true);
        if stale {
            self.refresh_viewers().await;
        }
        let v = self.viewers.lock();
        if let Some(x) = v.cache.choose(&mut rand::thread_rng()) {
            return x.clone();
        }
        if !v.recent.is_empty() {
            let idx = rand::random::<usize>() % v.recent.len();
            return v.recent[idx].clone();
        }
        ["friend", "viewer", "chatter", "follower", "subscriber"].choose(&mut rand::thread_rng()).unwrap().to_string()
    }

    // ------------------------------------------------------------------
    // Исполнение реакции
    // ------------------------------------------------------------------

    /// Отправить текст в чат от имени бота.
    pub async fn say(&self, text: &str) -> bool {
        self.say_to(text, None).await
    }

    /// Отправить текст в чат; `reply_to` — id сообщения, на которое отвечаем реплаем.
    pub async fn say_to(&self, text: &str, reply_to: Option<&str>) -> bool {
        let Some(ids) = self.ids() else {
            tracing::warn!(target: "signorebot::chat", "Чат недоступен (аккаунты не готовы), сообщение не отправлено");
            return false;
        };
        self.in_flight.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let result = self.helix.send_chat_message(AccountKind::Bot, &ids.broadcaster_id, &ids.bot_id, text, reply_to).await;
        if let Ok(r) = &result {
            self.remember_sent(&r.message_id);
        }
        self.in_flight.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
        match result {
            Ok(r) if r.is_sent => {
                tracing::info!(target: "signorebot::chat", "Чат: {text}");
                true
            }
            Ok(r) => {
                let why = r.drop_reason.map(|d| format!("{}: {}", d.code, d.message)).unwrap_or_else(|| "отклонено Twitch".into());
                tracing::warn!(target: "signorebot::chat", "Сообщение не доставлено ({why}): {text}");
                false
            }
            Err(e) => {
                tracing::error!(target: "signorebot::chat", "Ошибка отправки в чат: {e}");
                false
            }
        }
    }

    /// Выполнить реакцию: чат и/или медиа. Возвращает (chat_sent, media_sent).
    pub async fn execute(&self, response: &Response, ctx: &ActionCtx) -> (bool, bool) {
        let o = self.execute_full(response, ctx).await;
        (o.chat_sent, o.media_sent)
    }

    /// То же с подробным итогом (нужно наградам: было ли медиа доставлено).
    pub async fn execute_full(&self, response: &Response, ctx: &ActionCtx) -> ExecOutcome {
        self.execute_full_opt(response, ctx, true).await
    }

    /// `queue = false` — медиа для неподключённого оверлея не откладывать
    /// (награда с возвратом баллов: зритель получит баллы, а не медиа позже).
    pub async fn execute_full_opt(&self, response: &Response, ctx: &ActionCtx, queue: bool) -> ExecOutcome {
        let mut out = ExecOutcome::default();
        let mut chat_sent = false;
        let mut media_sent = false;

        if response.chat.enabled && !response.chat.components.is_empty() {
            let random_viewer = if RenderCtx::needs_random_viewer(&response.chat.components, &ctx.target) {
                Some(self.random_viewer().await)
            } else {
                None
            };
            let rctx = RenderCtx { author: ctx.author.clone(), target: ctx.target.clone(), vars: ctx.vars.clone(), random_viewer };
            let text = render(&response.chat.components, &rctx);
            if !text.is_empty() {
                chat_sent = self.say_to(&text, ctx.reply_to.as_deref()).await;
            }
        }

        if response.media.enabled && media_has_payload(&response.media) {
            let r = self.send_media(response, ctx, true, queue);
            media_sent = r.sent;
            out.media_unavailable = !r.sent && !r.no_target;
            out.unavailable_overlays = r.unavailable.iter().map(|(n, _)| n.clone()).collect();
            // Фолбэк оверлея: текст в чат и/или медиа на другой оверлей.
            for (name, fb) in r.fallbacks {
                tracing::info!(target: "signorebot::media", "Оверлей «{name}» недоступен — срабатывает его резервная реакция ({})", ctx.label);
                let mut fctx = ctx.clone();
                fctx.vars.insert("overlay".into(), name.clone());
                fctx.vars.insert("reaction".into(), ctx.label.clone());
                fctx.label = format!("{} → резерв «{name}»", ctx.label);
                fctx.antispam_user = None;
                if fb.chat.enabled && !fb.chat.components.is_empty() {
                    let rctx = RenderCtx { author: fctx.author.clone(), target: fctx.target.clone(), vars: fctx.vars.clone(), random_viewer: None };
                    let text = render(&fb.chat.components, &rctx);
                    if !text.is_empty() {
                        self.say(&text).await;
                    }
                }
                if fb.media.enabled && media_has_payload(&fb.media) {
                    // без повторного фолбэка — чтобы не зациклиться
                    let _ = self.send_media(&fb, &fctx, false, true);
                }
            }
        }
        out.chat_sent = chat_sent;
        out.media_sent = media_sent;
        out
    }

    /// Отправить медиа на целевые оверлеи. Если у оверлея задана резервная
    /// реакция, сообщение ему не ставится в отложенную очередь — вместо
    /// этого возвращается реакция для исполнения (`allow_fallback`).
    fn send_media(&self, response: &Response, ctx: &ActionCtx, allow_fallback: bool, queue: bool) -> MediaSend {
        let m = &response.media;
        let cfg = self.config.read();

        // Антиспам: тот же файл от того же пользователя в узком окне.
        let window = cfg.overlay_settings.antispam_window_ms;
        if window > 0 {
            if let Some(user) = &ctx.antispam_user {
                let key = (user.to_lowercase(), m.file.clone());
                let mut map = self.antispam.lock();
                let now = Instant::now();
                map.retain(|_, t| now.duration_since(*t) < Duration::from_secs(60));
                if let Some(t) = map.get(&key) {
                    if now.duration_since(*t) < Duration::from_millis(window as u64) {
                        tracing::info!(target: "signorebot::media", "Антиспам: «{}» от {} повторно за {} мс, пропуск", m.file, user, now.duration_since(*t).as_millis());
                        return MediaSend::default();
                    }
                }
                map.insert(key, now);
            }
        }

        let rctx = RenderCtx { author: ctx.author.clone(), target: ctx.target.clone(), vars: ctx.vars.clone(), random_viewer: None };
        let mut payload = serde_json::json!({
            "command": "playVideo",
            "videoFile": m.file,
            "secondaryFile": if m.secondary_file.is_empty() { serde_json::Value::Null } else { serde_json::Value::String(m.secondary_file.clone()) },
            "volume": m.volume,
            "queueMode": match m.queue_mode { QueueMode::Queue => "queue", QueueMode::Immediate => "immediate" },
            "chromakey": m.chromakey,
            "animation": {
                "enter": m.animation.enter, "exit": m.animation.exit,
                "enterDuration": m.animation.enter_duration, "exitDuration": m.animation.exit_duration,
            },
        });
        if let Some(d) = m.image_duration_sec {
            payload["duration"] = serde_json::json!(d);
        }
        if m.text.enabled {
            payload["text"] = serde_json::json!({
                "enabled": true,
                "content": message::substitute(&m.text.content, &rctx),
                "position": m.text.position,
                "animation": m.text.animation,
                "animationAmplitude": m.text.animation_amplitude,
                "font": m.text.font,
            });
        }
        let msg = payload.to_string();

        // Целевые оверлеи: конкретный или все настроенные.
        let targets: Vec<(String, String)> = match &m.overlay {
            Some(id) => match cfg.overlay_by_id(id) {
                Some(o) => vec![(o.name.clone(), o.path.clone())],
                None => {
                    tracing::warn!(target: "signorebot::media", "Оверлей «{id}» не найден в настройках, медиа «{}» отправлено на все оверлеи", m.file);
                    cfg.overlays.iter().map(|o| (o.name.clone(), o.path.clone())).collect()
                }
            },
            None => cfg.overlays.iter().map(|o| (o.name.clone(), o.path.clone())).collect(),
        };
        drop(cfg);

        if targets.is_empty() {
            tracing::warn!(target: "signorebot::media", "Медиа «{}» ({}) — в настройках нет ни одного оверлея", m.file, ctx.label);
            return MediaSend { no_target: true, ..Default::default() };
        }
        let mut out = MediaSend::default();
        for (name, path) in targets {
            let fallback = if allow_fallback { self.config.read().overlays.iter().find(|o| o.path == path && o.fallback_enabled).and_then(|o| o.fallback.clone()) } else { None };
            let has_fb = fallback.as_ref().map(|f| (f.chat.enabled && !f.chat.components.is_empty()) || (f.media.enabled && !f.media.file.is_empty())).unwrap_or(false);
            if self.hub.send_to_path_opt(&path, &msg, queue && !has_fb) {
                tracing::info!(target: "signorebot::media", "Медиа: «{}» → оверлей «{name}»", ctx.label);
                out.sent = true;
            } else if has_fb {
                tracing::warn!(target: "signorebot::media", "Медиа: «{}» → оверлей «{name}» не подключён; в очередь не ставим — есть резервная реакция", ctx.label);
                out.unavailable.push((name.clone(), path.clone()));
                out.fallbacks.push((name, fallback.unwrap()));
            } else if !queue {
                tracing::warn!(target: "signorebot::media", "Медиа: «{}» → оверлей «{name}» не подключён; в очередь не ставим — баллы будут возвращены", ctx.label);
                out.unavailable.push((name, path));
            } else {
                tracing::warn!(target: "signorebot::media", "Медиа: «{}» → оверлей «{name}» не подключён, поставлено в очередь (30 с)", ctx.label);
                out.unavailable.push((name, path));
            }
        }
        self.changed(Changed::Media);
        out
    }

    // ------------------------------------------------------------------
    // Погашения наград
    // ------------------------------------------------------------------

    pub fn redemptions(&self) -> Vec<PendingRedemption> {
        self.redemptions.lock().clone()
    }

    fn save_redemptions(&self, list: &[PendingRedemption]) {
        if let Ok(json) = serde_json::to_vec_pretty(list) {
            let _ = std::fs::write(&self.redemptions_file, json);
        }
    }

    fn push_redemption(&self, r: PendingRedemption) {
        let mut l = self.redemptions.lock();
        l.retain(|x| x.redemption_id != r.redemption_id);
        l.insert(0, r);
        l.truncate(200);
        self.save_redemptions(&l);
        drop(l);
        self.changed(Changed::Rewards);
    }

    fn set_redemption_status(&self, redemption_id: &str, status: &str) -> bool {
        let mut l = self.redemptions.lock();
        let Some(r) = l.iter_mut().find(|x| x.redemption_id == redemption_id) else { return false };
        if r.status == status {
            return false;
        }
        r.status = status.to_string();
        self.save_redemptions(&l);
        drop(l);
        self.changed(Changed::Rewards);
        true
    }

    /// Убрать запись из списка (пользователь разобрался сам).
    pub fn dismiss_redemption(&self, redemption_id: &str) {
        self.set_redemption_status(redemption_id, "dismissed");
    }

    /// Погашение закрыто в Twitch (стример/модератор/бот) — отмечаем.
    pub fn on_redemption_update(&self, redemption_id: &str, status: &str) {
        let st = match status { "canceled" => "canceled", "fulfilled" => "fulfilled", _ => return };
        if self.set_redemption_status(redemption_id, st) {
            tracing::info!(target: "signorebot::rewards", "Погашение {redemption_id}: {}", if st == "canceled" { "баллы возвращены" } else { "выполнено" });
        }
    }

    // ------------------------------------------------------------------
    // Чат
    // ------------------------------------------------------------------

    /// Запомнить id отправленного сообщения (для отсева эха).
    pub fn remember_sent(&self, message_id: &str) {
        if message_id.is_empty() {
            return;
        }
        let mut q = self.sent_ids.lock();
        if q.len() >= 64 {
            q.pop_front();
        }
        q.push_back(message_id.to_string());
    }

    fn take_sent(&self, message_id: &str) -> bool {
        let mut q = self.sent_ids.lock();
        if let Some(pos) = q.iter().position(|x| x == message_id) {
            q.remove(pos);
            true
        } else {
            false
        }
    }

    /// Это эхо нашего же сообщения? Сначала по `message_id`. Если аккаунты
    /// разные — достаточно автора. Если бот и стример — один аккаунт, автор
    /// ничего не говорит: тогда ждём, пока завершатся отправки «в полёте»
    /// (EventSub может обогнать ответ Helix), и проверяем id ещё раз.
    async fn is_own_echo(&self, msg: &ChatMessage) -> bool {
        if self.take_sent(&msg.message_id) {
            return true;
        }
        let Some(ids) = self.ids() else { return false };
        if msg.user_id != ids.bot_id {
            return false;
        }
        if ids.bot_id != ids.broadcaster_id {
            return true; // отдельный бот: всё от него — наше
        }
        for _ in 0..10 {
            if self.in_flight.load(std::sync::atomic::Ordering::SeqCst) == 0 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
            if self.take_sent(&msg.message_id) {
                return true;
            }
        }
        self.take_sent(&msg.message_id)
    }

    pub async fn on_chat(&self, msg: ChatMessage) {
        if self.is_own_echo(&msg).await {
            return;
        }
        self.note_chatter(&msg.user_name);

        // Авто-shoutout по первому сообщению.
        {
            let cfg = self.config.read();
            let list = cfg.shoutout.auto_list.clone();
            drop(cfg);
            if self.shoutout.enqueue_message(&msg.user_login, &list) {
                self.changed(Changed::Shoutout);
            }
        }

        if let Some(reward_id) = &msg.reward_id {
            self.handle_reward(reward_id, &msg.user_name, &msg.text, RewardSource::Chat, None).await;
            return;
        }

        if self.check_banword(&msg).await {
            return;
        }

        if !msg.text.starts_with('!') {
            return;
        }
        self.handle_command(&msg).await;
    }

    async fn check_banword(&self, msg: &ChatMessage) -> bool {
        let (skip_priv, matcher) = {
            let cfg = self.config.read();
            (cfg.banwords.skip_privileged, Arc::clone(&self.banwords.lock()))
        };
        if skip_priv && (msg.is_broadcaster || msg.is_moderator) {
            return false;
        }
        let Some(hit) = matcher.check(&msg.text) else { return false };
        tracing::info!(target: "signorebot::banwords", "Запрещённое слово «{}» ({:?}) от {}: «{}»", hit.word, hit.kind, msg.user_name, msg.text);
        if msg.is_broadcaster || msg.is_moderator {
            tracing::info!(target: "signorebot::banwords", "Сообщение {} не удалено: Twitch не позволяет удалять сообщения стримера и модераторов", msg.user_name);
            return true;
        }
        let Some(ids) = self.ids() else { return true };
        match self.helix.delete_chat_message(AccountKind::Bot, &ids.broadcaster_id, &ids.bot_id, &msg.message_id).await {
            Ok(()) => {
                tracing::info!(target: "signorebot::banwords", "Сообщение удалено");
                let line = format!(
                    "[{}] Удалено от {}: \"{}\" (слово: \"{}\", тип: {:?})\n",
                    chrono::Local::now().to_rfc3339(),
                    msg.user_name,
                    msg.text,
                    hit.word,
                    hit.kind
                );
                let _ = std::fs::OpenOptions::new().create(true).append(true).open(&self.deleted_log).and_then(|mut f| {
                    use std::io::Write;
                    f.write_all(line.as_bytes())
                });
            }
            Err(e) => {
                let hint = match e.status() {
                    Some(403) => " — бот должен быть модератором канала",
                    _ => "",
                };
                tracing::warn!(target: "signorebot::banwords", "Не удалось удалить сообщение: {e}{hint}");
            }
        }
        true
    }

    pub fn has_permission(msg: &ChatMessage, perms: &[String]) -> bool {
        if perms.is_empty() {
            return true;
        }
        let login = msg.user_login.to_lowercase();
        perms.iter().any(|p| match p.as_str() {
            "everyone" => true,
            "broadcaster" => msg.is_broadcaster,
            "moderators" => msg.is_moderator || msg.is_broadcaster,
            "vips" => msg.is_vip,
            "subscribers" => msg.is_subscriber,
            other => other.strip_prefix("user:").map(|u| u.to_lowercase() == login).unwrap_or(false),
        })
    }

    async fn handle_command(&self, msg: &ChatMessage) {
        // После «!» сразу имя: «! кусь» — не команда (как в старой версии).
        if msg.text[1..].starts_with(char::is_whitespace) {
            return;
        }
        let mut parts = msg.text[1..].split_whitespace();
        let Some(name) = parts.next() else { return };
        let name = name.to_lowercase();
        let args: Vec<&str> = parts.collect();

        let cmd = {
            let cfg = self.config.read();
            cfg.commands.iter().find(|c| c.name == name || c.aliases.contains(&name)).cloned()
        };
        let Some(cmd) = cmd else { return };
        if !cmd.enabled {
            return;
        }
        if !Self::has_permission(msg, &cmd.permissions) {
            tracing::info!(target: "signorebot::commands", "!{}: у {} нет прав", cmd.name, msg.user_name);
            return;
        }
        if cmd.cooldown_sec > 0 {
            let mut cd = self.cooldowns.lock();
            let now = Instant::now();
            if let Some(t) = cd.get(&cmd.id) {
                let left = Duration::from_secs(cmd.cooldown_sec as u64).saturating_sub(now.duration_since(*t));
                if !left.is_zero() {
                    tracing::info!(target: "signorebot::commands", "!{}: кулдаун, ещё {} с", cmd.name, left.as_secs());
                    return;
                }
            }
            cd.insert(cmd.id.clone(), now);
        }
        if cmd.cooldown_user_sec > 0 {
            let mut cd = self.user_cooldowns.lock();
            let now = Instant::now();
            let ttl = Duration::from_secs(cmd.cooldown_user_sec as u64);
            cd.retain(|_, t| now.duration_since(*t) < ttl.max(Duration::from_secs(3600)));
            let key = format!("{}:{}", cmd.id, msg.user_login);
            if let Some(t) = cd.get(&key) {
                let left = ttl.saturating_sub(now.duration_since(*t));
                if !left.is_zero() {
                    tracing::info!(target: "signorebot::commands", "!{}: кулдаун для {}, ещё {} с", cmd.name, msg.user_name, left.as_secs());
                    return;
                }
            }
            cd.insert(key, now);
        }
        tracing::info!(target: "signorebot::commands", "!{} от {}{}", cmd.name, msg.user_name,
            if args.is_empty() { String::new() } else { format!(": {}", args.join(" ")) });

        let mut vars = BTreeMap::new();
        vars.insert("user".into(), msg.user_name.clone());
        vars.insert("message".into(), args.join(" "));
        let target = args.first().map(|t| t.trim_start_matches('@').to_string());
        let ctx = ActionCtx {
            author: msg.user_name.clone(),
            target,
            vars,
            label: format!("!{}", cmd.name),
            antispam_user: Some(msg.user_login.clone()),
            reply_to: if cmd.reply { Some(msg.message_id.clone()) } else { None },
        };
        self.execute(&cmd.response, &ctx).await;
    }

    // ------------------------------------------------------------------
    // Награды
    // ------------------------------------------------------------------

    fn reward_fingerprint(reward_id: &str, user: &str, input: &str) -> String {
        let norm = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase();
        format!("{reward_id}:{}:{}", norm(user), norm(input))
    }

    fn reward_is_duplicate(&self, reward_id: &str, user: &str, input: &str, source: RewardSource, event_id: Option<&str>) -> RewardDup {
        let mut d = self.rewards.lock();
        let now = Instant::now();
        d.by_event.retain(|_, t| now.duration_since(*t) < REWARD_EVENT_TTL);
        d.by_fingerprint.retain(|_, (t, _, _)| now.duration_since(*t) < REWARD_EVENT_TTL);
        if let Some(id) = event_id {
            if d.by_event.contains_key(id) {
                return RewardDup::SameEvent;
            }
            d.by_event.insert(id.to_string(), now);
        }
        let fp = Self::reward_fingerprint(reward_id, user, input);
        if let Some((t, sources, outcome)) = d.by_fingerprint.get_mut(&fp) {
            if now.duration_since(*t) < REWARD_CROSS_SOURCE_TTL && !sources.contains(&source) {
                sources.insert(source);
                return RewardDup::CrossSource { via: if source == RewardSource::Chat { "EventSub" } else { "чат" }, outcome: outcome.clone() };
            }
        }
        d.by_fingerprint.insert(fp, (now, HashSet::from([source]), None));
        RewardDup::No
    }

    fn store_reward_outcome(&self, reward_id: &str, user: &str, input: &str, outcome: &RewardOutcome) {
        let fp = Self::reward_fingerprint(reward_id, user, input);
        if let Some(e) = self.rewards.lock().by_fingerprint.get_mut(&fp) {
            e.2 = Some(outcome.clone());
        }
    }

    pub async fn handle_reward(&self, reward_id: &str, user: &str, input: &str, source: RewardSource, event_id: Option<&str>) {
        match self.reward_is_duplicate(reward_id, user, input, source, event_id) {
            RewardDup::No => {}
            RewardDup::SameEvent => {
                tracing::debug!(target: "signorebot::rewards", "Награда {reward_id} от {user} — дубликат (redemption id уже обработан)");
                return;
            }
            RewardDup::CrossSource { via, outcome } => {
                // Чат опередил EventSub: реакция уже выполнена, но учёт погашения
                // (список невыполненных, возврат баллов, закрытие) возможен только
                // с id погашения — довершаем его здесь.
                if let (RewardSource::EventSub, Some(id)) = (source, event_id) {
                    let reward = self.config.read().rewards.iter().find(|r| r.reward_id == reward_id).cloned();
                    match (reward, outcome) {
                        (Some(reward), Some(out)) => {
                            tracing::debug!(target: "signorebot::rewards", "Награда «{}» от {user}: реакция уже выполнена по чату, довершаем учёт погашения", reward.reward_title);
                            self.finish_redemption(&reward, reward_id, id, user, &out).await;
                        }
                        (Some(reward), None) => tracing::warn!(target: "signorebot::rewards", "Награда «{}» от {user}: погашение без итога реакции — учёт пропущен", reward.reward_title),
                        (None, _) => {}
                    }
                } else {
                    tracing::debug!(target: "signorebot::rewards", "Награда {reward_id} от {user} — дубликат (уже обработано через {via})");
                }
                return;
            }
        }
        let reward = self.config.read().rewards.iter().find(|r| r.reward_id == reward_id).cloned();
        let Some(reward) = reward else {
            tracing::info!(target: "signorebot::rewards", "Награда {reward_id} от {user} — реакция не настроена");
            return;
        };
        if !reward.enabled {
            tracing::info!(target: "signorebot::rewards", "Награда «{}» отключена", reward.reward_title);
            return;
        }
        tracing::info!(target: "signorebot::rewards", "Награда «{}» от {user}{}", reward.reward_title,
            if input.is_empty() { String::new() } else { format!(": «{input}»") });
        let mut vars = BTreeMap::new();
        vars.insert("user".into(), user.to_string());
        vars.insert("message".into(), input.to_string());
        let ctx = ActionCtx {
            author: user.to_string(),
            target: if input.is_empty() { None } else { Some(input.to_string()) },
            vars,
            label: format!("Награда: {}", reward.reward_title),
            antispam_user: Some(user.to_lowercase()),
            reply_to: None,
        };
        // Награда с возвратом: медиа для выключенного оверлея не откладываем —
        // иначе зритель получит и баллы назад, и медиа через полминуты. Это
        // верно и для чат-пути: id погашения принесёт EventSub следом.
        let manages = reward.managed && reward.refund_if_unavailable;
        let out = self.execute_full_opt(&reward.response, &ctx, !manages).await;
        let outcome = RewardOutcome { media_unavailable: out.media_unavailable, unavailable_overlays: out.unavailable_overlays.clone(), media_sent: out.media_sent };
        self.store_reward_outcome(reward_id, user, input, &outcome);
        let Some(redemption_id) = event_id else { return }; // чат-путь: id погашения нет
        self.finish_redemption(&reward, reward_id, redemption_id, user, &outcome).await;
    }

    /// Учёт погашения после выполнения реакции: запись в невыполненные,
    /// возврат баллов (для наград с возвратом) или закрытие удачного погашения.
    async fn finish_redemption(&self, reward: &crate::config::Reward, reward_id: &str, redemption_id: &str, user: &str, out: &RewardOutcome) {
        let manages = reward.managed && reward.refund_if_unavailable;
        if out.media_unavailable {
            let reason = format!("оверлей недоступен: {}", out.unavailable_overlays.join(", "));
            let mut entry = PendingRedemption {
                redemption_id: redemption_id.to_string(),
                reward_id: reward_id.to_string(),
                reward_title: reward.reward_title.clone(),
                user: user.to_string(),
                at: chrono::Utc::now().timestamp_millis(),
                reason,
                status: "pending".into(),
            };
            if manages {
                match self.refund(reward_id, redemption_id).await {
                    Ok(()) => {
                        entry.status = "refunded".into();
                        tracing::info!(target: "signorebot::rewards", "Баллы за «{}» возвращены {user}: оверлей был недоступен", reward.reward_title);
                    }
                    Err(e) => tracing::warn!(target: "signorebot::rewards", "Не удалось вернуть баллы за «{}» {user}: {e}", reward.reward_title),
                }
            } else {
                tracing::warn!(target: "signorebot::rewards", "Награда «{}» от {user} не выполнена ({}). Вернуть баллы можно в очереди запросов Twitch", reward.reward_title, entry.reason);
            }
            self.push_redemption(entry);
        } else if manages && (out.media_sent || !reward.response.media.enabled) {
            // Бот ведёт очередь запросов этой награды: удачные погашения закрываем сами.
            if let Some(ids) = self.ids() {
                if let Err(e) = self.helix.update_redemption_status(AccountKind::Broadcaster, &ids.broadcaster_id, reward_id, redemption_id, "FULFILLED").await {
                    tracing::warn!(target: "signorebot::rewards", "Не удалось пометить погашение выполненным: {e}");
                }
            }
        }
    }

    /// Вернуть баллы за погашение (CANCELED). Нужно право `channel:manage:redemptions`
    /// у стримера и награда, созданная нашим приложением.
    pub async fn refund(&self, reward_id: &str, redemption_id: &str) -> Result<(), String> {
        let ids = self.ids().ok_or("бот не запущен")?;
        if !self.auth.has_scope(AccountKind::Broadcaster, "channel:manage:redemptions") {
            return Err("у стримера нет права channel:manage:redemptions — авторизуйте стримера заново".into());
        }
        self.helix.update_redemption_status(AccountKind::Broadcaster, &ids.broadcaster_id, reward_id, redemption_id, "CANCELED").await.map_err(|e| e.to_string())
    }

    // ------------------------------------------------------------------
    // События Twitch
    // ------------------------------------------------------------------

    pub async fn handle_event(&self, event_type: &str, mut vars: BTreeMap<String, String>) {
        let reaction = self.config.read().events.get(event_type).cloned();
        let Some(reaction) = reaction else {
            tracing::info!(target: "signorebot::events", "Событие «{event_type}» не настроено");
            return;
        };
        if !reaction.enabled {
            tracing::info!(target: "signorebot::events", "Событие «{event_type}» отключено");
            return;
        }
        if event_type == "subscribe" && reaction.skip_gifted && vars.get("isGift").map(|v| v == "true").unwrap_or(false) {
            tracing::info!(target: "signorebot::events", "Подарочная подписка — пропуск (о ней сообщит giftSub)");
            return;
        }
        let user = vars.get("user").filter(|u| !u.trim().is_empty()).cloned().unwrap_or_else(|| "someone".into());
        vars.insert("user".into(), user.clone());
        vars.entry("username".into()).or_insert_with(|| user.clone());
        tracing::info!(target: "signorebot::events", "Событие «{event_type}»: {}", serde_json::to_string(&vars).unwrap_or_default());
        let ctx = ActionCtx {
            author: user.clone(),
            target: None,
            vars,
            label: format!("Событие: {event_type}"),
            antispam_user: None,
            reply_to: None,
        };
        self.execute(&reaction.response, &ctx).await;
    }

    /// Главный диспетчер событий Twitch.
    pub async fn dispatch(&self, ev: TwitchEvent) {
        fn v(pairs: &[(&str, String)]) -> BTreeMap<String, String> {
            pairs.iter().map(|(k, val)| (k.to_string(), val.clone())).collect()
        }
        match ev {
            TwitchEvent::Chat(m) => self.on_chat(m).await,
            TwitchEvent::Redemption { redemption_id, reward_id, reward_title, user_name, user_input } => {
                tracing::debug!(target: "signorebot::rewards", "EventSub: «{reward_title}» от {user_name}");
                self.handle_reward(&reward_id, &user_name, &user_input, RewardSource::EventSub, Some(&redemption_id)).await;
            }
            TwitchEvent::RedemptionUpdate { redemption_id, status, .. } => self.on_redemption_update(&redemption_id, &status),
            TwitchEvent::RewardChanged { reward_id, title, removed } => {
                let configured = self.config.read().rewards.iter().any(|r| r.reward_id == reward_id);
                if removed {
                    if configured {
                        tracing::warn!(target: "signorebot::rewards", "Награда «{title}» удалена на Twitch; реакция в SignoreBot сохранена — на вкладке «Баллы канала» она помечена «нет на канале»");
                    }
                    // удалён оригинал управляемой копии → копия получает прежнее название
                    let copy = self.config.read().rewards.iter().find(|r| r.original_reward_id.as_deref() == Some(reward_id.as_str())).cloned();
                    if let (Some(copy), Some(ids)) = (copy, self.ids()) {
                        let new_title = copy.reward_title.trim_end_matches(" (бот)").to_string();
                        match self.helix.update_custom_reward_title(AccountKind::Broadcaster, &ids.broadcaster_id, &copy.reward_id, &new_title).await {
                            Ok(()) => {
                                if let Some(r) = self.config.write().rewards.iter_mut().find(|r| r.id == copy.id) {
                                    r.reward_title = new_title.clone();
                                    r.original_reward_id = None;
                                }
                                tracing::info!(target: "signorebot::rewards", "Оригинал «{title}» удалён — копия переименована в «{new_title}»");
                            }
                            Err(e) => tracing::warn!(target: "signorebot::rewards", "Оригинал «{title}» удалён, но переименовать копию не удалось: {e}. Нажмите «Убрать пометку» на вкладке «Баллы канала»"),
                        }
                    }
                } else {
                    // название могли поменять в панели Twitch — держим своё в актуальном состоянии
                    let mut renamed = false;
                    if let Some(r) = self.config.write().rewards.iter_mut().find(|r| r.reward_id == reward_id) {
                        if r.reward_title != title {
                            r.reward_title = title.clone();
                            renamed = true;
                        }
                    }
                    if renamed {
                        tracing::info!(target: "signorebot::rewards", "Награда переименована на Twitch: теперь «{title}»");
                    } else {
                        tracing::debug!(target: "signorebot::rewards", "Награда «{title}» изменена на Twitch");
                    }
                }
                self.changed(Changed::Rewards);
            }
            TwitchEvent::Follow { user_name, user_id } => {
                tracing::info!(target: "signorebot::events", "Новый фолловер: {user_name}");
                self.handle_event("follow", v(&[("user", user_name), ("userId", user_id)])).await;
            }
            TwitchEvent::Subscribe { user_name, user_id, tier, is_gift } => {
                tracing::info!(target: "signorebot::events", "Новая подписка: {user_name} ({tier}{})", if is_gift { ", подарок" } else { "" });
                self.handle_event("subscribe", v(&[("user", user_name), ("userId", user_id), ("tier", tier), ("isGift", is_gift.to_string())])).await;
            }
            TwitchEvent::Resub { user_name, tier, months, streak_months, message } => {
                tracing::info!(target: "signorebot::events", "Переподписка: {user_name} ({tier}, {months} мес.)");
                self.handle_event(
                    "resubscribe",
                    v(&[("user", user_name), ("tier", tier), ("months", months.to_string()), ("streakMonths", streak_months.to_string()), ("message", message)]),
                )
                .await;
            }
            TwitchEvent::GiftSub { user_name, tier, total, is_anonymous } => {
                tracing::info!(target: "signorebot::events", "Подарочные подписки: {user_name} подарил {total} × {tier}");
                self.handle_event("giftSub", v(&[("user", user_name), ("tier", tier), ("total", total.to_string()), ("isAnonymous", is_anonymous.to_string())])).await;
            }
            TwitchEvent::Cheer { user_name, user_id, bits, message, is_anonymous } => {
                tracing::info!(target: "signorebot::events", "Bits: {user_name} отправил {bits}{}", if message.is_empty() { String::new() } else { format!(": «{message}»") });
                self.handle_event(
                    "bits",
                    v(&[("user", user_name), ("userId", user_id), ("bits", bits.to_string()), ("message", message), ("isAnonymous", is_anonymous.to_string())]),
                )
                .await;
            }
            TwitchEvent::Raid { from_name, from_id, viewers } => {
                tracing::info!(target: "signorebot::events", "Рейд от {from_name} с {viewers} зрителями");
                {
                    let cfg = self.config.read();
                    let (mode, list) = (cfg.shoutout.raid_mode, cfg.shoutout.auto_list.clone());
                    drop(cfg);
                    if self.shoutout.enqueue_raid(&from_name, mode, &list) {
                        self.changed(Changed::Shoutout);
                    }
                }
                self.handle_event("raid", v(&[("user", from_name), ("fromUserId", from_id), ("viewers", viewers.to_string())])).await;
            }
            TwitchEvent::WatchStreak { user_name, user_id, streak_count, points, system_message, message } => {
                tracing::info!(target: "signorebot::events", "Watch streak: {user_name} — {streak_count} стримов подряд");
                self.handle_event(
                    "watchStreak",
                    v(&[
                        ("user", user_name),
                        ("userId", user_id),
                        ("streakCount", streak_count.to_string()),
                        ("channelPointsAwarded", points.to_string()),
                        ("systemMessage", system_message),
                        ("message", message),
                    ]),
                )
                .await;
            }
            TwitchEvent::Session { connected, session_id, subscriptions } => {
                *self.eventsub.lock() = EventSubStatus { connected, session_id, subscriptions };
                self.changed(Changed::EventSub);
            }
        }
    }

    // ------------------------------------------------------------------
    // Shoutout — отправка
    // ------------------------------------------------------------------

    /// Один проход рабочего цикла shoutout: отправить голову очереди.
    /// Возвращает `false`, если очередь пуста.
    pub async fn shoutout_step(&self) -> bool {
        let Some((item, wait)) = self.shoutout.next_ready() else {
            self.shoutout.set_idle();
            return false;
        };
        if !wait.is_zero() {
            tracing::info!(target: "signorebot::shoutout", "Шатаут: ожидание {} с (кулдаун)", wait.as_secs() + 1);
            tokio::time::sleep(wait + Duration::from_millis(500)).await;
        }
        if !self.shoutout.begin(item.id) {
            return true; // голова изменилась — повторим
        }
        self.changed(Changed::Shoutout);
        let cooldown = Duration::from_secs(self.config.read().shoutout.cooldown_sec as u64);
        let outcome = self.send_shoutout(&item.username).await;
        self.shoutout.finish(item.id, outcome, cooldown);
        self.changed(Changed::Shoutout);
        true
    }

    async fn send_shoutout(&self, username: &str) -> shoutout::SendOutcome {
        use shoutout::SendOutcome;
        let Some(ids) = self.ids() else {
            tracing::error!(target: "signorebot::shoutout", "Шатаут: аккаунты не готовы");
            return SendOutcome::Fail;
        };
        tracing::info!(target: "signorebot::shoutout", "Шатаут: отправка для {username}…");
        let to = match self.helix.user_by_login(AccountKind::Broadcaster, username).await {
            Ok(Some(u)) => u.id,
            Ok(None) => {
                tracing::error!(target: "signorebot::shoutout", "Шатаут: пользователь {username} не найден на Twitch");
                return SendOutcome::Fail;
            }
            Err(e) => {
                tracing::warn!(target: "signorebot::shoutout", "Шатаут: не удалось получить id {username}: {e}");
                return SendOutcome::Retry(Duration::from_secs(30));
            }
        };
        if to == ids.broadcaster_id {
            tracing::warn!(target: "signorebot::shoutout", "Шатаут самому себе невозможен — пропуск");
            return SendOutcome::Fail;
        }
        match self.helix.shoutout(AccountKind::Broadcaster, &ids.broadcaster_id, &to, &ids.broadcaster_id).await {
            Ok(()) => SendOutcome::Ok,
            Err(HelixError::Http { status, message, retry_after }) => Self::classify_shoutout_error(username, status, &message, retry_after),
            Err(e) => {
                tracing::warn!(target: "signorebot::shoutout", "Шатаут: {e}");
                SendOutcome::Retry(Duration::from_secs(30))
            }
        }
    }

    /// Ответы Twitch на POST chat/shoutouts (по документации Helix):
    /// 429 «same broadcaster … within 60 minutes» — уже сделан (например,
    /// вручную модератором) → считаем выполненным и НЕ держим очередь;
    /// 429 «another shoutout for two minutes» — ждём; 400 «not streaming live»
    /// и прочие 4xx — не повторяем; 5xx — повтор.
    pub fn classify_shoutout_error(username: &str, status: u16, message: &str, retry_after: Option<u64>) -> shoutout::SendOutcome {
        use shoutout::SendOutcome;
        let m = message.to_lowercase();
        match status {
            429 if m.contains("60 minutes") || m.contains("same broadcaster") || m.contains("already") => {
                tracing::info!(target: "signorebot::shoutout", "Twitch: {username} уже получал шатаут в течение часа (вероятно, вручную) — считаем выполненным");
                SendOutcome::AlreadyDone
            }
            429 => {
                let wait = retry_after.unwrap_or(30).clamp(5, 130);
                tracing::info!(target: "signorebot::shoutout", "Twitch: кулдаун шатаутов, повтор через {wait} с");
                SendOutcome::Retry(Duration::from_secs(wait))
            }
            s if s >= 500 => {
                tracing::warn!(target: "signorebot::shoutout", "Twitch {s}: {message}");
                SendOutcome::Retry(Duration::from_secs(30))
            }
            403 => {
                tracing::error!(target: "signorebot::shoutout", "Шатаут для {username} отклонён (403): {message} — нужны права moderator:manage:shoutouts у стримера");
                SendOutcome::Fail
            }
            _ => {
                tracing::warn!(target: "signorebot::shoutout", "Шатаут для {username} отклонён ({status}): {message}");
                SendOutcome::Fail
            }
        }
    }

    // ------------------------------------------------------------------
    // Тестовые вызовы из UI
    // ------------------------------------------------------------------

    pub fn test_event_vars(event_type: &str) -> BTreeMap<String, String> {
        let pairs: &[(&str, &str)] = match event_type {
            "follow" => &[("user", "TestUser"), ("userId", "12345")],
            "subscribe" => &[("user", "TestUser"), ("userId", "12345"), ("tier", "Tier 1"), ("isGift", "false")],
            "resubscribe" => &[("user", "TestUser"), ("tier", "Tier 1"), ("months", "6"), ("streakMonths", "3"), ("message", "Привет! Уже 6 месяцев!")],
            "giftSub" => &[("user", "TestGifter"), ("tier", "Tier 1"), ("total", "5"), ("isAnonymous", "false")],
            "bits" => &[("user", "TestCheerer"), ("userId", "12345"), ("bits", "250"), ("message", "Держи битсы!"), ("isAnonymous", "false")],
            "raid" => &[("user", "TestRaider"), ("fromUserId", "12345"), ("viewers", "42")],
            "watchStreak" => &[
                ("user", "TestStreaker"),
                ("userId", "12345"),
                ("streakCount", "120"),
                ("channelPointsAwarded", "450"),
                ("systemMessage", "TestStreaker watched 120 consecutive streams and sparked a watch streak!"),
                ("message", ""),
            ],
            _ => &[("user", "TestUser")],
        };
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    pub fn config_snapshot(&self) -> Config {
        self.config.read().clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(login: &str) -> ChatMessage {
        ChatMessage {
            message_id: "m".into(),
            user_id: "1".into(),
            user_login: login.into(),
            user_name: login.into(),
            text: String::new(),
            is_broadcaster: false,
            is_moderator: false,
            is_vip: false,
            is_subscriber: false,
            reward_id: None,
        }
    }

    #[test]
    fn shoutout_errors_are_classified() {
        use shoutout::SendOutcome;
        let c = |st, m: &str| Engine::classify_shoutout_error("x", st, m, Some(7));
        assert!(matches!(c(429, "The broadcaster may not give the same broadcaster a shoutout more than once within 60 minutes."), SendOutcome::AlreadyDone));
        assert!(matches!(c(429, "The broadcaster may not give another shoutout for two minutes."), SendOutcome::Retry(d) if d.as_secs() == 7));
        assert!(matches!(c(400, "The broadcaster is not streaming live or does not have one or more viewers."), SendOutcome::Fail));
        assert!(matches!(c(403, "not a moderator"), SendOutcome::Fail));
        assert!(matches!(c(503, "oops"), SendOutcome::Retry(_)));
    }

    #[test]
    fn permissions() {
        let mut m = msg("alice");
        assert!(Engine::has_permission(&m, &[]));
        assert!(Engine::has_permission(&m, &["everyone".into()]));
        assert!(!Engine::has_permission(&m, &["moderators".into()]));
        assert!(Engine::has_permission(&m, &["user:Alice".into()]));
        m.is_broadcaster = true;
        assert!(Engine::has_permission(&m, &["moderators".into()]));
        assert!(Engine::has_permission(&m, &["broadcaster".into()]));
    }
}
