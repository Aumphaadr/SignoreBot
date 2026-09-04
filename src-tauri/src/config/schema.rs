//! Схема конфига SignoreBot (версия 2).
//!
//! Единственный источник правды о структуре настроек: из этих типов
//! генерируются TypeScript-типы для фронтенда (`cargo test` → `ts-rs`).
//! Все имена полей в JSON — camelCase, как в старом `config.json`, чтобы
//! миграция была почти тождественной.
//!
//! Секретов (токенов) здесь нет — они живут в системном хранилище (`secrets`).

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use ts_rs::TS;

pub const CONFIG_VERSION: u32 = 2;

/// Client ID Twitch-приложения по умолчанию (публичный идентификатор, не секрет).
pub const DEFAULT_TWITCH_CLIENT_ID: &str = "555mp53tuhbk8w2m9ptwyb47t8crbq";
/// Client ID старой версии (Confidential-приложение) — заменяется на новый при загрузке.
pub const LEGACY_TWITCH_CLIENT_ID: &str = "h2f3713n62j16edr5mdqlxcu2juk3o";

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[serde(rename_all = "camelCase", default)]
#[ts(export, export_to = "config.ts")]
pub struct Config {
    pub version: u32,
    pub twitch: TwitchSettings,
    pub network: NetworkSettings,
    pub accounts: Accounts,
    pub overlay_settings: OverlaySettings,
    pub commands: Vec<Command>,
    pub rewards: Vec<Reward>,
    pub events: BTreeMap<String, EventReaction>,
    pub periodic_events: Vec<PeriodicEvent>,
    pub shoutout: ShoutoutSettings,
    pub banwords: BanwordSettings,
    pub overlays: Vec<Overlay>,
    pub obs: ObsSettings,
    pub notes: Vec<Note>,
    pub updates: UpdateSettings,
    pub app: AppSettings,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            version: CONFIG_VERSION,
            twitch: TwitchSettings::default(),
            network: NetworkSettings::default(),
            accounts: Accounts::default(),
            overlay_settings: OverlaySettings::default(),
            commands: vec![],
            rewards: vec![],
            events: BTreeMap::new(),
            periodic_events: vec![],
            shoutout: ShoutoutSettings::default(),
            banwords: BanwordSettings::default(),
            overlays: vec![],
            obs: ObsSettings::default(),
            notes: vec![],
            updates: UpdateSettings::default(),
            app: AppSettings::default(),
        }
    }
}

// ---------------------------------------------------------------------------
// Общие настройки
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[serde(rename_all = "camelCase", default)]
#[ts(export, export_to = "config.ts")]
pub struct TwitchSettings {
    /// Client ID публичного Twitch-приложения (Device Code Grant Flow).
    pub client_id: String,
}

impl Default for TwitchSettings {
    fn default() -> Self {
        Self { client_id: DEFAULT_TWITCH_CLIENT_ID.to_string() }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[serde(rename_all = "camelCase", default)]
#[ts(export, export_to = "config.ts")]
pub struct NetworkSettings {
    /// Порт HTTP-сервера оверлеев (страницы, медиа, WebSocket на том же порту).
    pub http_port: u16,
    /// Слушать все интерфейсы (доступ из LAN для OBS на другой машине).
    pub allow_lan: bool,
    /// Ключ доступа к оверлеям: `/overlay/<path>?key=…`. Генерируется при первом запуске.
    pub overlay_key: String,
    /// Показывать/копировать URL оверлеев с `127.0.0.1` (OBS на этом же
    /// компьютере): так работает Service Worker и порядок запуска OBS/бота не важен.
    pub prefer_localhost_urls: bool,
}

impl Default for NetworkSettings {
    fn default() -> Self {
        Self { http_port: 3001, allow_lan: true, overlay_key: String::new(), prefer_localhost_urls: true }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[serde(rename_all = "camelCase", default)]
#[ts(export, export_to = "config.ts")]
pub struct UpdateSettings {
    /// Репозиторий GitHub с релизами (для форков — свой).
    pub repo_url: String,
    pub check_on_start: bool,
}

impl Default for UpdateSettings {
    fn default() -> Self {
        Self { repo_url: "https://github.com/Aumphaadr/SignoreBot".into(), check_on_start: true }
    }
}

/// Поведение самого приложения (окно, трей).
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[serde(rename_all = "camelCase", default)]
#[ts(export, export_to = "config.ts")]
pub struct AppSettings {
    /// Закрытие окна прячет его в трей (бот работает дальше); иначе — выход.
    pub close_to_tray: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self { close_to_tray: true }
    }
}

/// Несекретная информация об авторизованных аккаунтах.
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Default)]
#[serde(rename_all = "camelCase", default)]
#[ts(export, export_to = "config.ts")]
pub struct Accounts {
    pub broadcaster: Option<AccountInfo>,
    pub bot: Option<AccountInfo>,
    /// Бот — тот же аккаунт, что и стример: один токен с объединёнными правами.
    pub same_account: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Default)]
