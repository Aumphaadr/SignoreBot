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
}
#[derive(Debug, Deserialize)]
struct RewardImage {
    url_1x: String,
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

    pub async fn custom_rewards(&self, kind: AccountKind, broadcaster_id: &str) -> Result<Vec<ChannelReward>, HelixError> {
        let env: Envelope<RawReward> = self.get_json(kind, "channel_points/custom_rewards", &[("broadcaster_id", broadcaster_id)]).await?;
        Ok(env
            .data
            .into_iter()
            .map(|r| ChannelReward {
                id: r.id,
                title: r.title,
                cost: r.cost,
                is_enabled: r.is_enabled,
                is_paused: r.is_paused,
                requires_input: r.is_user_input_required,
                background_color: r.background_color,
                image: r.image.or(r.default_image).map(|i| i.url_1x),
            })
            .collect())
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
