//! Tauri-команды — тонкий слой над `Core`. Все ошибки — строки для UI.

use crate::config::migrate::MigrationReport;
use crate::config::{store, Config};
use crate::core::{Core, CoreStatus};
use crate::engine::periodic::{Scheduler, TimerStatus};
use crate::engine::shoutout::ShoutoutStatus;
use crate::logging::LogEntry;
use crate::media::{self, MediaFile, ProbeResult};
use crate::overlay::obs::{self, ObsSource};
use crate::secrets::AccountKind;
use crate::twitch::auth::DeviceCode;
use crate::twitch::helix::ChannelReward;
use serde::Serialize;
use std::sync::Arc;
use tauri::State;

type Res<T> = Result<T, String>;

pub struct CoreState(pub Arc<Core>);

fn core(s: &State<'_, CoreState>) -> Arc<Core> {
    Arc::clone(&s.0)
}

// ---------------------------------------------------------------- статус

#[tauri::command]
pub fn status_get(s: State<'_, CoreState>) -> CoreStatus {
    core(&s).status()
}

#[tauri::command]
pub fn migration_dismiss(s: State<'_, CoreState>) {
    core(&s).dismiss_migration();
}

#[tauri::command]
pub fn log_history(s: State<'_, CoreState>) -> Vec<LogEntry> {
    core(&s).logs.history()
}

/// Сохранить кольцевой буфер логов в файл. Возвращает число записей.
#[tauri::command]
pub fn log_export(s: State<'_, CoreState>, path: String) -> Res<usize> {
    let entries = core(&s).logs.history();
    let mut out = String::new();
    for e in &entries {
        let t = chrono::DateTime::from_timestamp_millis(e.ts).map(|d| d.with_timezone(&chrono::Local).format("%Y-%m-%d %H:%M:%S%.3f").to_string()).unwrap_or_default();
        out.push_str(&format!("[{t}] [{}] [{}] {}\n", e.level.to_uppercase(), e.target, e.message));
    }
    std::fs::write(&path, out).map_err(|e| e.to_string())?;
    Ok(entries.len())
}

// ---------------------------------------------------------------- конфиг

#[tauri::command]
pub fn config_get(s: State<'_, CoreState>) -> Config {
    core(&s).config.read().clone()
}

/// Заменить одну верхнеуровневую секцию конфига.
#[tauri::command]
pub fn config_set_section(s: State<'_, CoreState>, section: String, value: serde_json::Value) -> Res<Config> {
    let c = core(&s);
    let mut doc = serde_json::to_value(c.config.read().clone()).map_err(|e| e.to_string())?;
    let obj = doc.as_object_mut().ok_or("конфиг не объект")?;
    if !obj.contains_key(&section) {
        return Err(format!("неизвестная секция «{section}»"));
    }
    obj.insert(section, value);
    let next: Config = serde_json::from_value(doc).map_err(|e| format!("некорректные данные: {e}"))?;
    c.update_config(|cfg| *cfg = next)?;
    let out = c.config.read().clone();
    Ok(out)
}

#[tauri::command]
pub fn config_export(s: State<'_, CoreState>) -> serde_json::Value {
    store::export_document(&core(&s).config.read())
}

/// Записать экспортированный документ в файл, выбранный пользователем.
#[tauri::command]
pub fn config_export_write(path: String, content: String) -> Res<()> {
    std::fs::write(&path, content).map_err(|e| e.to_string())
}

#[derive(Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "api.ts")]
pub struct ImportResult {
    pub report: MigrationReport,
    pub media_imported: usize,
    pub media_errors: Vec<String>,
}

/// Импорт конфига из файла (любой версии). Секреты и аккаунты не трогаются.
#[tauri::command]
pub fn config_import_file(s: State<'_, CoreState>, path: String) -> Res<ImportResult> {
    let c = core(&s);
    let text = std::fs::read_to_string(&path).map_err(|e| format!("не удалось прочитать файл: {e}"))?;
    let (imported, mut report) = store::parse_document(&text).map_err(|e| e.to_string())?;
    // Если оба аккаунта уже авторизованы — совет «авторизуйте заново» неуместен.
    if c.auth.both_ready() {
        report.notes.retain(|n| !n.contains("Токены"));
    }
    store::backup_file(&c.paths, &c.paths.config_file(), "before-import").map_err(|e| e.to_string())?;
    c.update_config(|cfg| {
        let keep_accounts = cfg.accounts.clone();
        let keep_net = cfg.network.clone();
        let keep_obs_pw = cfg.obs.password.clone();
        *cfg = imported;
        cfg.accounts = keep_accounts;
        cfg.network = keep_net;
        if cfg.obs.password.is_empty() {
            cfg.obs.password = keep_obs_pw;
        }
    })?;
    // Если рядом с файлом есть public/media (старая версия) — переносим медиа.
    let mut media_imported = 0;
    let mut media_errors = Vec::new();
    if let Some(dir) = std::path::Path::new(&path).parent() {
        for cand in [dir.join("public").join("media"), dir.join("media")] {
            if cand.is_dir() {
                let (n, errs) = media::import_dir(&c.paths, &cand);
                media_imported += n;
                media_errors.extend(errs);
                break;
            }
        }
    }
    tracing::info!(target: "signorebot::config", "Импортирован конфиг из {path}; медиа перенесено: {media_imported}");
    Ok(ImportResult { report, media_imported, media_errors })
}

