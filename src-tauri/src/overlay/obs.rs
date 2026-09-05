//! Интеграция с OBS WebSocket (v5): перезагрузка Browser Source, если
//! оверлеи не подключились; проверка соединения; список источников.

use crate::config::ObsSettings;
use obws::requests::inputs::{InputId, SetSettings};
use serde::Serialize;

#[derive(Debug, Clone, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "api.ts")]
pub struct ObsSource {
    pub input_name: String,
    pub input_kind: String,
    /// Текущий URL (если это browser source).
    pub url: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum ObsError {
    #[error("некорректный адрес OBS WebSocket: {0}")]
    BadUrl(String),
    #[error("OBS недоступен: {0}")]
    Connect(String),
    #[error("неверный пароль OBS WebSocket")]
    Auth,
    #[error("OBS: {0}")]
    Other(String),
}

fn parse_url(url: &str) -> Result<(String, u16), ObsError> {
    let u = url::Url::parse(url).map_err(|_| ObsError::BadUrl(url.into()))?;
    let host = u.host_str().ok_or_else(|| ObsError::BadUrl(url.into()))?.to_string();
    Ok((host, u.port().unwrap_or(4455)))
}

fn map_err(e: obws::error::Error) -> ObsError {
    let s = e.to_string();
    let l = s.to_lowercase();
    if l.contains("auth") || l.contains("4009") || l.contains("password") {
        ObsError::Auth
    } else if l.contains("connect") || l.contains("refused") || l.contains("timeout") || l.contains("io error") {
        ObsError::Connect(s)
    } else {
        ObsError::Other(s)
    }
}

async fn connect(settings: &ObsSettings) -> Result<obws::Client, ObsError> {
    let (host, port) = parse_url(&settings.url)?;
    let pw = if settings.password.is_empty() { None } else { Some(settings.password.clone()) };
    obws::Client::connect(host, port, pw).await.map_err(map_err)
}

/// Проверить соединение и вернуть список источников (browser source — с URL).
pub async fn test_connection(settings: &ObsSettings) -> Result<Vec<ObsSource>, ObsError> {
    let mut client = connect(settings).await?;
    let inputs = client.inputs().list(None).await.map_err(map_err)?;
    let mut out = Vec::new();
    for i in inputs {
        let is_browser = i.unversioned_kind.contains("browser");
        let url = if is_browser {
            client
                .inputs()
                .settings::<serde_json::Value>(InputId::Name(&i.id.name))
                .await
                .ok()
                .and_then(|s| s.settings.get("url").and_then(|u| u.as_str()).map(String::from))
        } else {
            None
        };
        out.push(ObsSource { input_name: i.id.name, input_kind: i.kind, url });
    }
    client.disconnect().await;
    Ok(out)
}

/// Перезагрузить перечисленные Browser Source, добавив cache-buster в URL.
/// Возвращает имена обновлённых источников.
pub async fn refresh_browser_sources(settings: &ObsSettings, input_names: &[String]) -> Result<Vec<String>, ObsError> {
    let mut client = connect(settings).await?;
    let mut refreshed = Vec::new();
    for name in input_names {
        let current = match client.inputs().settings::<serde_json::Value>(InputId::Name(name)).await {
            Ok(s) => s.settings,
            Err(e) => {
                let msg = e.to_string();
                if msg.contains("600") || msg.to_lowercase().replace(' ', "").contains("notfound") {
                    tracing::warn!(target: "signorebot::obs", "Источник «{name}» не найден в OBS. Проверьте имя в настройках.");
                } else {
                    tracing::warn!(target: "signorebot::obs", "Не удалось прочитать источник «{name}»: {msg}");
                }
                continue;
            }
        };
        let Some(url) = current.get("url").and_then(|u| u.as_str()) else {
            tracing::warn!(target: "signorebot::obs", "У источника «{name}» нет URL, пропуск");
            continue;
        };
        let new_url = match url::Url::parse(url) {
            Ok(mut u) => {
                let pairs: Vec<(String, String)> = u.query_pairs().filter(|(k, _)| k != "_botReload").map(|(k, v)| (k.into_owned(), v.into_owned())).collect();
                u.query_pairs_mut().clear().extend_pairs(pairs).append_pair("_botReload", &chrono::Utc::now().timestamp_millis().to_string());
                u.to_string()
            }
            Err(_) => format!("{url}{}_botReload={}", if url.contains('?') { "&" } else { "?" }, chrono::Utc::now().timestamp_millis()),
        };
        let settings_patch = serde_json::json!({ "url": new_url });
        match client.inputs().set_settings(SetSettings { input: InputId::Name(name), settings: &settings_patch, overlay: Some(true) }).await {
            Ok(()) => {
                tracing::info!(target: "signorebot::obs", "Browser Source «{name}» перезагружен");
                refreshed.push(name.clone());
            }
            Err(e) => tracing::warn!(target: "signorebot::obs", "Не удалось обновить «{name}»: {e}"),
        }
    }
    client.disconnect().await;
    Ok(refreshed)
}

/// Установить URL источника (кнопка «В OBS»). Если адрес уже такой —
/// ничего не пишем (запись перезагружает страницу в OBS); `Ok(false)`.
pub async fn set_browser_source_url(settings: &ObsSettings, input_name: &str, url: &str) -> Result<bool, ObsError> {
    let mut client = connect(settings).await?;
    let current = client
        .inputs()
        .settings::<serde_json::Value>(InputId::Name(input_name))
        .await
        .map_err(map_err)?
        .settings
        .get("url")
        .and_then(|u| u.as_str())
        .map(String::from);
    if current.as_deref() == Some(url) {
        client.disconnect().await;
        return Ok(false);
    }
    let patch = serde_json::json!({ "url": url });
    let r = client.inputs().set_settings(SetSettings { input: InputId::Name(input_name), settings: &patch, overlay: Some(true) }).await.map_err(map_err);
    client.disconnect().await;
    r.map(|_| true)
}

/// Имена всех Browser Source в OBS — для подсказок, когда привязка не нашлась.
pub async fn browser_source_names(settings: &ObsSettings) -> Result<Vec<String>, ObsError> {
    Ok(test_connection(settings).await?.into_iter().filter(|s| s.input_kind.contains("browser")).map(|s| s.input_name).collect())
}

/// Подобрать Browser Source по адресам: источник, чей URL ведёт на
/// `/overlay/<path>`, привязывается к этому оверлею. Возвращает пары
/// (путь оверлея, имя источника).
pub async fn match_sources(settings: &ObsSettings, paths: &[String]) -> Result<Vec<(String, String)>, ObsError> {
    let sources = test_connection(settings).await?;
    let mut out = Vec::new();
    for path in paths {
        let needle = format!("/overlay/{path}");
        if let Some(s) = sources.iter().find(|s| s.url.as_deref().map(|u| u.split('?').next().unwrap_or(u).trim_end_matches('/').ends_with(&needle)).unwrap_or(false)) {
            out.push((path.clone(), s.input_name.clone()));
        }
    }
    Ok(out)
}
