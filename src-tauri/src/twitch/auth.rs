//! OAuth Twitch для публичного клиента: Device Code Grant Flow, refresh,
//! validate. Секрета нет — только `client_id`.

use serde::{Deserialize, Serialize};

pub const ID_BASE: &str = "https://id.twitch.tv/oauth2";

/// Права, которые запрашиваем у стримера.
pub const BROADCASTER_SCOPES: &[&str] = &[
    "user:read:chat",              // чат через EventSub channel.chat.message / notification
    "moderator:read:followers",    // channel.follow v2
    "channel:read:subscriptions",  // subscribe / resub / gift
    "channel:read:redemptions",    // награды за баллы
    "bits:read",                   // cheer
    "moderator:read:chatters",     // список зрителей
    "moderator:read:shoutouts",
    "moderator:manage:shoutouts",  // /shoutout от имени стримера
];

/// Права бота (пишет в чат и удаляет сообщения как модератор).
pub const BOT_SCOPES: &[&str] = &[
    "user:read:chat",
    "user:write:chat",
    "moderator:manage:chat_messages",
    "moderator:manage:shoutouts",
];

#[derive(Debug, Clone, Deserialize, Serialize, ts_rs::TS)]
#[serde(rename_all(serialize = "camelCase", deserialize = "snake_case"))]
#[ts(export, export_to = "api.ts", rename_all = "camelCase")]
pub struct DeviceCode {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    #[ts(type = "number")]
    pub expires_in: u64,
    #[ts(type = "number")]
    pub interval: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TokenResponse {
    pub access_token: String,
    pub refresh_token: String,
    #[serde(default)]
    pub expires_in: i64,
    #[serde(default)]
    pub scope: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ValidateResponse {
    pub client_id: String,
    pub login: String,
    pub user_id: String,
    #[serde(default)]
    pub scopes: Vec<String>,
    #[serde(default)]
    pub expires_in: i64,
}

#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("сеть: {0}")]
    Network(#[from] reqwest::Error),
    /// Пользователь ещё не подтвердил код.
    #[error("ожидание подтверждения")]
    Pending,
    #[error("пользователь отклонил авторизацию")]
    Denied,
    #[error("код устарел, начните авторизацию заново")]
    Expired,
    #[error("токен недействителен")]
    Invalid,
    #[error("Twitch: {0}")]
    Twitch(String),
}

#[derive(Debug, Deserialize)]
struct TwitchErr {
    #[serde(default)]
    message: String,
    #[serde(default)]
    status: u16,
}

fn http() -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent("SignoreBot/0.1")
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .expect("reqwest client")
}

/// Шаг 1: запросить код устройства.
pub async fn device_code(client_id: &str, scopes: &[&str]) -> Result<DeviceCode, AuthError> {
    let resp = http()
        .post(format!("{ID_BASE}/device"))
        .form(&[("client_id", client_id), ("scopes", &scopes.join(" "))])
        .send()
        .await?;
    if !resp.status().is_success() {
        let e: TwitchErr = resp.json().await.unwrap_or(TwitchErr { message: "?".into(), status: 0 });
        return Err(AuthError::Twitch(format!("{} ({})", e.message, e.status)));
    }
    Ok(resp.json().await?)
}

/// Шаг 2: опрос токена. Возвращает `Pending`, пока пользователь не подтвердил.
pub async fn poll_device_token(client_id: &str, device_code: &str) -> Result<TokenResponse, AuthError> {
    let resp = http()
        .post(format!("{ID_BASE}/token"))
        .form(&[
            ("client_id", client_id),
            ("device_code", device_code),
            ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
        ])
        .send()
        .await?;
    let status = resp.status();
    if status.is_success() {
        return Ok(resp.json().await?);
    }
    let e: TwitchErr = resp.json().await.unwrap_or(TwitchErr { message: "?".into(), status: status.as_u16() });
    let m = e.message.to_lowercase();
    if m.contains("authorization_pending") || m.contains("pending") {
        Err(AuthError::Pending)
    } else if m.contains("denied") || m.contains("access_denied") {
        Err(AuthError::Denied)
    } else if m.contains("expired") {
        Err(AuthError::Expired)
    } else if m.contains("slow_down") {
        Err(AuthError::Pending)
    } else {
        Err(AuthError::Twitch(format!("{} ({})", e.message, e.status)))
    }
}

/// Обновить токен (публичный клиент — без секрета). Старый refresh-токен
/// после успеха недействителен.
pub async fn refresh(client_id: &str, refresh_token: &str) -> Result<TokenResponse, AuthError> {
    let resp = http()
        .post(format!("{ID_BASE}/token"))
        .form(&[("client_id", client_id), ("grant_type", "refresh_token"), ("refresh_token", refresh_token)])
        .send()
        .await?;
    let status = resp.status();
    if status.is_success() {
        return Ok(resp.json().await?);
    }
    let e: TwitchErr = resp.json().await.unwrap_or(TwitchErr { message: "?".into(), status: status.as_u16() });
    if status.as_u16() == 400 || status.as_u16() == 401 {
        return Err(AuthError::Invalid);
    }
    Err(AuthError::Twitch(format!("{} ({})", e.message, e.status)))
}

pub async fn validate(access_token: &str) -> Result<ValidateResponse, AuthError> {
    let resp = http().get(format!("{ID_BASE}/validate")).bearer_auth(access_token).send().await?;
    if resp.status().as_u16() == 401 {
        return Err(AuthError::Invalid);
    }
    if !resp.status().is_success() {
        return Err(AuthError::Twitch(format!("validate: HTTP {}", resp.status())));
    }
    Ok(resp.json().await?)
}

pub async fn revoke(client_id: &str, token: &str) -> Result<(), AuthError> {
    let _ = http()
        .post(format!("{ID_BASE}/revoke"))
        .form(&[("client_id", client_id), ("token", token)])
        .send()
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn twitch_snake_case_is_parsed_and_ui_gets_camel_case() {
        // Улов владельца: ответ /oauth2/device не разбирался — rename_all для
        // TypeScript ломал десериализацию snake_case-ответа Twitch.
        let raw = r#"{"device_code":"d","expires_in":1800,"interval":5,"user_code":"ABCD","verification_uri":"https://www.twitch.tv/activate?device-code=ABCD"}"#;
        let dc: DeviceCode = serde_json::from_str(raw).unwrap();
        assert_eq!(dc.user_code, "ABCD");
        let out = serde_json::to_value(&dc).unwrap();
        assert_eq!(out["userCode"], "ABCD");
        assert_eq!(out["verificationUri"], "https://www.twitch.tv/activate?device-code=ABCD");
    }
}

/// Каких прав не хватает.
pub fn missing_scopes(have: &[String], need: &[&str]) -> Vec<String> {
    need.iter().filter(|s| !have.iter().any(|h| h == *s)).map(|s| s.to_string()).collect()
}