#[serde(rename_all = "camelCase", default)]
#[ts(export, export_to = "config.ts")]
pub struct AccountInfo {
    pub login: String,
    pub user_id: String,
    pub display_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[serde(rename_all = "camelCase", default)]
#[ts(export, export_to = "config.ts")]
pub struct OverlaySettings {
    /// Пауза между элементами очереди на оверлее, мс.
    pub pause_between_ms: u32,
    /// Длительность показа картинки по умолчанию, с.
    pub image_duration_sec: f32,
    /// Антиспам: тот же файл от того же пользователя в этом окне (мс) отбрасывается.
    /// 0 — выключено.
    pub antispam_window_ms: u32,
}

impl Default for OverlaySettings {
    fn default() -> Self {
        Self { pause_between_ms: 3000, image_duration_sec: 10.0, antispam_window_ms: 1500 }
    }
}

// ---------------------------------------------------------------------------
// Реакция (чат + медиа)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Default)]
#[serde(rename_all = "camelCase", default)]
#[ts(export, export_to = "config.ts")]
pub struct Response {
    pub chat: ChatResponse,
    pub media: MediaResponse,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Default)]
#[serde(rename_all = "camelCase", default)]
#[ts(export, export_to = "config.ts")]
pub struct ChatResponse {
    pub enabled: bool,
    pub components: Vec<Component>,
}

/// Элемент чат-сообщения. Сообщение = конкатенация без разделителей.
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[serde(tag = "type", rename_all = "camelCase")]
#[ts(export, export_to = "config.ts")]
pub enum Component {
    /// Текст; поддерживает подстановки `{user}`, `{target}`, `{message}` и переменные события.
    Static {
        #[serde(default)]
        value: String,
    },
    /// `@автор`
    Author,
    /// `@цель` (первый аргумент команды) либо случайный зритель.
    Target,
    /// `@случайный зритель`
    RandomViewer,
    /// Случайное целое в [min, max].
    Random {
        #[serde(default)]
#[ts(type = "number")]
        min: i64,
        #[serde(default = "default_random_max")]
#[ts(type = "number")]
        max: i64,
    },
    /// Случайная фраза из набора.
    Phrase {
        #[serde(default)]
        phrases: Vec<String>,
    },
    /// Пробел.
    Space,
    /// Значение переменной события по имени.
    Variable {
        #[serde(default)]
        name: String,
    },
}

