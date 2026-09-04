//! EventSub WebSocket: одна сессия под токеном стримера получает чат
//! (`channel.chat.message`) и все события канала. IRC не используется.

use super::accounts::AuthManager;
use super::helix::Helix;
use crate::secrets::AccountKind;
use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;
use tokio_util::sync::CancellationToken;

pub const EVENTSUB_URL: &str = "wss://eventsub.wss.twitch.tv/ws?keepalive_timeout_seconds=30";
const RECONNECT_DELAY: Duration = Duration::from_secs(10);
const DEDUP_TTL: Duration = Duration::from_secs(600);
const DEDUP_MAX: usize = 2000;

#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub message_id: String,
    pub user_id: String,
    pub user_login: String,
    pub user_name: String,
    pub text: String,
    pub is_broadcaster: bool,
    pub is_moderator: bool,
    pub is_vip: bool,
    pub is_subscriber: bool,
    /// Награда за баллы с текстом (id награды).
    pub reward_id: Option<String>,
}

#[derive(Debug, Clone)]
pub enum TwitchEvent {
    Chat(ChatMessage),
    Redemption { redemption_id: String, reward_id: String, reward_title: String, user_name: String, user_input: String },
    /// Погашение закрыто (стример/модератор/бот): status = fulfilled | canceled.
    RedemptionUpdate { redemption_id: String, reward_id: String, status: String },
    /// Награда на канале создана/изменена/удалена — панели пора перечитать список.
    RewardChanged { reward_id: String, title: String, removed: bool },
    Follow { user_name: String, user_id: String },
    Subscribe { user_name: String, user_id: String, tier: String, is_gift: bool },
    Resub { user_name: String, tier: String, months: u64, streak_months: u64, message: String },
    GiftSub { user_name: String, tier: String, total: u64, is_anonymous: bool },
    Cheer { user_name: String, user_id: String, bits: u64, message: String, is_anonymous: bool },
    Raid { from_name: String, from_id: String, viewers: u64 },
    WatchStreak { user_name: String, user_id: String, streak_count: u64, points: u64, system_message: String, message: String },
    /// Состояние сессии для UI.
    Session { connected: bool, session_id: Option<String>, subscriptions: usize },
}

pub struct SessionParams {
    pub helix: Arc<Helix>,
    pub auth: Arc<AuthManager>,
    pub broadcaster_id: String,
    pub tx: mpsc::Sender<TwitchEvent>,
    pub cancel: CancellationToken,
}

fn tier_name(t: &str) -> String {
    match t {
        "1000" => "Tier 1".into(),
        "2000" => "Tier 2".into(),
        "3000" => "Tier 3".into(),
        other => other.to_string(),
    }
}

fn s(v: &Value, k: &str) -> String {
    v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string()
}
/// Отображаемое имя, а если его нет — логин.
fn name_or_login(v: &Value, name_key: &str, login_key: &str) -> String {
    let n = s(v, name_key);
    if n.trim().is_empty() { s(v, login_key) } else { n }
}
fn u(v: &Value, k: &str) -> u64 {
    v.get(k).and_then(|x| x.as_u64()).unwrap_or(0)
}
fn b(v: &Value, k: &str) -> bool {
    v.get(k).and_then(|x| x.as_bool()).unwrap_or(false)
}