// ---------------------------------------------------------------- авторизация

fn kind_of(k: &str) -> Res<AccountKind> {
    match k {
        "broadcaster" => Ok(AccountKind::Broadcaster),
        "bot" => Ok(AccountKind::Bot),
        _ => Err("kind должен быть broadcaster или bot".into()),
    }
}

#[tauri::command]
pub async fn auth_start(s: State<'_, CoreState>, kind: String) -> Res<DeviceCode> {
    let c = core(&s);
    let k = kind_of(&kind)?;
    c.auth.start_device_flow(k).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub fn auth_cancel(s: State<'_, CoreState>, kind: String) -> Res<()> {
    core(&s).auth.cancel_device_flow(kind_of(&kind)?);
    Ok(())
}

#[tauri::command]
pub async fn auth_logout(s: State<'_, CoreState>, kind: String) -> Res<()> {
    let c = core(&s);
    c.auth.logout(kind_of(&kind)?).await;
    Ok(())
}

#[tauri::command]
pub async fn auth_refresh(s: State<'_, CoreState>, kind: String) -> Res<()> {
    let c = core(&s);
    let k = kind_of(&kind)?;
    c.auth.refresh(k).await.map_err(|e| e.to_string())?;
    c.auth.validate_or_refresh(k).await;
    Ok(())
}

// ---------------------------------------------------------------- медиа

#[tauri::command]
pub fn media_list(s: State<'_, CoreState>) -> Res<Vec<MediaFile>> {
    let c = core(&s);
    let cfg = c.config.read().clone();
    media::list(&c.paths, &cfg).map_err(|e| e.to_string())
}

#[derive(Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "api.ts")]
pub struct MediaImportResult {
    pub files: Vec<MediaFile>,
    pub errors: Vec<String>,
}

#[tauri::command]
pub fn media_import(s: State<'_, CoreState>, paths: Vec<String>) -> MediaImportResult {
    let c = core(&s);
    let mut files = Vec::new();
    let mut errors = Vec::new();
    for p in paths {
        match media::import(&c.paths, std::path::Path::new(&p)) {
            Ok(f) => {
                tracing::info!(target: "signorebot::media", "Добавлен файл «{}»", f.name);
                files.push(f);
            }
            Err(e) => errors.push(format!("{p}: {e}")),
        }
    }
    MediaImportResult { files, errors }
}

#[tauri::command]
pub fn media_delete(s: State<'_, CoreState>, name: String) -> Res<()> {
    let c = core(&s);
    media::delete(&c.paths, &name).map_err(|e| e.to_string())?;
    tracing::info!(target: "signorebot::media", "Удалён файл «{name}»");
    Ok(())
}

#[tauri::command]
pub fn media_delete_unused(s: State<'_, CoreState>) -> Res<usize> {
    let c = core(&s);
    let cfg = c.config.read().clone();
    let list = media::list(&c.paths, &cfg).map_err(|e| e.to_string())?;
    let mut n = 0;
    for f in list.into_iter().filter(|f| !f.used) {
        if media::delete(&c.paths, &f.name).is_ok() {
            n += 1;
        }
    }
    tracing::info!(target: "signorebot::media", "Удалено неиспользуемых файлов: {n}");
    Ok(n)
}

#[tauri::command]
pub fn media_probe(s: State<'_, CoreState>, name: String) -> Res<ProbeResult> {
    media::probe(&core(&s).paths, &name).map_err(|e| e.to_string())
}

/// URL медиа для предпросмотра в панели.
#[tauri::command]
pub fn media_url(s: State<'_, CoreState>, name: String) -> Res<String> {
    let c = core(&s);
    let st = c.server_status();
    let port = st.address.as_ref().and_then(|a| a.rsplit(':').next()).and_then(|p| p.parse::<u16>().ok()).unwrap_or(st.port);
    let name = crate::paths::safe_file_name(&name).ok_or("недопустимое имя")?;
    Ok(format!("http://127.0.0.1:{port}/media/{}?key={}", urlencoding(&name), st.overlay_key))
}

fn urlencoding(s: &str) -> String {
    url::form_urlencoded::byte_serialize(s.as_bytes()).collect()
}

// ---------------------------------------------------------------- действия

#[tauri::command]
pub async fn event_test(s: State<'_, CoreState>, event_type: String, extra: Option<serde_json::Value>) -> Res<()> {
    let c = core(&s);
    let mut vars = crate::engine::Engine::test_event_vars(&event_type);
    if let Some(serde_json::Value::Object(m)) = extra {
        for (k, v) in m {
            vars.insert(k, v.as_str().map(String::from).unwrap_or_else(|| v.to_string()));
        }
    }
    tracing::info!(target: "signorebot::events", "Тестовое событие «{event_type}»");
    c.engine.handle_event(&event_type, vars).await;
    Ok(())
}

#[tauri::command]
pub async fn periodic_trigger(s: State<'_, CoreState>, id: String) -> Res<()> {
    let c = core(&s);
    let ev = c.config.read().periodic_events.iter().find(|p| p.id == id).cloned().ok_or("событие не найдено")?;
    tracing::info!(target: "signorebot::periodic", "Таймер «{}» запущен вручную", ev.name);
    Scheduler::trigger(&c.engine, &ev).await;
    Ok(())
}

#[tauri::command]
pub fn periodic_status(s: State<'_, CoreState>) -> Vec<TimerStatus> {
    let c = core(&s);
    c.scheduler.status(&c.engine)
}

#[tauri::command]
pub fn shoutout_status(s: State<'_, CoreState>) -> ShoutoutStatus {
    core(&s).engine.shoutout.status()
}

#[tauri::command]
pub fn shoutout_trigger(s: State<'_, CoreState>, username: String) -> Res<()> {
    let c = core(&s);
    c.engine.shoutout.enqueue_manual(&username)?;
    c.engine.shoutout.notify_handle().notify_one();
    Ok(())
}

#[tauri::command]
pub fn shoutout_remove(s: State<'_, CoreState>, id: u64) -> Res<()> {
    core(&s).engine.shoutout.remove(id).map(|_| ())
}

#[tauri::command]
pub fn shoutout_reset(s: State<'_, CoreState>) {
    core(&s).engine.shoutout.reset_done();
}

#[tauri::command]
pub async fn rewards_channel(s: State<'_, CoreState>) -> Res<Vec<ChannelReward>> {
    let c = core(&s);
    let ids = c.engine.ids().ok_or("Стример не авторизован")?;
    c.helix.custom_rewards(AccountKind::Broadcaster, &ids.broadcaster_id).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn chat_send(s: State<'_, CoreState>, text: String) -> Res<()> {
    let c = core(&s);
    let t = text.trim();
    if t.is_empty() {
        return Err("пустое сообщение".into());
    }
    if c.engine.say(&crate::engine::message::truncate_chars(t, 500)).await {
        Ok(())
    } else {
        Err("не удалось отправить (см. логи)".into())
    }
}

#[derive(Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "api.ts")]
pub struct ViewersInfo {
    pub cached: Vec<String>,
    pub recent: Vec<String>,
}

#[tauri::command]
pub fn viewers_get(s: State<'_, CoreState>) -> ViewersInfo {
    let (cached, recent) = core(&s).engine.viewers_snapshot();
    ViewersInfo { cached, recent }
}

// ---------------------------------------------------------------- оверлеи / OBS

/// Очистить очередь (и immediate) на оверлее; `path` пуст — на всех.
#[tauri::command]
pub fn overlay_clear(s: State<'_, CoreState>, path: Option<String>, all: bool) -> Res<()> {
    let c = core(&s);
    let msg = serde_json::json!({ "command": if all { "clearAll" } else { "clearQueue" } }).to_string();
    match path.filter(|p| !p.is_empty()) {
        Some(p) => {
            c.hub.send_to_path(&p, &msg);
        }
        None => {
            c.hub.broadcast(&msg);
        }
    }
    tracing::info!(target: "signorebot::overlay", "Оверлеи: {}", if all { "остановлено всё" } else { "очередь очищена" });
    Ok(())
}

/// Тестовая отправка медиа на оверлей.
#[tauri::command]
pub async fn media_test(s: State<'_, CoreState>, response: crate::config::Response) -> Res<bool> {
    let c = core(&s);
    let ctx = crate::engine::ActionCtx {
        author: "TestUser".into(),
        target: None,
        vars: Default::default(),
        label: "Тест".into(),
        antispam_user: None,
    };
    let mut r = response;
    r.chat.enabled = false;
    r.media.enabled = true;
    let (_, sent) = c.engine.execute(&r, &ctx).await;
    Ok(sent)
}

#[tauri::command]
pub async fn obs_test(s: State<'_, CoreState>) -> Res<Vec<ObsSource>> {
    let settings = core(&s).config.read().obs.clone();
    obs::test_connection(&settings).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn obs_refresh(s: State<'_, CoreState>) -> Res<Vec<String>> {
    let c = core(&s);
    let settings = c.config.read().obs.clone();
    let names: Vec<String> = settings.browser_sources.iter().map(|b| b.input_name.clone()).collect();
    if names.is_empty() {
        return Err("Нет привязок Browser Source".into());
    }
    obs::refresh_browser_sources(&settings, &names).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn obs_set_url(s: State<'_, CoreState>, input_name: String, overlay_path: String) -> Res<String> {
    let c = core(&s);
    let settings = c.config.read().obs.clone();
    let url = c.overlay_status().into_iter().find(|o| o.path == overlay_path).map(|o| o.url).ok_or("оверлей не найден")?;
    obs::set_browser_source_url(&settings, &input_name, &url).await.map_err(|e| e.to_string())?;
    tracing::info!(target: "signorebot::obs", "Источнику «{input_name}» прописан URL оверлея «{overlay_path}»");
    Ok(url)
}

#[tauri::command]
pub fn overlay_key_regenerate(s: State<'_, CoreState>) -> Res<String> {
    let c = core(&s);
    let key = crate::config::random_key();
    let k2 = key.clone();
    c.update_config(move |cfg| cfg.network.overlay_key = k2)?;
    tracing::warn!(target: "signorebot::server", "Ключ оверлеев перевыпущен — обновите URL в OBS");
    Ok(key)
}

/// Проверить обновления по релизам GitHub (`updates.repoUrl` из конфига).
#[tauri::command]
pub async fn updates_check(s: State<'_, CoreState>) -> Res<crate::updates::UpdateInfo> {
    let repo = core(&s).config.read().updates.repo_url.clone();
    let info = crate::updates::check(&repo).await?;
    if info.is_newer {
        tracing::info!(target: "signorebot::updates", "Доступно обновление {} (текущая {})", info.latest.clone().unwrap_or_default(), info.current);
    } else {
        tracing::info!(target: "signorebot::updates", "Обновлений нет (текущая {}{})", info.current, info.latest.as_ref().map(|l| format!(", последний релиз {l}")).unwrap_or_default());
    }
    Ok(info)
}

#[tauri::command]
pub fn app_open_data_dir(s: State<'_, CoreState>) -> Res<()> {
    let p = core(&s).paths.root.clone();
    open::that(p).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Каталог данных

#[derive(Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "api.ts")]
pub struct DataDirInfo {
    pub current: String,
    pub default: String,
    pub source: crate::paths::PathSource,
}

#[tauri::command]
pub fn data_dir_info(s: State<'_, CoreState>) -> DataDirInfo {
    let p = &core(&s).paths;
    DataDirInfo { current: p.root.to_string_lossy().into_owned(), default: p.default_root.to_string_lossy().into_owned(), source: p.source }
}

/// Переключить каталог данных. `path = None` — вернуться к стандартному.
/// При `copy` содержимое текущего каталога (конфиг, медиа, резервные копии,
/// файл секретов) копируется в новый; старое не удаляется. Применяется после
/// перезапуска приложения. Возвращает число скопированных файлов.
#[tauri::command]
pub fn data_dir_set(s: State<'_, CoreState>, path: Option<String>, copy: bool) -> Res<u64> {
    let c = core(&s);
    // конфиг пишем из памяти — он свежее файла на диске
    let cfg_bytes = serde_json::to_vec_pretty(&*c.config.read()).map_err(|e| e.to_string())?;
    let target = path.as_deref().map(str::trim).filter(|p| !p.is_empty()).map(std::path::PathBuf::from);
    let (dir, copied) = crate::paths::relocate(&c.paths, target, copy, &cfg_bytes)?;
    tracing::info!(target: "signorebot::core", "Каталог данных после перезапуска: {} (скопировано файлов: {copied})", dir.display());
    Ok(copied)
}

/// Перезапустить приложение (после смены каталога данных).
#[tauri::command]
pub fn app_restart(app: tauri::AppHandle) {
    app.restart();
}
