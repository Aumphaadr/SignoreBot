//! Менеджер аккаунтов: хранит пары токенов, обновляет их с ретраями,
//! ведёт Device-Code-авторизацию и сообщает статус в UI.

use super::auth::{self, AuthError, DeviceCode};
use crate::config::AccountInfo;
use crate::secrets::{AccountKind, Secrets, TokenPair};
use parking_lot::Mutex;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex as AsyncMutex};

#[derive(Debug, Clone, Serialize, ts_rs::TS, Default)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "api.ts")]
pub struct AccountStatus {
    /// none | pending | authorized | invalid
    pub state: String,
    pub login: Option<String>,
    pub user_id: Option<String>,
    pub scopes: Vec<String>,
    pub missing_scopes: Vec<String>,
    /// Unix-время истечения access-токена, с.
    #[ts(type = "number")]
    pub expires_at: Option<i64>,
    /// Ошибка последней операции (для UI).
    pub error: Option<String>,
    /// Активная Device-Code-авторизация.
    pub device: Option<DeviceCode>,
}

#[derive(Default)]
struct Slot {
    tokens: Option<TokenPair>,
    info: Option<AccountInfo>,
    error: Option<String>,
    device: Option<DeviceCode>,
    /// Токен признан недействительным и обновить не удалось.
    invalid: bool,
}

/// Уведомления об изменении состояния аккаунтов.
#[derive(Debug, Clone)]
pub enum AuthEvent {
    Changed(AccountKind),
}

pub struct AuthManager {
    client_id: Mutex<String>,
    secrets: Secrets,
    slots: Mutex<HashMap<AccountKind, Slot>>,
    /// Сериализует refresh для каждого аккаунта (refresh-токен одноразовый!).
    refresh_locks: HashMap<AccountKind, AsyncMutex<()>>,
    tx: broadcast::Sender<AuthEvent>,
    /// Режим «один аккаунт»: роль бота использует токен и данные стримера,
    /// а стример авторизуется с объединённым набором прав.
    shared: std::sync::atomic::AtomicBool,
}

/// Права стримера вместе с правами бота: запрашиваются у стримера всегда,
/// обязательны — только в режиме «один аккаунт».
fn union_scopes() -> &'static [&'static str] {
    static UNION: std::sync::OnceLock<Vec<&'static str>> = std::sync::OnceLock::new();
    UNION.get_or_init(|| {
        let mut v: Vec<&'static str> = auth::BROADCASTER_SCOPES.to_vec();
        for s in auth::BOT_SCOPES {
            if !v.contains(s) {
                v.push(s);
            }
        }
        v
    })
}

impl AuthManager {
    pub fn new(client_id: String, secrets: Secrets) -> Arc<Self> {
        let (tx, _) = broadcast::channel(16);
        let mut refresh_locks = HashMap::new();
        refresh_locks.insert(AccountKind::Broadcaster, AsyncMutex::new(()));
        refresh_locks.insert(AccountKind::Bot, AsyncMutex::new(()));
        let mut slots = HashMap::new();
        for kind in [AccountKind::Broadcaster, AccountKind::Bot] {
            let tokens = secrets.get(kind).ok().flatten();
            slots.insert(kind, Slot { tokens, ..Default::default() });
        }
        Arc::new(Self { client_id: Mutex::new(client_id), secrets, slots: Mutex::new(slots), refresh_locks, tx, shared: std::sync::atomic::AtomicBool::new(false) })
    }