/// Разобрать уведомление в событие.
pub fn parse_notification(sub_type: &str, ev: &Value) -> Option<TwitchEvent> {
    Some(match sub_type {
        "channel.chat.message" => {
            let mut is_broadcaster = false;
            let mut is_moderator = false;
            let mut is_vip = false;
            let mut is_subscriber = false;
            if let Some(badges) = ev.get("badges").and_then(|x| x.as_array()) {
                for bd in badges {
                    match s(bd, "set_id").as_str() {
                        "broadcaster" => is_broadcaster = true,
                        "moderator" => is_moderator = true,
                        "vip" => is_vip = true,
                        "subscriber" | "founder" => is_subscriber = true,
                        _ => {}
                    }
                }
            }
            let reward_id = ev.get("channel_points_custom_reward_id").and_then(|x| x.as_str()).filter(|x| !x.is_empty()).map(String::from);
            TwitchEvent::Chat(ChatMessage {
                message_id: s(ev, "message_id"),
                user_id: s(ev, "chatter_user_id"),
                user_login: s(ev, "chatter_user_login"),
                user_name: name_or_login(ev, "chatter_user_name", "chatter_user_login"),
                text: ev.get("message").map(|m| s(m, "text")).unwrap_or_default(),
                is_broadcaster,
                is_moderator,
                is_vip,
                is_subscriber,
                reward_id,
            })
        }
        "channel.channel_points_custom_reward_redemption.update" => {
            let reward = ev.get("reward").cloned().unwrap_or(Value::Null);
            TwitchEvent::RedemptionUpdate { redemption_id: s(ev, "id"), reward_id: s(&reward, "id"), status: s(ev, "status").to_lowercase() }
        }
        "channel.channel_points_custom_reward.add" | "channel.channel_points_custom_reward.update" | "channel.channel_points_custom_reward.remove" => {
            TwitchEvent::RewardChanged { reward_id: s(ev, "id"), title: s(ev, "title"), removed: sub_type == "channel.channel_points_custom_reward.remove" }
        }
        "channel.channel_points_custom_reward_redemption.add" => {
            let reward = ev.get("reward").cloned().unwrap_or(Value::Null);
            TwitchEvent::Redemption {
                redemption_id: s(ev, "id"),
                reward_id: s(&reward, "id"),
                reward_title: s(&reward, "title"),
                user_name: name_or_login(ev, "user_name", "user_login"),
                user_input: s(ev, "user_input"),
            }
        }
        "channel.follow" => TwitchEvent::Follow { user_name: name_or_login(ev, "user_name", "user_login"), user_id: s(ev, "user_id") },
        "channel.subscribe" => TwitchEvent::Subscribe {
            user_name: s(ev, "user_name"),
            user_id: s(ev, "user_id"),
            tier: tier_name(&s(ev, "tier")),
            is_gift: b(ev, "is_gift"),
        },
        "channel.subscription.message" => TwitchEvent::Resub {
            user_name: s(ev, "user_name"),
            tier: tier_name(&s(ev, "tier")),
            months: u(ev, "cumulative_months").max(1),
            streak_months: u(ev, "streak_months"),
            message: ev.get("message").map(|m| s(m, "text")).unwrap_or_default(),
        },
        "channel.subscription.gift" => {
            let anon = b(ev, "is_anonymous");
            let name = s(ev, "user_name");
            TwitchEvent::GiftSub {
                user_name: if anon || name.is_empty() { "Аноним".into() } else { name },
                tier: tier_name(&s(ev, "tier")),
                total: u(ev, "total"),
                is_anonymous: anon,
            }
        }
        "channel.cheer" => {
            let anon = b(ev, "is_anonymous");
            let name = s(ev, "user_name");
            TwitchEvent::Cheer {
                user_name: if anon { "Аноним".into() } else if name.is_empty() { "Someone".into() } else { name },
                user_id: s(ev, "user_id"),
                bits: u(ev, "bits"),
                message: s(ev, "message"),
                is_anonymous: anon,
            }
        }
        "channel.raid" => TwitchEvent::Raid {
            from_name: s(ev, "from_broadcaster_user_name"),
            from_id: s(ev, "from_broadcaster_user_id"),
            viewers: u(ev, "viewers"),
        },
        "channel.chat.notification" => {
            if s(ev, "notice_type") != "watch_streak" {
                return None;
            }
            let ws = ev.get("watch_streak").cloned().unwrap_or(Value::Null);
            let name = s(ev, "chatter_user_name");
            TwitchEvent::WatchStreak {
                user_name: if name.is_empty() { s(ev, "chatter_user_login") } else { name },
                user_id: s(ev, "chatter_user_id"),
                streak_count: u(&ws, "streak_count"),
                points: u(&ws, "channel_points_awarded"),
                system_message: s(ev, "system_message"),
                message: ev.get("message").map(|m| s(m, "text")).unwrap_or_default(),
            }
        }
        _ => return None,
    })
}

