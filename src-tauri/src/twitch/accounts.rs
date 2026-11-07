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
    /// Попытка входа хранится отдельно от действующей авторизации.
    pub attempt: Option<LoginAttempt>,
}

#[derive(Debug, Clone, Serialize, ts_rs::TS, Default)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "api.ts")]
pub struct LoginAttempt {
    pub device: Option<DeviceCode>,
    pub error: Option<String>,
    /// Twitch ID совпал со второй ролью. Токены кандидата не приняты.
    pub conflicting_login: Option<String>,
}

#[derive(Debug, Clone, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "api.ts")]
pub struct AuthSetupStatus {
    pub same_account: bool,
    pub selected_same_account: bool,
    /// Неперенаправленный слот бота, включая незавершённый вход.
    pub bot: AccountStatus,
    pub broadcaster: AccountStatus,
    /// Старая конфигурация с двумя входами одним Twitch ID.
    pub duplicate_accounts: bool,
}

#[derive(Default)]
struct Slot {
    tokens: Option<TokenPair>,
    info: Option<AccountInfo>,
    error: Option<String>,
    attempt: Option<LoginAttempt>,
    validated: bool,
    /// Токен признан недействительным и обновить не удалось.
    invalid: bool,
}

/// Уведомления об изменении состояния аккаунтов.
#[derive(Debug, Clone)]
pub enum AuthEvent {
    Changed(AccountKind),
}

struct AuthState {
    slots: HashMap<AccountKind, Slot>,
    shared: bool,
    selected_shared: bool,
    /// Меняется при выборе режима, отмене и начале нового входа.
    revision: u64,
}

pub struct AuthManager {
    client_id: Mutex<String>,
    secrets: Secrets,
    state: Mutex<AuthState>,
    /// Сериализует refresh для каждого аккаунта (refresh-токен одноразовый!).
    refresh_locks: HashMap<AccountKind, AsyncMutex<()>>,
    tx: broadcast::Sender<AuthEvent>,
}

