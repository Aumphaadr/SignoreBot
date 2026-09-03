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
    by_fingerprint: HashMap<String, (Instant, HashSet<RewardSource>)>,
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
    antispam: Mutex<HashMap<(String, String), Instant>>,
    eventsub: Mutex<EventSubStatus>,
    changed_tx: broadcast::Sender<Changed>,
}

impl Engine {
    pub fn new(config: SharedConfig, auth: Arc<AuthManager>, helix: Arc<Helix>, hub: OverlayHub, deleted_log: std::path::PathBuf) -> Arc<Self> {
        let (changed_tx, _) = broadcast::channel(64);
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
            antispam: Mutex::new(HashMap::new()),
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
        let Some(ids) = self.ids() else {
            tracing::warn!(target: "signorebot::chat", "Чат недоступен (аккаунты не готовы), сообщение не отправлено");
            return false;
        };
        match self.helix.send_chat_message(AccountKind::Bot, &ids.broadcaster_id, &ids.bot_id, text, None).await {
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
                chat_sent = self.say(&text).await;
            }
        }

        if response.media.enabled && !response.media.file.is_empty() {
            media_sent = self.send_media(response, ctx);
        }
        (chat_sent, media_sent)
    }

    fn send_media(&self, response: &Response, ctx: &ActionCtx) -> bool {
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
                        return false;
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
            return false;
        }
        let mut any = false;
        for (name, path) in targets {
            if self.hub.send_to_path(&path, &msg) {
                tracing::info!(target: "signorebot::media", "Медиа: «{}» → оверлей «{name}»", ctx.label);
                any = true;
            } else {
                tracing::warn!(target: "signorebot::media", "Медиа: «{}» → оверлей «{name}» не подключён, поставлено в очередь (30 с)", ctx.label);
            }
        }
        self.changed(Changed::Media);
        any
    }

    // ------------------------------------------------------------------
    // Чат
    // ------------------------------------------------------------------

    pub async fn on_chat(&self, msg: ChatMessage) {
        let ids = self.ids();
        if let Some(ids) = &ids {
            if msg.user_id == ids.bot_id {
                return;
            }
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
        };
        self.execute(&cmd.response, &ctx).await;
    }

    // ------------------------------------------------------------------
    // Награды
    // ------------------------------------------------------------------

    fn reward_is_duplicate(&self, reward_id: &str, user: &str, input: &str, source: RewardSource, event_id: Option<&str>) -> Option<String> {
        let mut d = self.rewards.lock();
        let now = Instant::now();
        d.by_event.retain(|_, t| now.duration_since(*t) < REWARD_EVENT_TTL);
        d.by_fingerprint.retain(|_, (t, _)| now.duration_since(*t) < REWARD_EVENT_TTL);
        if let Some(id) = event_id {
            if d.by_event.contains_key(id) {
                return Some("redemption id уже обработан".into());
            }
            d.by_event.insert(id.to_string(), now);
        }
        let norm = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase();
        let fp = format!("{reward_id}:{}:{}", norm(user), norm(input));
        if let Some((t, sources)) = d.by_fingerprint.get_mut(&fp) {
            if now.duration_since(*t) < REWARD_CROSS_SOURCE_TTL && !sources.contains(&source) {
                sources.insert(source);
                return Some(format!("уже обработано через {}", if source == RewardSource::Chat { "EventSub" } else { "чат" }));
            }
        }
        d.by_fingerprint.insert(fp, (now, HashSet::from([source])));
        None
    }

    pub async fn handle_reward(&self, reward_id: &str, user: &str, input: &str, source: RewardSource, event_id: Option<&str>) {
        if let Some(why) = self.reward_is_duplicate(reward_id, user, input, source, event_id) {
            tracing::debug!(target: "signorebot::rewards", "Награда {reward_id} от {user} — дубликат ({why})");
            return;
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
        };
        self.execute(&reward.response, &ctx).await;
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