struct Deduper {
    seen: VecDeque<(String, Instant)>,
}
impl Deduper {
    fn new() -> Self {
        Self { seen: VecDeque::new() }
    }
    fn is_dup(&mut self, id: &str) -> bool {
        let now = Instant::now();
        while let Some((_, t)) = self.seen.front() {
            if now.duration_since(*t) > DEDUP_TTL {
                self.seen.pop_front();
            } else {
                break;
            }
        }
        if self.seen.iter().any(|(x, _)| x == id) {
            return true;
        }
        self.seen.push_back((id.to_string(), now));
        while self.seen.len() > DEDUP_MAX {
            self.seen.pop_front();
        }
        false
    }
}

type WsStream = tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

struct Welcome {
    ws: WsStream,
    session_id: String,
    keepalive: Duration,
    /// Уведомления, пришедшие до welcome (не бывает, но на всякий случай).
    pending: Vec<Value>,
}

async fn connect_and_wait_welcome(url: &str) -> anyhow::Result<Welcome> {
    let (mut ws, _) = tokio::time::timeout(Duration::from_secs(15), tokio_tungstenite::connect_async(url)).await??;
    let mut pending = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        let msg = tokio::time::timeout(remaining, ws.next()).await;
        let Ok(Some(msg)) = msg else { anyhow::bail!("welcome не получен") };
        let msg = msg?;
        let text = match msg {
            Message::Text(t) => t.to_string(),
            Message::Ping(p) => {
                let _ = ws.send(Message::Pong(p)).await;
                continue;
            }
            Message::Close(c) => anyhow::bail!("соединение закрыто до welcome: {c:?}"),
            _ => continue,
        };
        let v: Value = serde_json::from_str(&text)?;
        let kind = v.pointer("/metadata/message_type").and_then(|x| x.as_str()).unwrap_or("");
        if kind == "session_welcome" {
            let session_id = v.pointer("/payload/session/id").and_then(|x| x.as_str()).unwrap_or("").to_string();
            let ka = v.pointer("/payload/session/keepalive_timeout_seconds").and_then(|x| x.as_u64()).unwrap_or(10);
            return Ok(Welcome { ws, session_id, keepalive: Duration::from_secs(ka), pending });
        }
        if kind == "notification" {
            pending.push(v);
        }
    }
}

struct SubSpec {
    kind: &'static str,
    version: &'static str,
    condition: Value,
}

fn subscriptions(bid: &str) -> Vec<SubSpec> {
    let bc = serde_json::json!({ "broadcaster_user_id": bid });
    vec![
        SubSpec { kind: "channel.chat.message", version: "1", condition: serde_json::json!({ "broadcaster_user_id": bid, "user_id": bid }) },
        SubSpec { kind: "channel.chat.notification", version: "1", condition: serde_json::json!({ "broadcaster_user_id": bid, "user_id": bid }) },
        SubSpec { kind: "channel.channel_points_custom_reward_redemption.add", version: "1", condition: bc.clone() },
        SubSpec { kind: "channel.channel_points_custom_reward_redemption.update", version: "1", condition: bc.clone() },
        SubSpec { kind: "channel.channel_points_custom_reward.add", version: "1", condition: bc.clone() },
        SubSpec { kind: "channel.channel_points_custom_reward.update", version: "1", condition: bc.clone() },
        SubSpec { kind: "channel.channel_points_custom_reward.remove", version: "1", condition: bc.clone() },
        SubSpec { kind: "channel.follow", version: "2", condition: serde_json::json!({ "broadcaster_user_id": bid, "moderator_user_id": bid }) },
        SubSpec { kind: "channel.subscribe", version: "1", condition: bc.clone() },
        SubSpec { kind: "channel.subscription.message", version: "1", condition: bc.clone() },
        SubSpec { kind: "channel.subscription.gift", version: "1", condition: bc.clone() },
        SubSpec { kind: "channel.cheer", version: "1", condition: bc.clone() },
        SubSpec { kind: "channel.raid", version: "1", condition: serde_json::json!({ "to_broadcaster_user_id": bid }) },
    ]
}