/// Права стримера вместе с правами бота: запрашиваются у стримера всегда,
/// обязательны — только в режиме «один аккаунт».
/// Обязательные права в общем режиме: стример + бот, без необязательных.
fn required_union_scopes() -> &'static [&'static str] {
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

fn union_scopes() -> &'static [&'static str] {
    static UNION: std::sync::OnceLock<Vec<&'static str>> = std::sync::OnceLock::new();
    UNION.get_or_init(|| {
        let mut v: Vec<&'static str> = auth::BROADCASTER_SCOPES.to_vec();
        for s in auth::BROADCASTER_OPTIONAL_SCOPES {
            if !v.contains(s) {
                v.push(s);
            }
        }
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
        Arc::new(Self {
            client_id: Mutex::new(client_id),
            secrets,
            state: Mutex::new(AuthState {
                slots,
                shared: false,
                selected_shared: false,
                revision: 0,
            }),
            refresh_locks,
            tx,
        })
    }

    /// Есть ли у токена роли конкретное право.
    pub fn has_scope(&self, kind: AccountKind, scope: &str) -> bool {
        let kind = self.k(kind);
        self.state
            .lock()
            .slots
            .get(&kind)
            .and_then(|s| s.tokens.as_ref())
            .map(|t| t.scopes.iter().any(|x| x == scope))
            .unwrap_or(false)
    }

    pub fn is_shared(&self) -> bool {
        self.state.lock().shared
    }
    /// Восстановить сохранённый режим при запуске, без изменения аккаунтов.
    pub fn set_shared(&self, on: bool) {
        let mut state = self.state.lock();
        state.shared = on;
        state.selected_shared = on;
        state.revision += 1;
        drop(state);
        self.notify_all();
    }

    /// Выбор режима — намерение пользователя. Рабочий отправитель меняется
    /// только тогда, когда целевой набор аккаунтов уже проверен.
    pub fn select_mode(&self, on: bool) {
        let mut state = self.state.lock();
        state.revision += 1;
        for slot in state.slots.values_mut() {
            slot.attempt = None;
        }
        state.selected_shared = on;
        Self::apply_selected_mode(&mut state);
        drop(state);
        self.notify_all();
    }

    fn slot_ready(slot: &Slot, required: &[&str]) -> bool {
        slot.validated
            && !slot.invalid
            && slot.info.is_some()
            && slot
                .tokens
                .as_ref()
                .is_some_and(|t| t.expires_at > chrono::Utc::now().timestamp() && auth::missing_scopes(&t.scopes, required).is_empty())
    }

    fn different_accounts(state: &AuthState) -> bool {
        match (&state.slots[&AccountKind::Broadcaster].info, &state.slots[&AccountKind::Bot].info) {
            (Some(a), Some(b)) => a.user_id != b.user_id,
            _ => false,
        }
    }

    fn apply_selected_mode(state: &mut AuthState) {
        let b = &state.slots[&AccountKind::Broadcaster];
        if b.tokens.is_none() {
            state.shared = state.selected_shared;
        } else if state.selected_shared {
            if Self::slot_ready(b, required_union_scopes()) {
                state.shared = true;
            }
        } else if !state.shared
            || (Self::slot_ready(b, auth::BROADCASTER_SCOPES) && Self::slot_ready(&state.slots[&AccountKind::Bot], auth::BOT_SCOPES) && Self::different_accounts(state))
        {
            state.shared = false;
        }
    }

    /// Реальный слот для роли: в общем режиме бот читает слот стримера.
    fn k(&self, kind: AccountKind) -> AccountKind {
        if kind == AccountKind::Bot && self.is_shared() {
            AccountKind::Broadcaster
        } else {
            kind
        }
    }
    pub fn required_scopes_for(&self, kind: AccountKind) -> &'static [&'static str] {
        if self.is_shared() {
            required_union_scopes()
        } else {
            Self::required_scopes(kind)
        }
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

    pub fn stored_info(&self, kind: AccountKind) -> Option<AccountInfo> {
        self.state.lock().slots.get(&kind).and_then(|s| s.info.clone())
    }

    pub fn info(&self, kind: AccountKind) -> Option<AccountInfo> {
        let kind = self.k(kind);
        self.state.lock().slots.get(&kind).and_then(|s| s.info.clone())
    }
    pub fn has_tokens(&self, kind: AccountKind) -> bool {
        let kind = self.k(kind);
        self.state.lock().slots.get(&kind).map(|s| s.tokens.is_some()).unwrap_or(false)
    }
    pub fn is_ready(&self, kind: AccountKind) -> bool {
        let state = self.state.lock();
        let real = if state.shared { AccountKind::Broadcaster } else { kind };
        let required = if state.shared { required_union_scopes() } else { Self::required_scopes(kind) };
        Self::slot_ready(&state.slots[&real], required)
    }
    /// В отдельном режиме два одинаковых ID никогда не запускают рантайм,
    /// в том числе если такие токены остались от старой версии.
    pub fn both_ready(&self) -> bool {
        let state = self.state.lock();
        if state.shared {
            Self::slot_ready(&state.slots[&AccountKind::Broadcaster], required_union_scopes())
        } else {
            Self::different_accounts(&state)
                && Self::slot_ready(&state.slots[&AccountKind::Broadcaster], auth::BROADCASTER_SCOPES)
                && Self::slot_ready(&state.slots[&AccountKind::Bot], auth::BOT_SCOPES)
        }
    }

    fn slot_status(slot: &Slot, required: &[&str]) -> AccountStatus {
        let scopes = slot.tokens.as_ref().map(|t| t.scopes.clone()).unwrap_or_default();
        let device = slot.attempt.as_ref().and_then(|a| a.device.clone());
        let state = if slot.invalid {
            "invalid"
        } else if slot.validated && slot.tokens.is_some() && slot.info.is_some() {
            "authorized"
        } else if device.is_some() {
            "pending"
        } else {
            "none"
        };
        AccountStatus {
            state: state.into(),
            login: slot.info.as_ref().map(|i| i.login.clone()),
            user_id: slot.info.as_ref().map(|i| i.user_id.clone()),
            missing_scopes: auth::missing_scopes(&scopes, required),
            scopes,
            expires_at: slot.tokens.as_ref().map(|t| t.expires_at),
            error: slot.error.clone(),
            device,
            attempt: slot.attempt.clone(),
        }
    }

    pub fn status(&self, kind: AccountKind) -> AccountStatus {
        let state = self.state.lock();
        let real = if state.shared { AccountKind::Broadcaster } else { kind };
        let required = if state.shared { required_union_scopes() } else { Self::required_scopes(kind) };
        Self::slot_status(&state.slots[&real], required)
    }

    pub fn setup_status(&self) -> AuthSetupStatus {
        let state = self.state.lock();
        let b = &state.slots[&AccountKind::Broadcaster];
        let o = &state.slots[&AccountKind::Bot];
        AuthSetupStatus {
            same_account: state.shared,
            selected_same_account: state.selected_shared,
            bot: Self::slot_status(o, auth::BOT_SCOPES),
            broadcaster: Self::slot_status(b, if state.selected_shared { required_union_scopes() } else { auth::BROADCASTER_SCOPES }),
            duplicate_accounts: !state.selected_shared && b.tokens.is_some() && o.tokens.is_some() && b.info.is_some() && o.info.is_some() && !Self::different_accounts(&state),
        }
    }

    /// Восстановить из конфига логины (для показа до первой валидации).
    pub fn seed_info(&self, kind: AccountKind, info: Option<AccountInfo>) {
        if let Some(i) = info {
            let mut state = self.state.lock();
            let slots = &mut state.slots;
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
        let mut state = self.state.lock();
        let slots = &mut state.slots;
        let slot = slots.get_mut(&kind).expect("slot");
        slot.tokens = Some(pair);
        slot.invalid = false;
        slot.error = None;
    }

    /// Проверить токен через `/validate`, при необходимости обновить.
    /// Возвращает `true`, если аккаунт готов к работе.
    pub async fn validate_or_refresh(&self, kind: AccountKind) -> bool {
        self.validate_slot(self.k(kind)).await
    }

    async fn validate_slot(&self, kind: AccountKind) -> bool {
        let required = if kind == AccountKind::Broadcaster && self.is_shared() {
            required_union_scopes()
        } else {
            Self::required_scopes(kind)
        };
        let Some(pair) = self.state.lock().slots.get(&kind).and_then(|s| s.tokens.clone()) else {
            return false;
        };
        match auth::validate(&pair.access_token).await {
            Ok(v) => {
                let info = AccountInfo {
                    login: v.login.clone(),
                    user_id: v.user_id.clone(),
                    display_name: v.login.clone(),
                };
                {
                    let mut state = self.state.lock();
                    let slots = &mut state.slots;
                    let slot = slots.get_mut(&kind).expect("slot");
                    if slot.tokens.as_ref() != Some(&pair) {
                        return false;
                    }
                    slot.info = Some(info);
                    slot.invalid = false;
                    slot.validated = true;
                    slot.error = None;
                    if let Some(t) = &mut slot.tokens {
                        t.expires_at = chrono::Utc::now().timestamp() + v.expires_in;
                        t.scopes = v.scopes.clone();
                    }
                    Self::apply_selected_mode(&mut state);
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
                    let _ = self.refresh_slot(kind).await;
                }
                self.notify(kind);
                true
            }
            Err(AuthError::Invalid) => {
                tracing::info!(target: "signorebot::auth", "Токен {} недействителен, обновляем…", kind.label());
                let ok = self.refresh_slot(kind).await.is_ok();
                if ok {
                    // После refresh узнаём login/user_id.
                    return Box::pin(self.validate_slot(kind)).await;
                }
                self.notify(kind);
                false
            }
            Err(e) => {
                tracing::warn!(target: "signorebot::auth", "Не удалось проверить токен {}: {e}", kind.label());
                let mut state = self.state.lock();
                let slots = &mut state.slots;
                slots.get_mut(&kind).expect("slot").error = Some(e.to_string());
                drop(state);
                self.notify(kind);
                // Сеть недоступна — считаем, что токен, возможно, ещё жив.
                self.state.lock().slots.get(&kind).map(|s| s.info.is_some()).unwrap_or(false)
            }
        }
    }

    /// Обновить токен. Ошибка `Invalid` помечает аккаунт как требующий
    /// переавторизации; сетевые ошибки — временные.
    pub async fn refresh(&self, kind: AccountKind) -> Result<(), AuthError> {
        self.refresh_slot(self.k(kind)).await
    }

    /// Управление входом в панели адресует сам аккаунт, а не текущую роль.
    pub async fn refresh_account(&self, kind: AccountKind) -> Result<(), AuthError> {
        self.refresh_slot(kind).await?;
        self.validate_slot(kind).await;
        Ok(())
    }

    async fn refresh_slot(&self, kind: AccountKind) -> Result<(), AuthError> {
        let _guard = self.refresh_locks[&kind].lock().await;
        let Some(mut pair) = self.state.lock().slots.get(&kind).and_then(|s| s.tokens.clone()) else {
            return Err(AuthError::Invalid);
        };
        if let Some(stored) = self.adopt_stored_if_rotated(kind, &pair) {
            pair = stored;
        }
        let client_id = self.client_id();
        match auth::refresh(&client_id, &pair.refresh_token).await {
            Ok(t) => {
                if self.state.lock().slots[&kind].tokens.as_ref() != Some(&pair) {
                    return Ok(());
                }
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
                if self.state.lock().slots[&kind].tokens.as_ref() != Some(&pair) {
                    return Ok(());
                }
                tracing::error!(target: "signorebot::auth",
                    "Refresh-токен {} отклонён Twitch. Нужна повторная авторизация.", kind.label());
                let mut state = self.state.lock();
                let slots = &mut state.slots;
                let slot = slots.get_mut(&kind).expect("slot");
                slot.invalid = true;
                slot.error = Some("Требуется повторная авторизация".into());
                drop(state);
                self.notify(kind);
                Err(AuthError::Invalid)
            }
            Err(e) => {
                tracing::warn!(target: "signorebot::auth", "Не удалось обновить токен {}: {e}", kind.label());
                self.state.lock().slots.get_mut(&kind).expect("slot").error = Some(e.to_string());
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
        if let Some(slot) = self.state.lock().slots.get_mut(&kind) {
            slot.tokens = Some(stored.clone());
        }
        Some(stored)
    }

    /// Действующий access-токен (обновляется, если истекает в ближайшие 5 минут).
    pub async fn access_token(&self, kind: AccountKind) -> Result<String, AuthError> {
        let kind = self.k(kind);
        let pair = self.state.lock().slots.get(&kind).and_then(|s| s.tokens.clone()).ok_or(AuthError::Invalid)?;
        let now = chrono::Utc::now().timestamp();
        if pair.expires_at > 0 && pair.expires_at - now < 300 {
            self.refresh_slot(kind).await?;
            return self
                .state
                .lock()
                .slots
                .get(&kind)
                .and_then(|s| s.tokens.as_ref().map(|t| t.access_token.clone()))
                .ok_or(AuthError::Invalid);
        }
        Ok(pair.access_token)
    }

    /// Helix ответил 401 — токен внезапно протух: обновить и вернуть новый.
    pub async fn on_unauthorized(&self, kind: AccountKind) -> Result<String, AuthError> {
        let kind = self.k(kind);
        self.refresh_slot(kind).await?;
        self.state
            .lock()
            .slots
            .get(&kind)
            .and_then(|s| s.tokens.as_ref().map(|t| t.access_token.clone()))
            .ok_or(AuthError::Invalid)
    }

    fn notify_all(&self) {
        let _ = self.tx.send(AuthEvent::Changed(AccountKind::Broadcaster));
        let _ = self.tx.send(AuthEvent::Changed(AccountKind::Bot));
    }

    fn begin_attempt(&self, kind: AccountKind) -> Result<u64, AuthError> {
        let mut state = self.state.lock();
        if kind == AccountKind::Bot && (state.selected_shared || !Self::slot_ready(&state.slots[&AccountKind::Broadcaster], auth::BROADCASTER_SCOPES)) {
            return Err(AuthError::Twitch("Сначала подключите канал стримера и выберите отдельный аккаунт бота".into()));
        }
        state.revision += 1;
        for slot in state.slots.values_mut() {
            slot.attempt = None;
        }
        state.slots.get_mut(&kind).unwrap().attempt = Some(LoginAttempt::default());
        Ok(state.revision)
    }

    fn attempt_current(state: &AuthState, kind: AccountKind, revision: u64) -> bool {
        state.revision == revision && state.slots[&kind].attempt.is_some()
    }

    fn fail_attempt(&self, kind: AccountKind, revision: u64, error: String) {
        let mut state = self.state.lock();
        if !Self::attempt_current(&state, kind, revision) {
            return;
        }
        let attempt = state.slots.get_mut(&kind).unwrap().attempt.as_mut().unwrap();
        attempt.device = None;
        attempt.error = Some(error);
        drop(state);
        self.notify_all();
    }

    /// Только проверенный кандидат может заменить действующие токены.
    /// Проверка ID симметрична: защищает и замену стримера аккаунтом бота.
    fn accept_candidate(&self, kind: AccountKind, revision: u64, mut pair: TokenPair, validated: auth::ValidateResponse) -> Result<bool, String> {
        let client_id = self.client_id();
        let mut state = self.state.lock();
        if !Self::attempt_current(&state, kind, revision) {
            return Ok(false);
        }
        let required = if kind == AccountKind::Broadcaster {
            required_union_scopes()
        } else {
            Self::required_scopes(kind)
        };
        let error = if validated.client_id != client_id || validated.user_id.is_empty() || validated.login.is_empty() || validated.expires_in <= 0 {
            Some("Twitch вернул некорректные данные аккаунта. Повторите вход".to_string())
        } else if !auth::missing_scopes(&validated.scopes, required).is_empty() {
            Some(format!(
                "Не хватает прав: {}. Повторите вход и подтвердите разрешения Twitch",
                auth::missing_scopes(&validated.scopes, required).join(", ")
            ))
        } else if !state.selected_shared {
            let other = if kind == AccountKind::Bot { AccountKind::Broadcaster } else { AccountKind::Bot };
            if state.slots[&other].tokens.is_some() && state.slots[&other].info.as_ref().is_some_and(|i| i.user_id == validated.user_id) {
                let attempt = state.slots.get_mut(&kind).unwrap().attempt.as_mut().unwrap();
                attempt.conflicting_login = Some(validated.login.clone());
                Some("Для отдельного бота нужен другой Twitch-аккаунт".to_string())
            } else if kind == AccountKind::Bot && !Self::slot_ready(&state.slots[&AccountKind::Broadcaster], auth::BROADCASTER_SCOPES) {
                Some("Сначала подключите канал стримера".to_string())
            } else {
                None
            }
        } else {
            None
        };
        if let Some(error) = error {
            let attempt = state.slots.get_mut(&kind).unwrap().attempt.as_mut().unwrap();
            attempt.device = None;
            attempt.error = Some(error.clone());
            drop(state);
            self.notify_all();
            return Err(error);
        }
        pair.scopes = validated.scopes;
        pair.expires_at = chrono::Utc::now().timestamp() + validated.expires_in;
        // Сначала надёжно сохраняем, затем публикуем новую авторизацию.
        if let Err(e) = self.secrets.set(kind, &pair) {
            drop(state);
            let error = format!("Не удалось сохранить авторизацию: {e}. Повторите вход");
            self.fail_attempt(kind, revision, error.clone());
            return Err(error);
        }
        let slot = state.slots.get_mut(&kind).unwrap();
        slot.tokens = Some(pair);
        slot.info = Some(AccountInfo {
            login: validated.login.clone(),
            user_id: validated.user_id,
            display_name: validated.login,
        });
        slot.invalid = false;
        slot.validated = true;
        slot.error = None;
        slot.attempt = None;
        Self::apply_selected_mode(&mut state);
        drop(state);
        self.notify_all();
        Ok(true)
    }

    /// Вход всегда адресуется явно указанной роли, даже если пока работает
    /// общий аккаунт. Активный отправитель остаётся прежним до принятия кандидата.
    pub async fn start_device_flow(self: &Arc<Self>, kind: AccountKind) -> Result<DeviceCode, AuthError> {
        let revision = self.begin_attempt(kind)?;
        self.notify_all();
        let scopes = if kind == AccountKind::Broadcaster { union_scopes() } else { auth::BOT_SCOPES };
        let client_id = self.client_id();
        let dc = match auth::device_code(&client_id, scopes).await {
            Ok(dc) => dc,
            Err(e) => {
                self.fail_attempt(kind, revision, e.to_string());
                return Err(e);
            }
        };
        {
            let mut state = self.state.lock();
            if !Self::attempt_current(&state, kind, revision) {
                return Err(AuthError::Expired);
            }
            state.slots.get_mut(&kind).unwrap().attempt.as_mut().unwrap().device = Some(dc.clone());
        }
        self.notify_all();
        let me = Arc::clone(self);
        let dc2 = dc.clone();
        tokio::spawn(async move {
            let interval = std::time::Duration::from_secs(dc2.interval.max(1));
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(dc2.expires_in);
            loop {
                tokio::time::sleep(interval).await;
                if !Self::attempt_current(&me.state.lock(), kind, revision) {
                    return;
                }
                if std::time::Instant::now() > deadline {
                    me.fail_attempt(kind, revision, "Код устарел, начните вход заново".into());
                    return;
                }
                match auth::poll_device_token(&client_id, &dc2.device_code).await {
                    Ok(t) => {
                        let pair = TokenPair {
                            access_token: t.access_token,
                            refresh_token: t.refresh_token,
                            expires_at: 0,
                            scopes: t.scope,
                        };
                        // Кандидат не попадает ни в runtime, ни в хранилище до /validate.
                        match auth::validate(&pair.access_token).await {
                            Ok(v) => {
                                let _guard = me.refresh_locks[&kind].lock().await;
                                let _ = me.accept_candidate(kind, revision, pair, v);
                            }
                            Err(e) => me.fail_attempt(kind, revision, format!("Не удалось проверить новый вход: {e}. Повторите вход")),
                        }
                        return;
                    }
                    Err(AuthError::Pending) => continue,
                    Err(AuthError::Denied) => {
                        me.fail_attempt(kind, revision, "Вход отклонён на Twitch. Повторите вход".into());
                        return;
                    }
                    Err(AuthError::Expired) => {
                        me.fail_attempt(kind, revision, "Код устарел, начните вход заново".into());
                        return;
                    }
                    Err(e) => {
                        tracing::warn!(target: "signorebot::auth", "Опрос авторизации {}: {e}", kind.label());
                    }
                }
            }
        });
        Ok(dc)
    }

    pub fn cancel_device_flow(&self, _kind: AccountKind) {
        let mut state = self.state.lock();
        state.revision += 1;
        for slot in state.slots.values_mut() {
            slot.attempt = None;
        }
        drop(state);
        self.notify_all();
    }

    /// Убрать аккаунт и все ожидающие входы до сетевого отзыва токена.
    /// Вызывается под refresh-lock, чтобы завершившийся refresh не вернул токен.
    fn clear_account(&self, kind: AccountKind) -> Option<TokenPair> {
        let mut state = self.state.lock();
        state.revision += 1;
        for slot in state.slots.values_mut() {
            slot.attempt = None;
        }
        let slot = state.slots.get_mut(&kind).expect("slot");
        let pair = slot.tokens.take();
        *slot = Slot::default();
        let _ = self.secrets.delete(kind);
        Self::apply_selected_mode(&mut state);
        drop(state);
        self.notify_all();
        pair
    }

    /// Выход сразу отменяет вход бота, даже если нужно подождать refresh.
    /// Локальное состояние очищается до HTTP: медленный Twitch не удерживает
    /// аккаунт подключённым и не позволяет завершить вход зависимого бота.
    pub async fn logout(&self, kind: AccountKind) {
        self.cancel_device_flow(kind);
        let _guard = self.refresh_locks[&kind].lock().await;
        let pair = self.clear_account(kind);
        if let Some(p) = pair {
            let _ = auth::revoke(&self.client_id(), &p.access_token).await;
        }
        tracing::info!(target: "signorebot::auth", "Аккаунт {} отключён", kind.label());
    }

    /// Секунды до истечения access-токена (для планировщика).
    pub fn seconds_until_expiry(&self, kind: AccountKind) -> Option<i64> {
        let kind = self.k(kind);
        self.state
            .lock()
            .slots
            .get(&kind)
            .and_then(|s| s.tokens.as_ref())
            .map(|t| t.expires_at - chrono::Utc::now().timestamp())
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

    fn manager() -> (tempfile::TempDir, Arc<AuthManager>) {
        let dir = tempfile::tempdir().unwrap();
        let auth = AuthManager::new("cid".into(), Secrets::file_only(&AppPaths::new(dir.path())));
        (dir, auth)
    }

    fn candidate(id: &str, kind: AccountKind) -> (TokenPair, auth::ValidateResponse) {
        let scopes: Vec<String> = if kind == AccountKind::Broadcaster { union_scopes() } else { auth::BOT_SCOPES }
            .iter()
            .map(|s| s.to_string())
            .collect();
        (
            TokenPair {
                access_token: format!("access-{id}"),
                refresh_token: format!("refresh-{id}"),
                expires_at: 0,
                scopes: vec![],
            },
            auth::ValidateResponse {
                client_id: "cid".into(),
                login: format!("user{id}"),
                user_id: id.into(),
                scopes,
                expires_in: 3600,
            },
        )
    }

    fn connect(auth: &AuthManager, kind: AccountKind, id: &str) {
        let revision = auth.begin_attempt(kind).unwrap();
        let (pair, v) = candidate(id, kind);
        assert!(auth.accept_candidate(kind, revision, pair, v).unwrap());
    }

    fn legacy_slot(auth: &AuthManager, kind: AccountKind, id: &str) {
        let (pair, v) = candidate(id, kind);
        auth.store(
            kind,
            TokenPair {
                scopes: v.scopes,
                expires_at: chrono::Utc::now().timestamp() + 3600,
                ..pair
            },
        );
        let mut state = auth.state.lock();
        let slot = state.slots.get_mut(&kind).unwrap();
        slot.info = Some(AccountInfo {
            login: format!("user{id}"),
            user_id: id.into(),
            display_name: format!("User{id}"),
        });
        slot.validated = true;
    }

    #[test]
    fn shared_needs_one_verified_login_and_separate_needs_two_distinct_ids() {
        let (_dir, auth) = manager();
        auth.select_mode(true);
        assert!(auth.begin_attempt(AccountKind::Bot).is_err());
        connect(&auth, AccountKind::Broadcaster, "1");
        assert!(auth.both_ready());
        assert_eq!(auth.info(AccountKind::Bot).unwrap().user_id, "1");
        auth.select_mode(false);
        assert!(auth.is_shared()); // продолжаем писать прежним аккаунтом
        assert!(!auth.setup_status().selected_same_account);
        connect(&auth, AccountKind::Bot, "2");
        assert!(!auth.is_shared());
        assert!(auth.both_ready());
        assert_eq!(auth.info(AccountKind::Bot).unwrap().user_id, "2");
    }

    #[test]
    fn duplicate_bot_never_replaces_active_token_or_switches_mode() {
        let (_dir, auth) = manager();
        auth.select_mode(true);
        connect(&auth, AccountKind::Broadcaster, "1");
        auth.select_mode(false);
        let revision = auth.begin_attempt(AccountKind::Bot).unwrap();
        let (pair, v) = candidate("1", AccountKind::Bot);
        assert!(auth.accept_candidate(AccountKind::Bot, revision, pair, v).is_err());
        assert!(auth.is_shared());
        assert!(auth.both_ready());
        assert!(auth.secrets.get(AccountKind::Bot).unwrap().is_none());
        assert_eq!(auth.setup_status().bot.attempt.unwrap().conflicting_login.as_deref(), Some("user1"));
        assert_eq!(auth.secrets.get(AccountKind::Broadcaster).unwrap().unwrap().access_token, "access-1");
        auth.select_mode(true); // явное согласие писать от имени стримера
        assert!(auth.setup_status().bot.attempt.is_none());
    }

    #[test]
    fn duplicate_streamer_is_rejected_when_bot_was_connected_first() {
        let (_dir, auth) = manager();
        legacy_slot(&auth, AccountKind::Bot, "1");
        let revision = auth.begin_attempt(AccountKind::Broadcaster).unwrap();
        let (pair, v) = candidate("1", AccountKind::Broadcaster);
        assert!(auth.accept_candidate(AccountKind::Broadcaster, revision, pair, v).is_err());
        assert!(auth.secrets.get(AccountKind::Broadcaster).unwrap().is_none());
        assert_eq!(auth.info(AccountKind::Bot).unwrap().user_id, "1");
        assert!(!auth.both_ready());
        connect(&auth, AccountKind::Broadcaster, "2");
        assert!(auth.both_ready());
    }

    #[test]
    fn failed_replacement_preserves_existing_separate_accounts_and_secrets() {
        let (_dir, auth) = manager();
        connect(&auth, AccountKind::Broadcaster, "1");
        connect(&auth, AccountKind::Bot, "2");
        for (kind, duplicate) in [(AccountKind::Bot, "1"), (AccountKind::Broadcaster, "2")] {
            let previous = auth.secrets.get(kind).unwrap();
            let revision = auth.begin_attempt(kind).unwrap();
            let (pair, v) = candidate(duplicate, kind);
            assert!(auth.accept_candidate(kind, revision, pair, v).is_err());
            assert_eq!(auth.secrets.get(kind).unwrap(), previous);
            assert!(auth.both_ready());
        }
        assert_eq!(auth.info(AccountKind::Broadcaster).unwrap().user_id, "1");
        assert_eq!(auth.info(AccountKind::Bot).unwrap().user_id, "2");
    }

    #[test]
    fn canceled_or_superseded_result_cannot_be_saved() {
        let (_dir, auth) = manager();
        for change_mode in [false, true] {
            let revision = auth.begin_attempt(AccountKind::Broadcaster).unwrap();
            if change_mode {
                auth.select_mode(true);
            } else {
                auth.cancel_device_flow(AccountKind::Broadcaster);
            }
            let (pair, v) = candidate("1", AccountKind::Broadcaster);
            assert!(!auth.accept_candidate(AccountKind::Broadcaster, revision, pair, v).unwrap());
            assert!(!auth.has_tokens(AccountKind::Broadcaster));
            assert!(auth.secrets.get(AccountKind::Broadcaster).unwrap().is_none());
        }
        let old_revision = auth.begin_attempt(AccountKind::Broadcaster).unwrap();
        connect(&auth, AccountKind::Broadcaster, "2");
        let (pair, v) = candidate("1", AccountKind::Broadcaster);
        assert!(!auth.accept_candidate(AccountKind::Broadcaster, old_revision, pair, v).unwrap());
        assert_eq!(auth.info(AccountKind::Broadcaster).unwrap().user_id, "2");
    }

    #[test]
    fn candidate_requires_valid_identity_client_and_scopes() {
        let (_dir, auth) = manager();
        auth.select_mode(true);
        connect(&auth, AccountKind::Broadcaster, "1");
        for problem in ["client", "id", "scopes"] {
            let revision = auth.begin_attempt(AccountKind::Broadcaster).unwrap();
            let (pair, mut v) = candidate("2", AccountKind::Broadcaster);
            match problem {
                "client" => v.client_id = "other-app".into(),
                "id" => v.user_id.clear(),
                _ => v.scopes.clear(),
            }
            assert!(auth.accept_candidate(AccountKind::Broadcaster, revision, pair, v).is_err());
            assert_eq!(auth.info(AccountKind::Broadcaster).unwrap().user_id, "1");
            assert!(auth.both_ready());
        }
    }

    #[test]
    fn legacy_duplicate_ids_are_not_ready_and_require_explicit_repair() {
        let (_dir, auth) = manager();
        legacy_slot(&auth, AccountKind::Broadcaster, "1");
        legacy_slot(&auth, AccountKind::Bot, "1");
        assert!(auth.setup_status().duplicate_accounts);
        assert!(!auth.both_ready());
        assert!(!auth.is_shared());
        auth.select_mode(true);
        assert!(auth.is_shared());
        assert!(auth.both_ready());
        assert!(!auth.setup_status().duplicate_accounts);
    }

    #[test]
    fn separate_draft_explains_duplicate_inactive_bot_while_shared_keeps_running() {
        let (_dir, auth) = manager();
        legacy_slot(&auth, AccountKind::Broadcaster, "1");
        legacy_slot(&auth, AccountKind::Bot, "1");
        auth.select_mode(true);
        assert!(auth.is_shared());
        auth.select_mode(false);
        assert!(auth.is_shared());
        assert!(auth.both_ready());
        assert!(auth.setup_status().duplicate_accounts);
        connect(&auth, AccountKind::Bot, "2");
        assert!(!auth.setup_status().duplicate_accounts);
        assert!(!auth.is_shared());
    }

    #[test]
    fn missing_shared_scopes_keep_separate_sender_until_streamer_reauthenticated() {
        let (_dir, auth) = manager();
        legacy_slot(&auth, AccountKind::Broadcaster, "1");
        legacy_slot(&auth, AccountKind::Bot, "2");
        auth.state
            .lock()
            .slots
            .get_mut(&AccountKind::Broadcaster)
            .unwrap()
            .tokens
            .as_mut()
            .unwrap()
            .scopes
            .retain(|s| *s != "user:write:chat");
        assert!(auth.both_ready());
        auth.select_mode(true);
        assert!(!auth.is_shared());
        assert!(auth.both_ready());
        assert_eq!(auth.info(AccountKind::Bot).unwrap().user_id, "2");
        connect(&auth, AccountKind::Broadcaster, "1");
        assert!(auth.is_shared());
        assert_eq!(auth.info(AccountKind::Bot).unwrap().user_id, "1");
    }

    #[test]
    fn storage_failure_keeps_previous_authorization_and_secret_cache() {
        let (dir, auth) = manager();
        auth.select_mode(true);
        connect(&auth, AccountKind::Broadcaster, "1");
        let previous = auth.secrets.get(AccountKind::Broadcaster).unwrap();
        // Надёжный отказ записи, независимо от прав пользователя процесса.
        std::fs::create_dir(AppPaths::new(dir.path()).secrets_fallback_file().with_extension("tmp")).unwrap();
        let revision = auth.begin_attempt(AccountKind::Broadcaster).unwrap();
        let (pair, v) = candidate("2", AccountKind::Broadcaster);
        assert!(auth.accept_candidate(AccountKind::Broadcaster, revision, pair, v).is_err());
        assert_eq!(auth.secrets.get(AccountKind::Broadcaster).unwrap(), previous);
        assert_eq!(auth.info(AccountKind::Broadcaster).unwrap().user_id, "1");
        assert!(auth.both_ready());
    }

    #[tokio::test]
    async fn logout_unused_bot_does_not_revoke_shared_streamer_or_accept_late_candidate() {
        let (_dir, auth) = manager();
        auth.select_mode(true);
        connect(&auth, AccountKind::Broadcaster, "1");
        auth.select_mode(false);
        let revision = auth.begin_attempt(AccountKind::Bot).unwrap();
        // В неактивном слоте ещё нет токена: HTTP-запрос отзыва не требуется.
        auth.logout(AccountKind::Bot).await;
        assert!(auth.is_shared());
        assert!(auth.both_ready());
        assert_eq!(auth.info(AccountKind::Broadcaster).unwrap().user_id, "1");
        let (pair, v) = candidate("2", AccountKind::Bot);
        assert!(!auth.accept_candidate(AccountKind::Bot, revision, pair, v).unwrap());
    }

    #[test]
    fn streamer_logout_clears_pending_bot_before_network_revoke() {
        let (_dir, auth) = manager();
        connect(&auth, AccountKind::Broadcaster, "1");
        let revision = auth.begin_attempt(AccountKind::Bot).unwrap();
        let pair = auth.clear_account(AccountKind::Broadcaster).unwrap();
        assert_eq!(pair.access_token, "access-1");
        assert!(!auth.has_tokens(AccountKind::Broadcaster));
        assert!(auth.secrets.get(AccountKind::Broadcaster).unwrap().is_none());
        assert!(auth.setup_status().bot.attempt.is_none());
        assert!(auth.begin_attempt(AccountKind::Bot).is_err());
        let (pair, v) = candidate("2", AccountKind::Bot);
        assert!(!auth.accept_candidate(AccountKind::Bot, revision, pair, v).unwrap());
        assert!(!auth.both_ready());
    }

    #[tokio::test]
    async fn streamer_logout_cancels_pending_bot_while_waiting_for_refresh() {
        let (_dir, auth) = manager();
        connect(&auth, AccountKind::Broadcaster, "1");
        let revision = auth.begin_attempt(AccountKind::Bot).unwrap();
        let guard = auth.refresh_locks[&AccountKind::Broadcaster].lock().await;
        let manager = Arc::clone(&auth);
        let logout = tokio::spawn(async move { manager.logout(AccountKind::Broadcaster).await });
        tokio::task::yield_now().await;
        assert!(auth.setup_status().bot.attempt.is_none());
        let (pair, v) = candidate("2", AccountKind::Bot);
        assert!(!auth.accept_candidate(AccountKind::Bot, revision, pair, v).unwrap());
        // Прерываем до HTTP: тест проверяет ожидание refresh без живого Twitch.
        logout.abort();
        assert!(logout.await.unwrap_err().is_cancelled());
        drop(guard);
    }

    #[test]
    fn seeded_account_info_is_not_proof_of_valid_authorization() {
        let (_dir, auth) = manager();
        let (pair, _) = candidate("1", AccountKind::Broadcaster);
        auth.store(AccountKind::Broadcaster, pair);
        auth.seed_info(
            AccountKind::Broadcaster,
            Some(AccountInfo {
                login: "old".into(),
                user_id: "1".into(),
                display_name: "Old".into(),
            }),
        );
        auth.select_mode(true);
        assert!(!auth.both_ready());
        assert_ne!(auth.status(AccountKind::Broadcaster).state, "authorized");
        assert!(auth.begin_attempt(AccountKind::Bot).is_err());
    }

    #[test]
    fn shared_mode_routes_bot_to_broadcaster_slot() {
        let dir = tempfile::tempdir().unwrap();
        let secrets = Secrets::file_only(&AppPaths::new(dir.path()));
        let auth = AuthManager::new("cid".into(), secrets);
        let pair = TokenPair {
            access_token: "a".into(),
            refresh_token: "r".into(),
            expires_at: 10,
            scopes: auth::BROADCASTER_SCOPES.iter().map(|s| s.to_string()).collect(),
        };
        auth.store(AccountKind::Broadcaster, pair);
        auth.state.lock().slots.get_mut(&AccountKind::Broadcaster).unwrap().info = Some(AccountInfo {
            login: "cozy".into(),
            user_id: "1".into(),
            display_name: "cozy".into(),
        });
        // обычный режим: бот пуст
        assert!(!auth.has_tokens(AccountKind::Bot));
        assert!(!auth.both_ready());
        // один аккаунт: бот читает слот стримера, но стримеру не хватает прав бота
        auth.set_shared(true);
        assert!(auth.has_tokens(AccountKind::Bot));
        assert_eq!(auth.info(AccountKind::Bot).unwrap().user_id, "1");
        assert!(!auth.both_ready());
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
        let old = TokenPair {
            access_token: "a1".into(),
            refresh_token: "r1".into(),
            expires_at: 10,
            scopes: vec![],
        };
        auth.store(AccountKind::Bot, old.clone());
        // хранилище совпадает с памятью — ничего не меняем
        assert!(auth.adopt_stored_if_rotated(AccountKind::Bot, &old).is_none());
        // другой процесс обновил токен
        let fresh = TokenPair {
            access_token: "a2".into(),
            refresh_token: "r2".into(),
            expires_at: 20,
            scopes: vec![],
        };
        secrets.set(AccountKind::Bot, &fresh).unwrap();
        let adopted = auth.adopt_stored_if_rotated(AccountKind::Bot, &old).unwrap();
        assert_eq!(adopted, fresh);
        assert_eq!(auth.state.lock().slots[&AccountKind::Bot].tokens, Some(fresh));
    }
}