    pub fn is_shared(&self) -> bool {
        self.shared.load(std::sync::atomic::Ordering::SeqCst)
    }
    /// Включить/выключить режим «один аккаунт». Уведомляет обе роли.
    pub fn set_shared(&self, on: bool) {
        self.shared.store(on, std::sync::atomic::Ordering::SeqCst);
        self.notify(AccountKind::Broadcaster);
    }
    /// Реальный слот для роли: в режиме «один аккаунт» бот читает слот стримера.
    fn k(&self, kind: AccountKind) -> AccountKind {
        if kind == AccountKind::Bot && self.is_shared() { AccountKind::Broadcaster } else { kind }
    }
    /// Набор прав, который нужен роли с учётом режима.
    pub fn required_scopes_for(&self, kind: AccountKind) -> &'static [&'static str] {
        if self.is_shared() { union_scopes() } else { Self::required_scopes(kind) }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<AuthEvent> {
        self.tx.subscribe()
    }
    pub fn client_id(&self) -> String {
        self.client_id.lock().clone()
    }
    pub fn set_client_id(&self, id: String) {
        *self.client_id.lock() = id;
    }

    fn notify(&self, kind: AccountKind) {
        let _ = self.tx.send(AuthEvent::Changed(kind));
        if kind == AccountKind::Broadcaster && self.is_shared() {
            let _ = self.tx.send(AuthEvent::Changed(AccountKind::Bot));
        }
    }

    pub fn required_scopes(kind: AccountKind) -> &'static [&'static str] {
        match kind {
            AccountKind::Broadcaster => auth::BROADCASTER_SCOPES,
            AccountKind::Bot => auth::BOT_SCOPES,
        }
    }

    pub fn info(&self, kind: AccountKind) -> Option<AccountInfo> {
        let kind = self.k(kind);
        self.slots.lock().get(&kind).and_then(|s| s.info.clone())
    }
    pub fn has_tokens(&self, kind: AccountKind) -> bool {
        let kind = self.k(kind);
        self.slots.lock().get(&kind).map(|s| s.tokens.is_some()).unwrap_or(false)
    }
    pub fn is_ready(&self, kind: AccountKind) -> bool {
        let kind = self.k(kind);
        self.slots.lock().get(&kind).map(|s| s.tokens.is_some() && s.info.is_some() && !s.invalid).unwrap_or(false)
    }
    /// Оба аккаунта авторизованы и проверены.
    pub fn both_ready(&self) -> bool {
        self.is_ready(AccountKind::Broadcaster) && self.is_ready(AccountKind::Bot)
    }

    pub fn status(&self, kind: AccountKind) -> AccountStatus {
        let required = self.required_scopes_for(kind);
        let kind = self.k(kind);
        let slots = self.slots.lock();
        let s = slots.get(&kind).expect("slot");
        let scopes = s.tokens.as_ref().map(|t| t.scopes.clone()).unwrap_or_default();
        let state = if s.device.is_some() {
            "pending"
        } else if s.tokens.is_none() {
            "none"
        } else if s.invalid {
            "invalid"
        } else {
            "authorized"
        };
        AccountStatus {
            state: state.into(),
            login: s.info.as_ref().map(|i| i.login.clone()),
            user_id: s.info.as_ref().map(|i| i.user_id.clone()),
            missing_scopes: auth::missing_scopes(&scopes, required),
            scopes,
            expires_at: s.tokens.as_ref().map(|t| t.expires_at),
            error: s.error.clone(),
            device: s.device.clone(),
        }
    }

    /// Восстановить из конфига логины (для показа до первой валидации).
    pub fn seed_info(&self, kind: AccountKind, info: Option<AccountInfo>) {
        let kind = self.k(kind);
        if let Some(i) = info {
            let mut slots = self.slots.lock();
            let slot = slots.get_mut(&kind).expect("slot");
            if slot.info.is_none() && slot.tokens.is_some() {
                slot.info = Some(i);
            }
        }
    }

    fn store(&self, kind: AccountKind, pair: TokenPair) {
        if let Err(e) = self.secrets.set(kind, &pair) {
            tracing::error!(target: "signorebot::auth", "Не удалось сохранить токен {}: {e}", kind.label());
        }
        let mut slots = self.slots.lock();
        let slot = slots.get_mut(&kind).expect("slot");
        slot.tokens = Some(pair);
        slot.invalid = false;
        slot.error = None;
    }

    /// Проверить токен через `/validate`, при необходимости обновить.
    /// Возвращает `true`, если аккаунт готов к работе.
    pub async fn validate_or_refresh(&self, kind: AccountKind) -> bool {
        let required = self.required_scopes_for(kind);
        let kind = self.k(kind);
        let Some(pair) = self.slots.lock().get(&kind).and_then(|s| s.tokens.clone()) else {
            return false;
        };
        match auth::validate(&pair.access_token).await {
            Ok(v) => {
                let info = AccountInfo { login: v.login.clone(), user_id: v.user_id.clone(), display_name: v.login.clone() };
                {
                    let mut slots = self.slots.lock();
                    let slot = slots.get_mut(&kind).expect("slot");
                    slot.info = Some(info);
                    slot.invalid = false;
                    slot.error = None;
                    if let Some(t) = &mut slot.tokens {
                        t.expires_at = chrono::Utc::now().timestamp() + v.expires_in;
                        if t.scopes.is_empty() {
                            t.scopes = v.scopes.clone();
                        }
                    }
                }
                let missing = auth::missing_scopes(&v.scopes, required);
                if missing.is_empty() {
                    tracing::info!(target: "signorebot::auth", "Токен {} действителен: {} (истекает через {} мин)",
                        kind.label(), v.login, v.expires_in / 60);
                } else {
                    tracing::warn!(target: "signorebot::auth", "Токену {} ({}) не хватает прав: {}. Переавторизуйте аккаунт.",
                        kind.label(), v.login, missing.join(", "));
                }
                // Если истекает скоро — обновим заранее.
                if v.expires_in < 600 {
                    let _ = self.refresh(kind).await;
                }
                self.notify(kind);
                true
            }
            Err(AuthError::Invalid) => {
                tracing::info!(target: "signorebot::auth", "Токен {} недействителен, обновляем…", kind.label());
                let ok = self.refresh(kind).await.is_ok();
                if ok {
                    // После refresh узнаём login/user_id.
                    return Box::pin(self.validate_or_refresh(kind)).await;
                }
                self.notify(kind);
                false
            }
            Err(e) => {
                tracing::warn!(target: "signorebot::auth", "Не удалось проверить токен {}: {e}", kind.label());
                let mut slots = self.slots.lock();
                slots.get_mut(&kind).expect("slot").error = Some(e.to_string());
                drop(slots);
                self.notify(kind);
                // Сеть недоступна — считаем, что токен, возможно, ещё жив.
                self.slots.lock().get(&kind).map(|s| s.info.is_some()).unwrap_or(false)
            }
        }
    }

    /// Обновить токен. Ошибка `Invalid` помечает аккаунт как требующий
    /// переавторизации; сетевые ошибки — временные.
    pub async fn refresh(&self, kind: AccountKind) -> Result<(), AuthError> {
        let kind = self.k(kind);
        let _guard = self.refresh_locks[&kind].lock().await;
        let Some(mut pair) = self.slots.lock().get(&kind).and_then(|s| s.tokens.clone()) else {
            return Err(AuthError::Invalid);
        };
        if let Some(stored) = self.adopt_stored_if_rotated(kind, &pair) {
            pair = stored;
        }
        let client_id = self.client_id();
        match auth::refresh(&client_id, &pair.refresh_token).await {
            Ok(t) => {
                let new_pair = TokenPair {
                    access_token: t.access_token,
                    refresh_token: t.refresh_token,
                    expires_at: chrono::Utc::now().timestamp() + t.expires_in,
                    scopes: if t.scope.is_empty() { pair.scopes } else { t.scope },
                };
                self.store(kind, new_pair);
                tracing::info!(target: "signorebot::auth", "Токен {} обновлён", kind.label());
                self.notify(kind);
                Ok(())
            }
            Err(AuthError::Invalid) => {
                tracing::error!(target: "signorebot::auth",
                    "Refresh-токен {} отклонён Twitch. Нужна повторная авторизация.", kind.label());
                let mut slots = self.slots.lock();
                let slot = slots.get_mut(&kind).expect("slot");
                slot.invalid = true;
                slot.error = Some("Требуется повторная авторизация".into());
                drop(slots);
                self.notify(kind);
                Err(AuthError::Invalid)
            }
            Err(e) => {
                tracing::warn!(target: "signorebot::auth", "Не удалось обновить токен {}: {e}", kind.label());
                self.slots.lock().get_mut(&kind).expect("slot").error = Some(e.to_string());
                self.notify(kind);
                Err(e)
            }
        }
    }

    /// Refresh-токен одноразовый. Если хранилище уже содержит другую пару
    /// (её обновил другой процесс — вторая копия приложения, или запись
    /// пережила перезапуск), наша в памяти протухла: берём свежую из
    /// хранилища, а не выкидываем аккаунт. Возвращает принятую пару.
    fn adopt_stored_if_rotated(&self, kind: AccountKind, current: &TokenPair) -> Option<TokenPair> {
        let stored = self.secrets.get(kind).ok().flatten()?;
        if stored.refresh_token.is_empty() || stored.refresh_token == current.refresh_token {
            return None;
        }
        tracing::warn!(target: "signorebot::auth", "Токен {} в хранилище отличается от токена в памяти (обновлён другим процессом?) — беру из хранилища", kind.label());
        if let Some(slot) = self.slots.lock().get_mut(&kind) {
            slot.tokens = Some(stored.clone());
        }
        Some(stored)
    }

    /// Действующий access-токен (обновляется, если истекает в ближайшие 5 минут).
    pub async fn access_token(&self, kind: AccountKind) -> Result<String, AuthError> {
        let kind = self.k(kind);
        let pair = self.slots.lock().get(&kind).and_then(|s| s.tokens.clone()).ok_or(AuthError::Invalid)?;
        let now = chrono::Utc::now().timestamp();
        if pair.expires_at > 0 && pair.expires_at - now < 300 {
            self.refresh(kind).await?;
            return self.slots.lock().get(&kind).and_then(|s| s.tokens.as_ref().map(|t| t.access_token.clone())).ok_or(AuthError::Invalid);
        }
        Ok(pair.access_token)
    }

    /// Helix ответил 401 — токен внезапно протух: обновить и вернуть новый.
    pub async fn on_unauthorized(&self, kind: AccountKind) -> Result<String, AuthError> {
        let kind = self.k(kind);
        self.refresh(kind).await?;
        self.slots.lock().get(&kind).and_then(|s| s.tokens.as_ref().map(|t| t.access_token.clone())).ok_or(AuthError::Invalid)
    }

    /// Начать Device-Code-авторизацию. Возвращает код для пользователя;
    /// опрос идёт в фоне до успеха/отказа/истечения.
    pub async fn start_device_flow(self: &Arc<Self>, kind: AccountKind) -> Result<DeviceCode, AuthError> {
        let kind = self.k(kind);
        // Стримеру всегда запрашиваем объединённый набор (его права + права
        // бота): тогда режим «один аккаунт» включается без повторной
        // авторизации. Обязательными права бота становятся только в этом режиме.
        let scopes = if kind == AccountKind::Broadcaster { union_scopes() } else { auth::BOT_SCOPES };
        let client_id = self.client_id();
        let dc = auth::device_code(&client_id, scopes).await?;
        {
            let mut slots = self.slots.lock();
            let slot = slots.get_mut(&kind).expect("slot");
            slot.device = Some(dc.clone());
            slot.error = None;
        }
        self.notify(kind);
        tracing::info!(target: "signorebot::auth", "Авторизация {}: откройте {} и введите код {}",
            kind.label(), dc.verification_uri, dc.user_code);

        let me = Arc::clone(self);
        let dc2 = dc.clone();
        tokio::spawn(async move {
            let interval = std::time::Duration::from_secs(dc2.interval.max(1));
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(dc2.expires_in);
            loop {
                tokio::time::sleep(interval).await;
                // отменено пользователем?
                if me.slots.lock().get(&kind).and_then(|s| s.device.as_ref()).map(|d| d.device_code != dc2.device_code).unwrap_or(true) {
                    return;
                }
                if std::time::Instant::now() > deadline {
                    me.finish_device(kind, Err("Код устарел, начните авторизацию заново".into()));
                    return;
                }
                match auth::poll_device_token(&client_id, &dc2.device_code).await {
                    Ok(t) => {
                        let pair = TokenPair {
                            access_token: t.access_token,
                            refresh_token: t.refresh_token,
                            expires_at: chrono::Utc::now().timestamp() + t.expires_in,
                            scopes: t.scope,
                        };
                        me.store(kind, pair);
                        me.finish_device(kind, Ok(()));
                        let ok = me.validate_or_refresh(kind).await;
                        if ok {
                            tracing::info!(target: "signorebot::auth", "Аккаунт {} авторизован", kind.label());
                        }
                        return;
                    }
                    Err(AuthError::Pending) => continue,
                    Err(AuthError::Denied) => {
                        me.finish_device(kind, Err("Авторизация отклонена".into()));
                        return;
                    }
                    Err(AuthError::Expired) => {
                        me.finish_device(kind, Err("Код устарел, начните авторизацию заново".into()));
                        return;
                    }
                    Err(e) => {
                        tracing::warn!(target: "signorebot::auth", "Опрос авторизации {}: {e}", kind.label());
                        continue;
                    }
                }
            }
        });
        Ok(dc)
    }

    fn finish_device(&self, kind: AccountKind, result: Result<(), String>) {
        {
            let mut slots = self.slots.lock();
            let slot = slots.get_mut(&kind).expect("slot");
            slot.device = None;
            if let Err(e) = result {
                slot.error = Some(e);
            }
        }
        self.notify(kind);
    }

    pub fn cancel_device_flow(&self, kind: AccountKind) {
        let kind = self.k(kind);
        self.finish_device(kind, Ok(()));
    }

    /// Выйти: отозвать токен, стереть из хранилища.
    pub async fn logout(&self, kind: AccountKind) {
        let kind = self.k(kind);
        let pair = self.slots.lock().get(&kind).and_then(|s| s.tokens.clone());
        if let Some(p) = pair {
            let _ = auth::revoke(&self.client_id(), &p.access_token).await;
        }
        let _ = self.secrets.delete(kind);
        {
            let mut slots = self.slots.lock();
            let slot = slots.get_mut(&kind).expect("slot");
            *slot = Slot::default();
        }
        tracing::info!(target: "signorebot::auth", "Аккаунт {} отключён", kind.label());
        self.notify(kind);
    }

    /// Секунды до истечения access-токена (для планировщика).
    pub fn seconds_until_expiry(&self, kind: AccountKind) -> Option<i64> {
        let kind = self.k(kind);
        self.slots.lock().get(&kind).and_then(|s| s.tokens.as_ref()).map(|t| t.expires_at - chrono::Utc::now().timestamp())
    }
}

