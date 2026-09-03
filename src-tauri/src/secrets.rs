//! Хранилище токенов Twitch.
//!
//! Основной бэкенд — системный keyring (KWallet/GNOME Keyring через Secret
//! Service, Windows Credential Manager, macOS Keychain). Если keyring
//! недоступен (нет D-Bus, headless и т.п.) — запасной файл `secrets.json` в
//! каталоге данных с правами 0600 и предупреждением в логе.
//!
//! Refresh-токены публичного клиента Twitch одноразовые, поэтому каждое
//! обновление записывается немедленно и атомарно.

use crate::paths::AppPaths;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::Arc;

const SERVICE: &str = "SignoreBot";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "api.ts")]
pub enum AccountKind {
    Broadcaster,
    Bot,
}

impl AccountKind {
    pub fn key(self) -> &'static str {
        match self {
            AccountKind::Broadcaster => "broadcaster",
            AccountKind::Bot => "bot",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            AccountKind::Broadcaster => "стримера",
            AccountKind::Bot => "бота",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TokenPair {
    pub access_token: String,
    pub refresh_token: String,
    /// Unix-время (с) предполагаемого истечения access-токена.
    #[serde(default)]
    pub expires_at: i64,
    #[serde(default)]
    pub scopes: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum SecretsError {
    #[error("keyring: {0}")]
    Keyring(String),
    #[error("файл секретов: {0}")]
    Io(#[from] std::io::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
}

#[derive(Clone)]
pub struct Secrets {
    inner: Arc<Mutex<Backend>>,
}

enum Backend {
    /// Системное хранилище; `shadow` — файл-страховка: если запись в keyring
    /// вдруг откажет, обновлённый (одноразовый!) refresh-токен не потеряется.
    Keyring { shadow: std::path::PathBuf },
    File { path: std::path::PathBuf, cache: BTreeMap<String, TokenPair> },
}

fn read_shadow(path: &std::path::Path) -> BTreeMap<String, TokenPair> {
    std::fs::read_to_string(path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

impl Secrets {
    /// Выбрать бэкенд: пробуем keyring пробной записью; иначе файл.
    ///
    /// В dev-режиме (`SIGNOREBOT_DATA_DIR` задан) или при `SIGNOREBOT_SECRETS=file`
    /// keyring НЕ трогаем: он общий для всех копий приложения, и тестовый
    /// экземпляр иначе подхватит боевые токены и подключится к каналу.
    pub fn open(paths: &AppPaths) -> Self {
        let forced = std::env::var("SIGNOREBOT_SECRETS").ok();
        let dev = std::env::var_os("SIGNOREBOT_DATA_DIR").is_some();
        if forced.as_deref() == Some("file") || (dev && forced.as_deref() != Some("keyring")) {
            tracing::warn!(target: "signorebot::secrets", "Токены хранятся в файле secrets.json каталога данных (dev-режим; общий keyring не используется)");
            return Self::file_only(paths);
        }
        let probe = keyring::Entry::new(SERVICE, "__probe__").and_then(|e| {
            e.set_password("ok")?;
            let v = e.get_password()?;
            let _ = e.delete_credential();
            Ok(v)
        });
        match probe {
            Ok(v) if v == "ok" => {
                tracing::info!(target: "signorebot::secrets", "Токены хранятся в системном хранилище (keyring)");
                Self { inner: Arc::new(Mutex::new(Backend::Keyring { shadow: paths.secrets_fallback_file() })) }
            }
            other => {
                let why = match other {
                    Ok(_) => "неожиданный ответ".to_string(),
                    Err(e) => e.to_string(),
                };
                tracing::warn!(target: "signorebot::secrets",
                    "Системное хранилище недоступно ({why}); токены будут в файле secrets.json (права 0600)");
                let path = paths.secrets_fallback_file();
                let cache = std::fs::read_to_string(&path)
                    .ok()
                    .and_then(|t| serde_json::from_str(&t).ok())
                    .unwrap_or_default();
                Self { inner: Arc::new(Mutex::new(Backend::File { path, cache })) }
            }
        }
    }

    /// Только для тестов: файловый бэкенд без keyring.
    pub fn file_only(paths: &AppPaths) -> Self {
        let path = paths.secrets_fallback_file();
        let cache = std::fs::read_to_string(&path)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        Self { inner: Arc::new(Mutex::new(Backend::File { path, cache })) }
    }

    pub fn backend_name(&self) -> &'static str {
        match &*self.inner.lock() {
            Backend::Keyring { .. } => "keyring",
            Backend::File { .. } => "file",
        }
    }

    pub fn get(&self, kind: AccountKind) -> Result<Option<TokenPair>, SecretsError> {
        match &*self.inner.lock() {
            Backend::Keyring { shadow } => {
                let entry = keyring::Entry::new(SERVICE, kind.key()).map_err(|e| SecretsError::Keyring(e.to_string()))?;
                let from_keyring: Option<TokenPair> = match entry.get_password() {
                    Ok(s) => serde_json::from_str(&s).ok(),
                    Err(keyring::Error::NoEntry) => None,
                    Err(e) => return Err(SecretsError::Keyring(e.to_string())),
                };
                // Страховка новее keyring, если последняя запись в keyring не удалась.
                let from_shadow = read_shadow(shadow).remove(kind.key());
                Ok(match (from_keyring, from_shadow) {
                    (Some(k), Some(f)) if f.expires_at > k.expires_at => Some(f),
                    (Some(k), _) => Some(k),
                    (None, f) => f,
                })
            }
            Backend::File { cache, .. } => Ok(cache.get(kind.key()).cloned()),
        }
    }

    pub fn set(&self, kind: AccountKind, pair: &TokenPair) -> Result<(), SecretsError> {
        let mut guard = self.inner.lock();
        match &mut *guard {
            Backend::Keyring { shadow } => {
                let entry = keyring::Entry::new(SERVICE, kind.key()).map_err(|e| SecretsError::Keyring(e.to_string()))?;
                match entry.set_password(&serde_json::to_string(pair)?) {
                    Ok(()) => {
                        // keyring принял — страховка не нужна (и не должна пережить logout)
                        let mut cache = read_shadow(shadow);
                        if cache.remove(kind.key()).is_some() {
                            let _ = write_secret_file(shadow, &cache);
                        }
                        Ok(())
                    }
                    Err(e) => {
                        tracing::error!(target: "signorebot::secrets", "keyring не принял токен {}: {e}; записан в secrets.json (права 0600)", kind.label());
                        let mut cache = read_shadow(shadow);
                        cache.insert(kind.key().to_string(), pair.clone());
                        write_secret_file(shadow, &cache)
                    }
                }
            }
            Backend::File { path, cache } => {
                cache.insert(kind.key().to_string(), pair.clone());
                write_secret_file(path, cache)
            }
        }
    }

    pub fn delete(&self, kind: AccountKind) -> Result<(), SecretsError> {
        let mut guard = self.inner.lock();
        match &mut *guard {
            Backend::Keyring { shadow } => {
                let mut cache = read_shadow(shadow);
                if cache.remove(kind.key()).is_some() {
                    let _ = write_secret_file(shadow, &cache);
                }
                let entry = keyring::Entry::new(SERVICE, kind.key()).map_err(|e| SecretsError::Keyring(e.to_string()))?;
                match entry.delete_credential() {
                    Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
                    Err(e) => Err(SecretsError::Keyring(e.to_string())),
                }
            }
            Backend::File { path, cache } => {
                cache.remove(kind.key());
                write_secret_file(path, cache)
            }
        }
    }
}

fn write_secret_file(path: &std::path::Path, cache: &BTreeMap<String, TokenPair>) -> Result<(), SecretsError> {
    let bytes = serde_json::to_vec_pretty(cache)?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("tmp");
    {
        let mut f = std::fs::File::create(&tmp)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = f.set_permissions(std::fs::Permissions::from_mode(0o600));
        }
        use std::io::Write;
        f.write_all(&bytes)?;
        f.sync_all()?;
    }
    std::fs::rename(&tmp, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_backend_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::new(dir.path());
        let s = Secrets::file_only(&paths);
        assert!(s.get(AccountKind::Bot).unwrap().is_none());
        let pair = TokenPair { access_token: "a".into(), refresh_token: "r".into(), expires_at: 1, scopes: vec!["x".into()] };
        s.set(AccountKind::Bot, &pair).unwrap();
        assert_eq!(s.get(AccountKind::Bot).unwrap(), Some(pair.clone()));
        // перечитываем с диска
        let s2 = Secrets::file_only(&paths);
        assert_eq!(s2.get(AccountKind::Bot).unwrap(), Some(pair));
        s2.delete(AccountKind::Bot).unwrap();
        assert!(Secrets::file_only(&paths).get(AccountKind::Bot).unwrap().is_none());
    }
}