fn default_random_max() -> i64 {
    100
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[serde(rename_all = "camelCase", default)]
#[ts(export, export_to = "config.ts")]
pub struct MediaResponse {
    pub enabled: bool,
    /// Имя файла в каталоге медиа.
    pub file: String,
    /// Вторичный файл: звук к картинке или картинка к звуку.
    pub secondary_file: String,
    /// 0..=100
    pub volume: u8,
    /// id оверлея; `None` — все оверлеи.
    pub overlay: Option<String>,
    pub queue_mode: QueueMode,
    /// CSS `mix-blend-mode` или "none".
    pub chromakey: String,
    /// Длительность показа картинки, с; `None` — из общих настроек.
    pub image_duration_sec: Option<f32>,
    pub animation: MediaAnimation,
    pub text: MediaText,
}

impl Default for MediaResponse {
    fn default() -> Self {
        Self {
            enabled: false,
            file: String::new(),
            secondary_file: String::new(),
            volume: 100,
            overlay: None,
            queue_mode: QueueMode::Queue,
            chromakey: "none".into(),
            image_duration_sec: None,
            animation: MediaAnimation::default(),
            text: MediaText::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "config.ts")]
pub enum QueueMode {
    #[default]
    Queue,
    Immediate,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[serde(rename_all = "camelCase", default)]
#[ts(export, export_to = "config.ts")]
pub struct MediaAnimation {
    /// none | fadeIn | fadeInLeft | fadeInRight | fadeInTop | fadeInBottom | scaleIn
    pub enter: String,
    /// none | fadeOut | fadeOutLeft | fadeOutRight | fadeOutTop | fadeOutBottom | scaleOut
    pub exit: String,
    pub enter_duration: f32,
    pub exit_duration: f32,
}

impl Default for MediaAnimation {
    fn default() -> Self {
        Self { enter: "none".into(), exit: "none".into(), enter_duration: 0.5, exit_duration: 0.5 }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[serde(rename_all = "camelCase", default)]
#[ts(export, export_to = "config.ts")]
pub struct MediaText {
    pub enabled: bool,
    pub content: String,
    /// overlay | above | below | left | right
    pub position: String,
    /// none | bounce | pulse | rubberBand | tada | wave | wiggle | wobble
    pub animation: String,
    pub animation_amplitude: f32,
    pub font: FontSettings,
}

impl Default for MediaText {
    fn default() -> Self {
        Self {
            enabled: false,
            content: String::new(),
            position: "overlay".into(),
            animation: "none".into(),
            animation_amplitude: 1.0,
            font: FontSettings::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Default)]
#[serde(rename_all = "camelCase", default)]
#[ts(export, export_to = "config.ts")]
pub struct FontSettings {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub font_family: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub font_size: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub font_weight: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub font_style: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub color: Option<String>,
}

// ---------------------------------------------------------------------------
// Команды
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[serde(rename_all = "camelCase", default)]
#[ts(export, export_to = "config.ts")]
pub struct Command {
    pub id: String,
    pub enabled: bool,
    /// Имя без `!`, в нижнем регистре.
    pub name: String,
    /// Алиасы без `!`, в нижнем регистре.
    pub aliases: Vec<String>,
    /// `broadcaster` | `moderators` | `vips` | `subscribers` | `everyone` | `user:<login>`.
    /// Пусто — доступно всем.
    pub permissions: Vec<String>,
    /// Кулдаун между срабатываниями, с (0 — нет).
    pub cooldown_sec: u32,
    pub response: Response,
}

impl Default for Command {
    fn default() -> Self {
        Self {
            id: new_id("cmd"),
            enabled: true,
            name: String::new(),
            aliases: vec![],
            permissions: vec![],
            cooldown_sec: 0,
            response: Response::default(),
        }
    }
}

// ---------------------------------------------------------------------------
// Награды за баллы
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[serde(rename_all = "camelCase", default)]
#[ts(export, export_to = "config.ts")]
pub struct Reward {
    pub id: String,
    pub enabled: bool,
    /// UUID награды в Twitch.
    pub reward_id: String,
    pub reward_title: String,
    pub response: Response,
    /// Награда создана через бота (Twitch разрешает менять её и возвращать
    /// баллы только приложению-создателю).
    #[serde(default)]
    pub managed: bool,
    /// Если медиа не доставлено (оверлей выключен) — вернуть баллы зрителю.
    /// Работает только для `managed`; тогда бот сам закрывает и удачные
    /// погашения (FULFILLED), чтобы они не копились в очереди запросов.
    #[serde(default)]
    pub refund_if_unavailable: bool,
    /// Id награды, с которой сделана управляемая копия (для подсказок).
    #[serde(default)]
    pub original_reward_id: Option<String>,
}

impl Default for Reward {
    fn default() -> Self {
        Self {
            id: new_id("reward"),
            enabled: true,
            reward_id: String::new(),
            reward_title: String::new(),
            response: Response::default(),
            managed: false,
            refund_if_unavailable: false,
            original_reward_id: None,
        }
    }
}

// ---------------------------------------------------------------------------
// События Twitch
// ---------------------------------------------------------------------------

/// Известные типы событий (ключи `Config::events`).
pub const EVENT_TYPES: &[&str] =
    &["follow", "subscribe", "resubscribe", "giftSub", "bits", "raid", "watchStreak"];

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Default)]
#[serde(rename_all = "camelCase", default)]
#[ts(export, export_to = "config.ts")]
pub struct EventReaction {
    pub enabled: bool,
    /// Для `subscribe`: не реагировать на подарочные подписки (о них сообщает `giftSub`).
    pub skip_gifted: bool,
    pub response: Response,
}

// ---------------------------------------------------------------------------
// Периодические события
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[serde(rename_all = "camelCase", default)]
#[ts(export, export_to = "config.ts")]
pub struct PeriodicEvent {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    /// ≥ 10
    pub interval_sec: u32,
    /// < interval
    pub offset_sec: u32,
    /// Цвет на таймлайне (#hex) или пусто.
    pub color: String,
    /// Сработать сразу при запуске бота (иначе первое срабатывание — через offset,
    /// но не раньше чем через интервал с момента запуска при offset=0).
    pub fire_on_start: bool,
    pub response: Response,
}

impl Default for PeriodicEvent {
    fn default() -> Self {
        Self {
            id: new_id("periodic"),
            name: String::new(),
            enabled: true,
            interval_sec: 300,
            offset_sec: 0,
            color: String::new(),
            fire_on_start: false,
            response: Response::default(),
        }
    }
}

// ---------------------------------------------------------------------------
// Shoutout
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[serde(rename_all = "camelCase", default)]
#[ts(export, export_to = "config.ts")]
pub struct ShoutoutSettings {
    /// Логины (нижний регистр) для авто-shoutout по первому сообщению за сессию.
    pub auto_list: Vec<String>,
    pub raid_mode: RaidShoutoutMode,
    /// Пауза между shoutout, с (Twitch: минимум 2 минуты).
    pub cooldown_sec: u32,
}

impl Default for ShoutoutSettings {
    fn default() -> Self {
        Self { auto_list: vec![], raid_mode: RaidShoutoutMode::None, cooldown_sec: 125 }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "config.ts")]
pub enum RaidShoutoutMode {
    #[default]
    None,
    Listed,
    Unlisted,
    All,
}

// ---------------------------------------------------------------------------
// Банворды
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Default)]
#[serde(rename_all = "camelCase", default)]
#[ts(export, export_to = "config.ts")]
pub struct BanwordSettings {
    pub words: Vec<BanWord>,
    /// Не проверять сообщения модераторов и стримера (их всё равно нельзя удалить).
    pub skip_privileged: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[serde(rename_all = "camelCase", default)]
#[ts(export, export_to = "config.ts")]
pub struct BanWord {
    pub word: String,
    pub kind: BanWordKind,
    /// Сгенерированные варианты написания (для показа в UI; матчинг пересчитывает сам).
    pub aliases: Vec<String>,
}

impl Default for BanWord {
    fn default() -> Self {
        Self { word: String::new(), kind: BanWordKind::Hard, aliases: vec![] }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "config.ts")]
pub enum BanWordKind {
    /// Подстрока где угодно.
    #[default]
    Hard,
    /// Только как отдельное слово.
    Soft,
}

// ---------------------------------------------------------------------------
// Оверлеи и OBS
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[serde(rename_all = "camelCase", default)]
#[ts(export, export_to = "config.ts")]
pub struct Overlay {
    pub id: String,
    pub name: String,
    /// Сегмент URL: `[a-z0-9-_]+`, уникален.
    pub path: String,
    /// Что сделать, если медиа пришло, а оверлей не подключён: текст в чат
    /// и/или медиа на другой оверлей. Когда задано, медиа для этого оверлея
    /// не ждёт в отложенной очереди — срабатывает эта реакция.
    #[serde(default)]
    pub fallback: Option<Response>,
    /// Резерв включён (состав хранится и в выключенном состоянии).
    #[serde(default)]
    pub fallback_enabled: bool,
}

impl Default for Overlay {
    fn default() -> Self {
        Self { id: new_id("overlay"), name: String::new(), path: String::new(), fallback: None, fallback_enabled: false }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[serde(rename_all = "camelCase", default)]
#[ts(export, export_to = "config.ts")]
pub struct ObsSettings {
    pub enabled: bool,
    pub url: String,
    pub password: String,
    pub auto_refresh: bool,
    pub browser_sources: Vec<ObsBrowserSource>,
}

impl Default for ObsSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            url: "ws://127.0.0.1:4455".into(),
            password: String::new(),
            auto_refresh: true,
            browser_sources: vec![],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Default)]
#[serde(rename_all = "camelCase", default)]
#[ts(export, export_to = "config.ts")]
pub struct ObsBrowserSource {
    pub overlay_path: String,
    pub input_name: String,
}

// ---------------------------------------------------------------------------
// Заметки
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[serde(rename_all = "camelCase", default)]
#[ts(export, export_to = "config.ts")]
pub struct Note {
    pub id: String,
    pub text: String,
    pub status: NoteStatus,
    pub created_at: String,
    pub updated_at: String,
}

impl Default for Note {
    fn default() -> Self {
        let now = chrono::Utc::now().to_rfc3339();
        Self {
            id: new_id("note"),
            text: String::new(),
            status: NoteStatus::Active,
            created_at: now.clone(),
            updated_at: now,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "config.ts")]
pub enum NoteStatus {
    #[default]
    Active,
    Done,
    Cancelled,
}

// ---------------------------------------------------------------------------
// Утилиты
// ---------------------------------------------------------------------------

/// Идентификатор вида `prefix_<base36 время>_<4 случайных>`.
pub fn new_id(prefix: &str) -> String {
    use rand::Rng;
    let ms = chrono::Utc::now().timestamp_millis() as u64;
    let mut rng = rand::thread_rng();
    let rnd: String = (0..4)
        .map(|_| {
            let i = rng.gen_range(0..36u8);
            (if i < 10 { b'0' + i } else { b'a' + i - 10 }) as char
        })
        .collect();
    format!("{prefix}_{}_{rnd}", to_base36(ms))
}

pub fn to_base36(mut n: u64) -> String {
    if n == 0 {
        return "0".into();
    }
    let mut out = Vec::new();
    while n > 0 {
        let d = (n % 36) as u8;
        out.push(if d < 10 { b'0' + d } else { b'a' + d - 10 });
        n /= 36;
    }
    out.reverse();
    String::from_utf8(out).unwrap()
}

/// Случайный ключ оверлеев (24 символа base36).
pub fn random_key() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    (0..24)
        .map(|_| {
            let i = rng.gen_range(0..36u8);
            (if i < 10 { b'0' + i } else { b'a' + i - 10 }) as char
        })
        .collect()
}

impl Config {
    /// Нормализация после загрузки/правки: дефолты, порядок, ограничения.
    pub fn normalize(&mut self) {
        self.version = CONFIG_VERSION;
        if self.twitch.client_id.trim().is_empty() || self.twitch.client_id == LEGACY_TWITCH_CLIENT_ID {
            self.twitch.client_id = DEFAULT_TWITCH_CLIENT_ID.into();
        }
        if self.network.overlay_key.is_empty() {
            self.network.overlay_key = random_key();
        }
        if self.network.http_port == 0 {
            self.network.http_port = 3001;
        }
        // Команды: имена/алиасы в нижнем регистре, без «!», без пустых;
        // имя и алиас не могут повторяться между командами (первая побеждает,
        // повторное имя получает суффикс, повторный алиас отбрасывается).
        let mut taken: std::collections::HashSet<String> = std::collections::HashSet::new();
        for c in &mut self.commands {
            // Пробелы внутри имени делают команду недостижимой («!my cmd» никогда
            // не совпадёт с первым словом сообщения) — склеиваем.
            c.name = c.name.trim().trim_start_matches('!').to_lowercase().split_whitespace().collect::<Vec<_>>().join("");
            if c.name.is_empty() {
                continue;
            }
            if taken.contains(&c.name) {
                let base = c.name.clone();
                let mut n = 2;
                while taken.contains(&format!("{base}-{n}")) {
                    n += 1;
                }
                c.name = format!("{base}-{n}");
            }
            taken.insert(c.name.clone());
            let mut aliases = Vec::new();
            for a in &c.aliases {
                let a = a.trim().trim_start_matches('!').to_lowercase().split_whitespace().collect::<Vec<_>>().join("");
                if a.is_empty() || a == c.name || taken.contains(&a) || aliases.contains(&a) {
                    continue;
                }
                aliases.push(a);
            }
            for a in &aliases {
                taken.insert(a.clone());
            }
            c.aliases = aliases;
            if c.id.is_empty() {
                c.id = new_id("cmd");
            }
            c.response.media.volume = c.response.media.volume.min(100);
        }
        self.commands.retain(|c| !c.name.is_empty());
        // Награды: на одну награду Twitch — одна реакция (первая побеждает).
        let mut seen_rewards: std::collections::HashSet<String> = std::collections::HashSet::new();
        self.rewards.retain(|r| r.reward_id.is_empty() || seen_rewards.insert(r.reward_id.clone()));
        for r in &mut self.rewards {
            if r.id.is_empty() {
                r.id = new_id("reward");
            }
            r.response.media.volume = r.response.media.volume.min(100);
        }
        for p in &mut self.periodic_events {
            if p.id.is_empty() {
                p.id = new_id("periodic");
            }
            if p.interval_sec < 10 {
                p.interval_sec = 10;
            }
            p.offset_sec %= p.interval_sec;
            p.response.media.volume = p.response.media.volume.min(100);
        }
        for e in self.events.values_mut() {
            e.response.media.volume = e.response.media.volume.min(100);
        }
        self.shoutout.auto_list =
            self.shoutout.auto_list.iter().map(|s| s.trim().trim_start_matches('@').to_lowercase()).filter(|s| !s.is_empty()).collect();
        self.shoutout.auto_list.dedup();
        if self.shoutout.cooldown_sec < 120 {
            self.shoutout.cooldown_sec = 125;
        }
        for w in &mut self.banwords.words {
            w.word = w.word.trim().to_lowercase();
        }
        self.banwords.words.retain(|w| !w.word.is_empty());
        let mut used_paths: std::collections::HashSet<String> = std::collections::HashSet::new();
        for (i, o) in self.overlays.iter_mut().enumerate() {
            if o.id.is_empty() {
                o.id = new_id("overlay");
            }
            let mut p = sanitize_overlay_path(&o.path);
            if p.is_empty() {
                p = sanitize_overlay_path(&o.name);
            }
            if p.is_empty() {
                p = format!("overlay-{}", i + 1);
            }
            if used_paths.contains(&p) {
                let base = p.clone();
                let mut n = 2;
                while used_paths.contains(&format!("{base}-{n}")) {
                    n += 1;
                }
                p = format!("{base}-{n}");
            }
            used_paths.insert(p.clone());
            o.path = p;
        }
        if self.obs.url.trim().is_empty() {
            self.obs.url = "ws://127.0.0.1:4455".into();
        }
        for n in &mut self.notes {
            if n.id.is_empty() {
                n.id = new_id("note");
            }
        }
    }

    pub fn overlay_by_id(&self, id: &str) -> Option<&Overlay> {
        self.overlays.iter().find(|o| o.id == id)
    }
    pub fn overlay_by_path(&self, path: &str) -> Option<&Overlay> {
        self.overlays.iter().find(|o| o.path == path)
    }
}

/// `[a-z0-9-_]`, пробелы → `-`, обрезка дефисов по краям.
pub fn sanitize_overlay_path(input: &str) -> String {
    let lower = input.trim().to_lowercase();
    let mut out = String::new();
    for ch in lower.chars() {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
            out.push(ch);
        } else if ch.is_whitespace() {
            out.push('-');
        }
    }
    out.trim_matches('-').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_roundtrip() {
        let c = Config::default();
        let s = serde_json::to_string_pretty(&c).unwrap();
        let back: Config = serde_json::from_str(&s).unwrap();
        assert_eq!(c, back);
    }

    #[test]
    fn component_tagging() {
        let comps: Vec<Component> = serde_json::from_str(
            r#"[{"type":"static","value":"hi"},{"type":"author"},{"type":"random","min":1,"max":6},{"type":"space"}]"#,
        )
        .unwrap();
        assert_eq!(comps.len(), 4);
        assert!(matches!(comps[2], Component::Random { min: 1, max: 6 }));
        let s = serde_json::to_string(&comps[0]).unwrap();
        assert_eq!(s, r#"{"type":"static","value":"hi"}"#);
    }

    #[test]
    fn normalize_fixes_things() {
        let mut c = Config::default();
        c.commands.push(Command { name: "!Кусь".into(), aliases: vec!["!КУСЬ".into(), "!bite".into()], ..Default::default() });
        c.periodic_events.push(PeriodicEvent { interval_sec: 3, offset_sec: 25, ..Default::default() });
        c.overlays.push(Overlay { path: " My Overlay!".into(), ..Default::default() });
        c.normalize();
        assert_eq!(c.commands[0].name, "кусь");
        assert_eq!(c.commands[0].aliases, vec!["bite"]);
        assert_eq!(c.periodic_events[0].interval_sec, 10);
        assert_eq!(c.periodic_events[0].offset_sec, 5);
        assert_eq!(c.overlays[0].path, "my-overlay");
        assert_eq!(c.network.overlay_key.len(), 24);
    }

    #[test]
    fn base36() {
        assert_eq!(to_base36(0), "0");
        assert_eq!(to_base36(35), "z");
        assert_eq!(to_base36(36), "10");
    }
}