async fn subscribe_all(p: &SessionParams, session_id: &str) -> usize {
    let mut ok = 0;
    let mut failed = Vec::new();
    for spec in subscriptions(&p.broadcaster_id) {
        match p.helix.create_eventsub(AccountKind::Broadcaster, spec.kind, spec.version, spec.condition, session_id).await {
            Ok(_) => ok += 1,
            Err(super::helix::HelixError::Http { status: 409, .. }) => ok += 1,
            Err(e) => {
                let hint = match e.status() {
                    Some(403) => " — нет прав, переавторизуйте стримера",
                    Some(401) => " — токен недействителен",
                    _ => "",
                };
                tracing::error!(target: "signorebot::eventsub", "Подписка {}: {e}{hint}", spec.kind);
                failed.push(spec.kind);
            }
        }
    }
    if failed.is_empty() {
        tracing::info!(target: "signorebot::eventsub", "EventSub: {ok} подписок создано");
    } else {
        tracing::warn!(target: "signorebot::eventsub", "EventSub: {ok} подписок создано, не удалось: {}", failed.join(", "));
    }
    ok
}

async fn handle_text(p: &SessionParams, dedup: &mut Deduper, v: &Value) {
    let msg_id = v.pointer("/metadata/message_id").and_then(|x| x.as_str()).unwrap_or("");
    if !msg_id.is_empty() && dedup.is_dup(msg_id) {
        tracing::debug!(target: "signorebot::eventsub", "Повторное уведомление {msg_id} пропущено");
        return;
    }
    let sub_type = v.pointer("/metadata/subscription_type").and_then(|x| x.as_str()).unwrap_or("");
    let ev = v.pointer("/payload/event").cloned().unwrap_or(Value::Null);
    if let Some(e) = parse_notification(sub_type, &ev) {
        let _ = p.tx.send(e).await;
    }
}

