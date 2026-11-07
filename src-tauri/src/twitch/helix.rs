//! Клиент Twitch Helix. Каждый вызов берёт свежий токен у `AuthManager`
//! и один раз повторяет запрос после 401 (с принудительным refresh).

use super::accounts::AuthManager;
use super::auth::AuthError;
use crate::secrets::AccountKind;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::sync::Arc;

pub const HELIX: &str = "https://api.twitch.tv/helix";

#[derive(Debug, thiserror::Error)]
pub enum HelixError {
    #[error("сеть: {0}")]
    Network(#[from] reqwest::Error),
    #[error("авторизация: {0}")]
    Auth(#[from] AuthError),
    /// HTTP-ошибка Twitch: статус, сообщение, Retry-After (с).
    #[error("Twitch {status}: {message}")]
    Http { status: u16, message: String, retry_after: Option<u64> },
}

impl HelixError {
    pub fn status(&self) -> Option<u16> {
        match self {
            HelixError::Http { status, .. } => Some(*status),
            _ => None,
        }
    }
}

#[derive(Debug, Deserialize)]
struct Envelope<T> {
    #[serde(default = "Vec::new")]
    data: Vec<T>,
}

#[derive(Debug, Deserialize)]
struct ErrBody {
    #[serde(default)]
    message: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct User {
    pub id: String,
    pub login: String,
    pub display_name: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Chatter {
    pub user_id: String,
    pub user_login: String,
    pub user_name: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "api.ts")]
pub struct ChannelReward {
    pub id: String,
    pub title: String,
    #[ts(type = "number")]
    pub cost: u64,
    pub is_enabled: bool,
    pub is_paused: bool,
    pub requires_input: bool,
    pub background_color: String,
    pub image: Option<String>,
    pub has_custom_image: bool,
    pub prompt: String,
    // Погашения проходят мимо очереди запросов (сразу FULFILLED) — вернуть баллы нельзя.
    pub skip_queue: bool,
    #[ts(type = "number | null")]
    pub cooldown_seconds: Option<u64>,
    #[ts(type = "number | null")]
    pub max_per_stream: Option<u64>,
    #[ts(type = "number | null")]
    pub max_per_user_per_stream: Option<u64>,
    // Создана нашим приложением — можно менять и возвращать баллы.
    pub is_managed: bool,
}

/// Параметры новой награды (управляемой копии).
/// Параметры награды для Create/Update Custom Reward (те же поля, что в панели Twitch).
#[derive(Debug, Clone, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "api.ts")]
pub struct NewReward {
    pub title: String,
    #[ts(type = "number")]
    pub cost: u64,
    pub prompt: String,
    pub is_user_input_required: bool,
    pub is_enabled: bool,
    pub background_color: String,
    #[ts(type = "number | null")]
    pub cooldown_seconds: Option<u64>,
    #[ts(type = "number | null")]
    pub max_per_stream: Option<u64>,
    #[ts(type = "number | null")]
    pub max_per_user_per_stream: Option<u64>,
    /// Погашения мимо очереди запросов: закрываются сразу, вернуть баллы нельзя.
    #[serde(default)]
    pub skip_queue: bool,
}

impl NewReward {
    fn body(&self) -> serde_json::Value {
        serde_json::json!({
            "title": self.title, "cost": self.cost, "prompt": self.prompt,
            "is_user_input_required": self.is_user_input_required, "is_enabled": self.is_enabled,
            "background_color": if self.background_color.is_empty() { serde_json::Value::Null } else { serde_json::Value::String(self.background_color.to_ascii_uppercase()) },
            "is_global_cooldown_enabled": self.cooldown_seconds.is_some(), "global_cooldown_seconds": self.cooldown_seconds.unwrap_or(0),
            "is_max_per_stream_enabled": self.max_per_stream.is_some(), "max_per_stream": self.max_per_stream.unwrap_or(0),
            "is_max_per_user_per_stream_enabled": self.max_per_user_per_stream.is_some(), "max_per_user_per_stream": self.max_per_user_per_stream.unwrap_or(0),
            "should_redemptions_skip_request_queue": self.skip_queue,
        })
    }
}

#[derive(Debug, Deserialize)]
struct RawReward {
    id: String,
    title: String,
    cost: u64,
    is_enabled: bool,
    is_paused: bool,
    is_user_input_required: bool,
    #[serde(default)]
    background_color: String,
    image: Option<RewardImage>,
    default_image: Option<RewardImage>,
    #[serde(default)]
    prompt: String,
    #[serde(default)]
    should_redemptions_skip_request_queue: bool,
    #[serde(default)]
    global_cooldown_setting: Option<CooldownSetting>,
    #[serde(default)]
    max_per_stream_setting: Option<MaxPerStream>,
    #[serde(default)]
    max_per_user_per_stream_setting: Option<MaxPerUser>,
}
#[derive(Debug, Deserialize, Default)]
struct CooldownSetting { #[serde(default)] is_enabled: bool, #[serde(default)] global_cooldown_seconds: u64 }
#[derive(Debug, Deserialize, Default)]
struct MaxPerStream { #[serde(default)] is_enabled: bool, #[serde(default)] max_per_stream: u64 }
#[derive(Debug, Deserialize, Default)]
struct MaxPerUser { #[serde(default)] is_enabled: bool, #[serde(default)] max_per_user_per_stream: u64 }
fn raw_to_reward(r: RawReward, managed: &std::collections::HashSet<String>) -> ChannelReward {
    let cd = r.global_cooldown_setting.unwrap_or_default();
    let mps = r.max_per_stream_setting.unwrap_or_default();
    let mpu = r.max_per_user_per_stream_setting.unwrap_or_default();
    ChannelReward {
        is_managed: managed.contains(&r.id),
        id: r.id,
        title: r.title,
        cost: r.cost,
        is_enabled: r.is_enabled,
        is_paused: r.is_paused,
        requires_input: r.is_user_input_required,
        background_color: r.background_color,
        has_custom_image: r.image.is_some(),
        image: reward_image(r.image, r.default_image),
        prompt: r.prompt,
        skip_queue: r.should_redemptions_skip_request_queue,
        cooldown_seconds: if cd.is_enabled { Some(cd.global_cooldown_seconds) } else { None },
        max_per_stream: if mps.is_enabled { Some(mps.max_per_stream) } else { None },
        max_per_user_per_stream: if mpu.is_enabled { Some(mpu.max_per_user_per_stream) } else { None },
    }
}

#[derive(Debug, Deserialize)]
struct RewardImage {
    #[serde(default)] url_1x: String,
    #[serde(default)] url_2x: String,
    #[serde(default)] url_4x: String,
}
fn reward_image(image: Option<RewardImage>, default: Option<RewardImage>) -> Option<String> {
    image.into_iter().chain(default).flat_map(|i| [i.url_4x, i.url_2x, i.url_1x]).find(|url| !url.is_empty())
}

#[derive(Debug, Deserialize)]
pub struct SendMessageResult {
    pub message_id: String,
    pub is_sent: bool,
    #[serde(default)]
    pub drop_reason: Option<DropReason>,
}
#[derive(Debug, Deserialize)]
pub struct DropReason {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Deserialize)]
pub struct EventSubCreated {
    pub id: String,
    pub status: String,
}

pub struct Helix {
    http: reqwest::Client,
    auth: Arc<AuthManager>,
}

enum Body<'a> {
    None,
    Json(&'a serde_json::Value),
}

impl Helix {
    pub fn new(auth: Arc<AuthManager>) -> Self {
        let http = reqwest::Client::builder()
            .user_agent("SignoreBot/0.1")
            .timeout(std::time::Duration::from_secs(20))
            .build()
            .expect("reqwest client");
        Self { http, auth }
    }

    async fn request(
        &self,
        kind: AccountKind,
        method: reqwest::Method,
        url: &str,
        query: &[(&str, &str)],
        body: Body<'_>,
    ) -> Result<reqwest::Response, HelixError> {
        let mut token = self.auth.access_token(kind).await?;
        for attempt in 0..2 {
            let mut req = self
                .http
                .request(method.clone(), url)
                .header("Client-Id", self.auth.client_id())
                .bearer_auth(&token)
                .query(query);
            if let Body::Json(v) = body {
                req = req.json(v);
            }
            let resp = req.send().await?;
            let status = resp.status().as_u16();
            if status == 401 && attempt == 0 {
                tracing::warn!(target: "signorebot::helix", "Twitch ответил 401 ({} {}), обновляем токен {}", method, url, kind.label());
                token = self.auth.on_unauthorized(kind).await?;
                continue;
            }
            if resp.status().is_success() {
                return Ok(resp);
            }
            // Retry-After — секунды; Ratelimit-Reset — Unix-время сброса.
            let retry_after = resp
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse::<u64>().ok())
                .or_else(|| {
                    resp.headers()
                        .get("ratelimit-reset")
                        .and_then(|v| v.to_str().ok())
                        .and_then(|v| v.parse::<u64>().ok())
                        .map(|reset| reset.saturating_sub(chrono::Utc::now().timestamp() as u64))
                });
            let message = resp.json::<ErrBody>().await.map(|e| e.message).unwrap_or_default();
            return Err(HelixError::Http { status, message, retry_after });
        }
        unreachable!()
    }

    async fn get_json<T: DeserializeOwned>(&self, kind: AccountKind, path: &str, query: &[(&str, &str)]) -> Result<T, HelixError> {
        let resp = self.request(kind, reqwest::Method::GET, &format!("{HELIX}/{path}"), query, Body::None).await?;
        Ok(resp.json::<T>().await?)
    }

    // ------------------------------------------------------------------

    pub async fn user_by_login(&self, kind: AccountKind, login: &str) -> Result<Option<User>, HelixError> {
        let env: Envelope<User> = self.get_json(kind, "users", &[("login", &login.to_lowercase())]).await?;
        Ok(env.data.into_iter().next())
    }

    /// Список зрителей (до 1000, постранично — берём первую страницу в 1000).
    pub async fn chatters(&self, kind: AccountKind, broadcaster_id: &str, moderator_id: &str) -> Result<Vec<Chatter>, HelixError> {
        let env: Envelope<Chatter> = self
            .get_json(kind, "chat/chatters", &[("broadcaster_id", broadcaster_id), ("moderator_id", moderator_id), ("first", "1000")])
            .await?;
        Ok(env.data)
    }

    pub async fn send_chat_message(
        &self,
        kind: AccountKind,
        broadcaster_id: &str,
        sender_id: &str,
        message: &str,
        reply_parent: Option<&str>,
    ) -> Result<SendMessageResult, HelixError> {
        let mut body = serde_json::json!({
            "broadcaster_id": broadcaster_id,
            "sender_id": sender_id,
            "message": message,
        });
        if let Some(p) = reply_parent {
            body["reply_parent_message_id"] = serde_json::Value::String(p.to_string());
        }
        let resp = self.request(kind, reqwest::Method::POST, &format!("{HELIX}/chat/messages"), &[], Body::Json(&body)).await?;
        let env: Envelope<SendMessageResult> = resp.json().await?;
        env.data.into_iter().next().ok_or(HelixError::Http { status: 0, message: "пустой ответ".into(), retry_after: None })
    }

    pub async fn delete_chat_message(&self, kind: AccountKind, broadcaster_id: &str, moderator_id: &str, message_id: &str) -> Result<(), HelixError> {
        self.request(
            kind,
            reqwest::Method::DELETE,
            &format!("{HELIX}/moderation/chat"),
            &[("broadcaster_id", broadcaster_id), ("moderator_id", moderator_id), ("message_id", message_id)],
            Body::None,
        )
        .await?;
        Ok(())
    }

    pub async fn shoutout(&self, kind: AccountKind, from_id: &str, to_id: &str, moderator_id: &str) -> Result<(), HelixError> {
        self.request(
            kind,
            reqwest::Method::POST,
            &format!("{HELIX}/chat/shoutouts"),
            &[("from_broadcaster_id", from_id), ("to_broadcaster_id", to_id), ("moderator_id", moderator_id)],
            Body::None,
        )
        .await?;
        Ok(())
    }

    /// Все награды канала; `is_managed` — по второму запросу с
    /// `only_manageable_rewards=true` (награды, созданные нашим Client ID).
    pub async fn custom_rewards(&self, kind: AccountKind, broadcaster_id: &str) -> Result<Vec<ChannelReward>, HelixError> {
        let env: Envelope<RawReward> = self.get_json(kind, "channel_points/custom_rewards", &[("broadcaster_id", broadcaster_id)]).await?;
        let managed: std::collections::HashSet<String> = match self
            .get_json::<Envelope<RawReward>>(kind, "channel_points/custom_rewards", &[("broadcaster_id", broadcaster_id), ("only_manageable_rewards", "true")])
            .await
        {
            Ok(m) => m.data.into_iter().map(|r| r.id).collect(),
            Err(e) => {
                tracing::debug!(target: "signorebot::rewards", "Список управляемых наград недоступен: {e}");
                Default::default()
            }
        };
        Ok(env.data.into_iter().map(|r| raw_to_reward(r, &managed)).collect())
    }

    /// Создать награду от имени стримера (нужно `channel:manage:redemptions`).
    pub async fn create_custom_reward(&self, kind: AccountKind, broadcaster_id: &str, r: &NewReward) -> Result<ChannelReward, HelixError> {
        let body = r.body();
        let resp = self.request(kind, reqwest::Method::POST, &format!("{HELIX}/channel_points/custom_rewards"), &[("broadcaster_id", broadcaster_id)], Body::Json(&body)).await?;
        let env: Envelope<RawReward> = resp.json().await?;
        let mut managed = std::collections::HashSet::new();
        let raw = env.data.into_iter().next().ok_or(HelixError::Http { status: 0, message: "пустой ответ".into(), retry_after: None })?;
        managed.insert(raw.id.clone());
        Ok(raw_to_reward(raw, &managed))
    }

    /// Изменить параметры награды, созданной нашим приложением; возвращает награду как она теперь на канале.
    pub async fn update_custom_reward(&self, kind: AccountKind, broadcaster_id: &str, reward_id: &str, r: &NewReward) -> Result<ChannelReward, HelixError> {
        let body = r.body();
        let resp = self.request(kind, reqwest::Method::PATCH, &format!("{HELIX}/channel_points/custom_rewards"), &[("broadcaster_id", broadcaster_id), ("id", reward_id)], Body::Json(&body)).await?;
        let env: Envelope<RawReward> = resp.json().await?;
        let raw = env.data.into_iter().next().ok_or(HelixError::Http { status: 0, message: "пустой ответ".into(), retry_after: None })?;
        let mut managed = std::collections::HashSet::new();
        managed.insert(raw.id.clone());
        Ok(raw_to_reward(raw, &managed))
    }

    /// Удалить награду, созданную нашим приложением (Twitch не даст удалить чужую).
    pub async fn delete_custom_reward(&self, kind: AccountKind, broadcaster_id: &str, reward_id: &str) -> Result<(), HelixError> {
        self.request(kind, reqwest::Method::DELETE, &format!("{HELIX}/channel_points/custom_rewards"), &[("broadcaster_id", broadcaster_id), ("id", reward_id)], Body::None).await?;
        Ok(())
    }

    /// Переименовать награду, созданную нашим приложением.
    pub async fn update_custom_reward_title(&self, kind: AccountKind, broadcaster_id: &str, reward_id: &str, title: &str) -> Result<(), HelixError> {
        let body = serde_json::json!({ "title": title });
        self.request(kind, reqwest::Method::PATCH, &format!("{HELIX}/channel_points/custom_rewards"), &[("broadcaster_id", broadcaster_id), ("id", reward_id)], Body::Json(&body)).await?;
        Ok(())
    }

    /// Закрыть погашение: `CANCELED` возвращает баллы зрителю, `FULFILLED` — нет.
    /// Только для наград, созданных нашим приложением, и только пока погашение UNFULFILLED.
    pub async fn update_redemption_status(&self, kind: AccountKind, broadcaster_id: &str, reward_id: &str, redemption_id: &str, status: &str) -> Result<(), HelixError> {
        let body = serde_json::json!({ "status": status });
        self.request(kind, reqwest::Method::PATCH, &format!("{HELIX}/channel_points/custom_rewards/redemptions"), &[("broadcaster_id", broadcaster_id), ("reward_id", reward_id), ("id", redemption_id)], Body::Json(&body)).await?;
        Ok(())
    }

    pub async fn create_eventsub(
        &self,
        kind: AccountKind,
        sub_type: &str,
        version: &str,
        condition: serde_json::Value,
        session_id: &str,
    ) -> Result<EventSubCreated, HelixError> {
        let body = serde_json::json!({
            "type": sub_type,
            "version": version,
            "condition": condition,
            "transport": { "method": "websocket", "session_id": session_id },
        });
        let resp = self.request(kind, reqwest::Method::POST, &format!("{HELIX}/eventsub/subscriptions"), &[], Body::Json(&body)).await?;
        let env: Envelope<EventSubCreated> = resp.json().await?;
        env.data.into_iter().next().ok_or(HelixError::Http { status: 0, message: "пустой ответ".into(), retry_after: None })
    }
}

/// Image URLs are Twitch-provided runtime content, never panel assets.
#[derive(Debug, Clone, Deserialize, Serialize, ts_rs::TS)]
#[serde(rename_all(serialize = "camelCase", deserialize = "snake_case"))]
#[ts(export, export_to = "api.ts", rename_all = "camelCase")]
pub struct BadgeImage { pub id: String, pub image_url_2x: String, pub title: String }
#[derive(Debug, Clone, Deserialize, Serialize, ts_rs::TS)]
#[serde(rename_all(serialize = "camelCase", deserialize = "snake_case"))]
#[ts(export, export_to = "api.ts", rename_all = "camelCase")]
pub struct BadgeSet { pub set_id: String, pub versions: Vec<BadgeImage> }
impl Helix {
    pub async fn chat_badges(&self, broadcaster_id: &str) -> Result<Vec<BadgeSet>, HelixError> {
        let mut global: Envelope<BadgeSet> = self.get_json(AccountKind::Broadcaster, "chat/badges/global", &[]).await?;
        let channel: Envelope<BadgeSet> = self.get_json(AccountKind::Broadcaster, "chat/badges", &[("broadcaster_id", broadcaster_id)]).await?;
        global.data.extend(channel.data); Ok(global.data)
    }
    pub async fn ban_user(&self, broadcaster_id: &str, user_id: &str, seconds: Option<u32>, reason: &str) -> Result<(), HelixError> {
        let mut data = serde_json::json!({"user_id": user_id, "reason": reason});
        if let Some(duration) = seconds { data["duration"] = duration.into(); }
        self.request(AccountKind::Broadcaster, reqwest::Method::POST, &format!("{HELIX}/moderation/bans"),
            &[("broadcaster_id", broadcaster_id), ("moderator_id", broadcaster_id)], Body::Json(&serde_json::json!({"data": data}))).await?;
        Ok(())
    }
}

impl Helix {
    pub async fn warn_user(&self, broadcaster_id: &str, user_id: &str, reason: &str) -> Result<(), HelixError> {
        self.request(AccountKind::Broadcaster, reqwest::Method::POST, &format!("{HELIX}/moderation/warnings"),
            &[("broadcaster_id", broadcaster_id), ("moderator_id", broadcaster_id)],
            Body::Json(&serde_json::json!({"data":{"user_id":user_id,"reason":reason}}))).await?;
        Ok(())
    }
}
#[cfg(test)]
mod reward_image_tests {
    use super::*;
    #[test] fn reward_images_use_large_custom_image_then_default() {
        let custom = serde_json::json!({"url_1x":"custom-1","url_2x":"custom-2","url_4x":"custom-4"});
        let default = serde_json::json!({"url_1x":"default-1","url_4x":"default-4"});
        assert_eq!(reward_image(Some(serde_json::from_value(custom).unwrap()), Some(serde_json::from_value(default.clone()).unwrap())).as_deref(), Some("custom-4"));
        assert_eq!(reward_image(None, Some(serde_json::from_value(default).unwrap())).as_deref(), Some("default-4"));
        assert_eq!(reward_image(Some(serde_json::from_value(serde_json::json!({"url_1x":"old"})).unwrap()), None).as_deref(), Some("old"));
        assert_eq!(reward_image(None, None), None);
    }
}

impl NewReward {
    pub fn managed_copy(original: &ChannelReward) -> Self {
        Self { title: format!("{} (бот)", original.title), cost: original.cost, prompt: original.prompt.clone(),
            is_user_input_required: original.requires_input, is_enabled: original.is_enabled,
            background_color: original.background_color.clone(), cooldown_seconds: original.cooldown_seconds,
            max_per_stream: original.max_per_stream, max_per_user_per_stream: original.max_per_user_per_stream,
            skip_queue: false }
    }
}
impl Helix {
    pub async fn preserve_reward_image(&self, url: &str) -> Result<String, String> {
        use base64::Engine as _;
        let mut response = self.http.get(url).send().await.map_err(|e| e.to_string())?.error_for_status().map_err(|e|e.to_string())?;
        let mut data = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|e|e.to_string())? {
            if data.len() + chunk.len() > 1024 * 1024 { return Err("Картинка награды больше 1 МБ".into()); }
            data.extend_from_slice(&chunk);
        }
        let kind = infer::get(&data).ok_or("Неизвестный формат картинки награды")?;
        if !["image/png", "image/jpeg", "image/webp", "image/gif"].contains(&kind.mime_type()) { return Err("Неподдерживаемый формат картинки награды".into()); }
        Ok(format!("data:{};base64,{}", kind.mime_type(), base64::engine::general_purpose::STANDARD.encode(&data)))
    }
}
#[cfg(test)]
mod managed_copy_tests {
    use super::*;
    #[test] fn copy_preserves_color_text_and_limits_in_the_actual_request_body() {
        let raw: RawReward = serde_json::from_value(serde_json::json!({"id":"r","title":"Question","cost":200,"is_enabled":true,"is_paused":false,"is_user_input_required":true,"background_color":"#3ad7bd","prompt":"Your question","image":null,"default_image":null,"global_cooldown_setting":{"is_enabled":true,"global_cooldown_seconds":60},"max_per_stream_setting":{"is_enabled":true,"max_per_stream":10},"max_per_user_per_stream_setting":{"is_enabled":true,"max_per_user_per_stream":2}})).unwrap();
        let original=raw_to_reward(raw,&Default::default()); let body=NewReward::managed_copy(&original).body();
        assert_eq!(body["background_color"],"#3AD7BD"); assert_eq!(body["cost"],200); assert_eq!(body["prompt"],"Your question");
        assert_eq!(body["is_user_input_required"],true); assert_eq!(body["global_cooldown_seconds"],60);
        assert_eq!(body["max_per_stream"],10); assert_eq!(body["max_per_user_per_stream"],2); assert_eq!(body["should_redemptions_skip_request_queue"],false);
    }
}