/// Фоновая задача: держит токены свежими (обновление за 5 минут до
/// истечения, повтор при сетевых ошибках через минуту).
pub async fn refresh_loop(auth: Arc<AuthManager>) {
    loop {
        let mut wait = 60i64 * 30;
        for kind in [AccountKind::Broadcaster, AccountKind::Bot] {
            if !auth.has_tokens(kind) {
                continue;
            }
            match auth.seconds_until_expiry(kind) {
                Some(s) if s < 300 => {
                    if auth.refresh(kind).await.is_err() {
                        wait = wait.min(60);
                    }
                }
                Some(s) => wait = wait.min(s - 300),
                None => {}
            }
        }
        tokio::time::sleep(std::time::Duration::from_secs(wait.clamp(30, 1800) as u64)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::AppPaths;

    #[test]
    fn shared_mode_routes_bot_to_broadcaster_slot() {
        let dir = tempfile::tempdir().unwrap();
        let secrets = Secrets::file_only(&AppPaths::new(dir.path()));
        let auth = AuthManager::new("cid".into(), secrets);
        let pair = TokenPair { access_token: "a".into(), refresh_token: "r".into(), expires_at: 10, scopes: auth::BROADCASTER_SCOPES.iter().map(|s| s.to_string()).collect() };
        auth.store(AccountKind::Broadcaster, pair);
        auth.slots.lock().get_mut(&AccountKind::Broadcaster).unwrap().info = Some(AccountInfo { login: "cozy".into(), user_id: "1".into(), display_name: "cozy".into() });
        // обычный режим: бот пуст
        assert!(!auth.has_tokens(AccountKind::Bot));
        assert!(!auth.both_ready());
        // один аккаунт: бот читает слот стримера, но стримеру не хватает прав бота
        auth.set_shared(true);
        assert!(auth.has_tokens(AccountKind::Bot));
        assert_eq!(auth.info(AccountKind::Bot).unwrap().user_id, "1");
        assert!(auth.both_ready());
        let st = auth.status(AccountKind::Broadcaster);
        assert!(st.missing_scopes.contains(&"user:write:chat".to_string()));
        assert!(st.missing_scopes.contains(&"moderator:manage:chat_messages".to_string()));
        assert_eq!(auth.required_scopes_for(AccountKind::Broadcaster).len(), 10);
        // выключили — бот снова отдельный и пустой
        auth.set_shared(false);
        assert!(!auth.has_tokens(AccountKind::Bot));
        assert!(auth.status(AccountKind::Broadcaster).missing_scopes.is_empty());
    }

    #[test]
    fn rotated_token_in_store_is_adopted() {
        let dir = tempfile::tempdir().unwrap();
        let secrets = Secrets::file_only(&AppPaths::new(dir.path()));
        let auth = AuthManager::new("cid".into(), secrets.clone());
        let old = TokenPair { access_token: "a1".into(), refresh_token: "r1".into(), expires_at: 10, scopes: vec![] };
        auth.store(AccountKind::Bot, old.clone());
        // хранилище совпадает с памятью — ничего не меняем
        assert!(auth.adopt_stored_if_rotated(AccountKind::Bot, &old).is_none());
        // другой процесс обновил токен
        let fresh = TokenPair { access_token: "a2".into(), refresh_token: "r2".into(), expires_at: 20, scopes: vec![] };
        secrets.set(AccountKind::Bot, &fresh).unwrap();
        let adopted = auth.adopt_stored_if_rotated(AccountKind::Bot, &old).unwrap();
        assert_eq!(adopted, fresh);
        assert_eq!(auth.slots.lock()[&AccountKind::Bot].tokens, Some(fresh));
    }
}