/// Главный цикл сессии. Живёт, пока не отменён `cancel`.
pub async fn run_session(p: SessionParams) {
    let mut dedup = Deduper::new();
    let mut url = EVENTSUB_URL.to_string();
    let mut handoff = false;

    'outer: loop {
        if p.cancel.is_cancelled() {
            break;
        }
        tracing::info!(target: "signorebot::eventsub", "Подключение к EventSub…");
        let welcome = tokio::select! {
            _ = p.cancel.cancelled() => break,
            r = connect_and_wait_welcome(&url) => r,
        };
        let Welcome { mut ws, session_id, keepalive, pending } = match welcome {
            Ok(w) => w,
            Err(e) => {
                tracing::warn!(target: "signorebot::eventsub", "EventSub недоступен: {e}. Повтор через {} с", RECONNECT_DELAY.as_secs());
                url = EVENTSUB_URL.to_string();
                handoff = false;
                tokio::select! {
                    _ = p.cancel.cancelled() => break,
                    _ = tokio::time::sleep(RECONNECT_DELAY) => continue,
                }
            }
        };
        tracing::info!(target: "signorebot::eventsub", "EventSub сессия {session_id} (keepalive {} с)", keepalive.as_secs());
        let subs = if handoff { 9 } else { subscribe_all(&p, &session_id).await };
        handoff = false;
        let _ = p.tx.send(TwitchEvent::Session { connected: true, session_id: Some(session_id.clone()), subscriptions: subs }).await;
        for v in pending {
            handle_text(&p, &mut dedup, &v).await;
        }

        let ka_timeout = keepalive + Duration::from_secs(10);
        loop {
            let msg = tokio::select! {
                _ = p.cancel.cancelled() => {
                    let _ = ws.close(None).await;
                    break 'outer;
                }
                m = tokio::time::timeout(ka_timeout, ws.next()) => m,
            };
            let msg = match msg {
                Err(_) => {
                    tracing::warn!(target: "signorebot::eventsub", "EventSub: нет keepalive {} с, переподключение", ka_timeout.as_secs());
                    break;
                }
                Ok(None) => {
                    tracing::warn!(target: "signorebot::eventsub", "EventSub: соединение закрыто");
                    break;
                }
                Ok(Some(Err(e))) => {
                    tracing::warn!(target: "signorebot::eventsub", "EventSub: ошибка сокета: {e}");
                    break;
                }
                Ok(Some(Ok(m))) => m,
            };
            match msg {
                Message::Ping(d) => {
                    let _ = ws.send(Message::Pong(d)).await;
                }
                Message::Close(c) => {
                    tracing::info!(target: "signorebot::eventsub", "EventSub закрыт: {c:?}");
                    break;
                }
                Message::Text(t) => {
                    let Ok(v) = serde_json::from_str::<Value>(&t) else { continue };
                    let kind = v.pointer("/metadata/message_type").and_then(|x| x.as_str()).unwrap_or("");
                    match kind {
                        "session_keepalive" => {}
                        "notification" => handle_text(&p, &mut dedup, &v).await,
                        "session_reconnect" => {
                            let new_url = v.pointer("/payload/session/reconnect_url").and_then(|x| x.as_str()).unwrap_or("").to_string();
                            tracing::info!(target: "signorebot::eventsub", "EventSub просит переподключиться");
                            match connect_and_wait_welcome(&new_url).await {
                                Ok(w) => {
                                    let _ = ws.close(None).await;
                                    ws = w.ws;
                                    tracing::info!(target: "signorebot::eventsub", "EventSub: перенос на новую сессию {}", w.session_id);
                                    let _ = p.tx.send(TwitchEvent::Session { connected: true, session_id: Some(w.session_id), subscriptions: subs }).await;
                                    for pv in w.pending {
                                        handle_text(&p, &mut dedup, &pv).await;
                                    }
                                }
                                Err(e) => {
                                    tracing::warn!(target: "signorebot::eventsub", "Перенос сессии не удался: {e}");
                                    break;
                                }
                            }
                        }
                        "revocation" => {
                            let st = v.pointer("/payload/subscription/type").and_then(|x| x.as_str()).unwrap_or("?");
                            let reason = v.pointer("/payload/subscription/status").and_then(|x| x.as_str()).unwrap_or("?");
                            tracing::warn!(target: "signorebot::eventsub", "Подписка {st} отозвана: {reason}");
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }
        let _ = p.tx.send(TwitchEvent::Session { connected: false, session_id: None, subscriptions: 0 }).await;
        url = EVENTSUB_URL.to_string();
        tokio::select! {
            _ = p.cancel.cancelled() => break,
            _ = tokio::time::sleep(RECONNECT_DELAY) => {}
        }
    }
    let _ = p.tx.send(TwitchEvent::Session { connected: false, session_id: None, subscriptions: 0 }).await;
    tracing::info!(target: "signorebot::eventsub", "EventSub остановлен");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_chat_message() {
        let ev = serde_json::json!({
            "message_id":"m1","chatter_user_id":"1","chatter_user_login":"u","chatter_user_name":"U",
            "message":{"text":"!кусь @x"},"badges":[{"set_id":"moderator","id":"1","info":""},{"set_id":"subscriber","id":"3","info":"3"}],
            "channel_points_custom_reward_id":null
        });
        let TwitchEvent::Chat(c) = parse_notification("channel.chat.message", &ev).unwrap() else { panic!() };
        assert_eq!(c.text, "!кусь @x");
        assert!(c.is_moderator && c.is_subscriber && !c.is_vip);
        assert_eq!(c.reward_id, None);
    }

    #[test]
    fn parses_raid_and_streak() {
        let ev = serde_json::json!({"from_broadcaster_user_name":"R","from_broadcaster_user_id":"9","viewers":42});
        let TwitchEvent::Raid { from_name, viewers, .. } = parse_notification("channel.raid", &ev).unwrap() else { panic!() };
        assert_eq!((from_name.as_str(), viewers), ("R", 42));
        let ev = serde_json::json!({"notice_type":"sub_gift"});
        assert!(parse_notification("channel.chat.notification", &ev).is_none());
        let ev = serde_json::json!({"notice_type":"watch_streak","chatter_user_name":"S","watch_streak":{"streak_count":5,"channel_points_awarded":10},"system_message":"x"});
        let TwitchEvent::WatchStreak { streak_count, .. } = parse_notification("channel.chat.notification", &ev).unwrap() else { panic!() };
        assert_eq!(streak_count, 5);
    }

    #[test]
    fn dedup() {
        let mut d = Deduper::new();
        assert!(!d.is_dup("a"));
        assert!(d.is_dup("a"));
        assert!(!d.is_dup("b"));
    }
}
