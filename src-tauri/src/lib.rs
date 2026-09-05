pub mod commands;
pub mod config;
pub mod core;
pub mod engine;
pub mod logging;
pub mod media;
pub mod overlay;
pub mod paths;
pub mod secrets;
pub mod twitch;
pub mod updates;

use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{Manager, WindowEvent};
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::Arc;

/// Панель сообщила, что загрузилась (`panel_ready`). Если через несколько
/// секунд после старта сигнала нет — окно показывает ошибку WebView вместо
/// интерфейса (обычно «Connection refused» на белом фоне), и надо объяснить.
pub struct PanelReady(pub Arc<AtomicBool>, pub Arc<AtomicU8>);

const PANEL_LOAD_TIMEOUT_SECS: u64 = 8;

fn panel_watchdog(app: tauri::AppHandle, ready: Arc<AtomicBool>, attempts: Arc<AtomicU8>) {
    use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(PANEL_LOAD_TIMEOUT_SECS)).await;
        if ready.load(Ordering::SeqCst) {
            return;
        }
        let Some(win) = app.get_webview_window("main") else { return };
        if !win.is_visible().unwrap_or(true) {
            return; // окно спрятано в трей — панели и не должно быть
        }
        let url = win.url().map(|u| u.to_string()).unwrap_or_else(|_| "?".into());
        let attempt = attempts.fetch_add(1, Ordering::SeqCst) + 1;
        tracing::warn!(target: "signorebot::core", "Панель не загрузилась за {PANEL_LOAD_TIMEOUT_SECS} с (попытка {attempt}, адрес окна: {url})");
        let logs = app.try_state::<commands::CoreState>().map(|c| c.0.paths.logs_dir().display().to_string()).unwrap_or_default();
        let why = if cfg!(debug_assertions) {
            "Это отладочная сборка: панель берётся с dev-сервера Vite на порту 1420, а он не запущен. Запустите `npm run dev` или используйте собранное приложение.".to_string()
        } else {
            format!(
                "Бот при этом работает: чат, награды и оверлеи живут в фоне, страдает только окно.\n\n\
                 Обычные причины:\n\
                 • VPN или прокси, который перехватывает и локальные адреса (у панели адрес {url}). Выключите его или добавьте исключение для localhost.\n\
                 • Антивирус или брандмауэр блокирует встроенный браузер (WebView2 на Windows).\n\
                 • Повреждён WebView2 Runtime — переустановите его с сайта Microsoft.\n\n\
                 Логи: {logs}"
            )
        };
        let text = if attempt == 1 {
            format!("Окно панели не загрузилось: встроенный браузер показал ошибку вместо интерфейса.\n\n{why}")
        } else {
            format!("Панель всё ещё не загружается.\n\n{why}\n\nМожно продолжить: бот работает из трея, окно попробуйте открыть позже через меню трея.")
        };
        let ready2 = Arc::clone(&ready);
        let attempts2 = Arc::clone(&attempts);
        let app2 = app.clone();
        app.dialog()
            .message(text)
            .title("SignoreBot: панель не загрузилась")
            .kind(MessageDialogKind::Warning)
            .buttons(MessageDialogButtons::OkCancelCustom("Перезагрузить панель".into(), "Продолжить".into()))
            .show(move |reload| {
                if !reload {
                    return;
                }
                if let Some(w) = app2.get_webview_window("main") {
                    tracing::info!(target: "signorebot::core", "Перезагрузка панели по запросу пользователя");
                    let _ = w.reload();
                }
                if attempts2.load(Ordering::SeqCst) < 3 {
                    panel_watchdog(app2.clone(), ready2, attempts2);
                }
            });
    });
}

fn show_main(app: &tauri::AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

/// Иконка в трее: «Показать» / «Выход». Закрытие окна прячет его в трей,
/// бот продолжает работать.
fn setup_tray(app: &tauri::App) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "Показать SignoreBot", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Выход (остановить бота)", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &quit])?;
    let mut builder = TrayIconBuilder::with_id("main").menu(&menu).tooltip("SignoreBot").show_menu_on_left_click(false);
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder
        .on_menu_event(|app, ev| match ev.id.as_ref() {
            "show" => show_main(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, ev| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = ev {
                show_main(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

struct TrayReady(bool);

fn resolve_paths(app: &tauri::AppHandle) -> paths::AppPaths {
    if let Some(p) = paths::AppPaths::from_env() {
        return p;
    }
    let dir = app.path().app_data_dir().unwrap_or_else(|_| std::env::current_dir().unwrap_or_default().join("signorebot-data"));
    paths::AppPaths::from_default(dir)
}

/// AppImage: `AppRun` из AppImageKit выставляет
/// `GST_PLUGIN_SYSTEM_PATH_1_0=$APPDIR/usr/lib/gstreamer-1.0:` (и то же для
/// `GST_PLUGIN_SYSTEM_PATH`), затирая прежнее значение. Каталога в AppDir
/// нет, а при заданной переменной GStreamer системные плагины не сканирует —
/// WebKit пишет «element appsink not found», `<video>` в панели не играет,
/// а WebKitWebProcess падает. Оставляем в переменных только существующие
/// каталоги; если таких нет — снимаем переменную, и GStreamer идёт по своим
/// умолчаниям. Работает до создания webview: дочерние процессы наследуют env.
#[cfg(target_os = "linux")]
fn fix_appimage_gstreamer_env() {
    if std::env::var_os("APPDIR").is_none() {
        return;
    }
    for key in ["GST_PLUGIN_SYSTEM_PATH_1_0", "GST_PLUGIN_SYSTEM_PATH"] {
        let Ok(value) = std::env::var(key) else { continue };
        let keep: Vec<&str> = value.split(':').filter(|p| !p.is_empty() && std::path::Path::new(p).is_dir()).collect();
        if keep.is_empty() {
            std::env::remove_var(key);
        } else if keep.len() != value.split(':').count() {
            std::env::set_var(key, keep.join(":"));
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    #[cfg(target_os = "linux")]
    fix_appimage_gstreamer_env();
    tauri::Builder::default()
        // Вторая копия приложения (например, новая сборка при живой старой в
        // трее) не запускается, а показывает окно первой: две копии по очереди
        // обновляли бы одноразовые refresh-токены Twitch и «выкидывали» друг друга.
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            tracing::warn!(target: "signorebot::core", "Попытка запустить вторую копию SignoreBot — показываю уже работающее окно");
            show_main(app);
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            let handle = app.handle().clone();
            let paths = resolve_paths(&handle);
            paths.ensure_dirs()?;
            logging::init(&paths);
            logging::prune_old_logs(&paths, 30);
            tracing::info!(target: "signorebot::core", "SignoreBot {} · данные: {}", env!("CARGO_PKG_VERSION"), paths.root.display());
            let core = core::Core::start(handle, paths)?;
            app.manage(commands::CoreState(core));
            let ready = Arc::new(AtomicBool::new(false));
            let attempts = Arc::new(AtomicU8::new(0));
            app.manage(PanelReady(Arc::clone(&ready), Arc::clone(&attempts)));
            panel_watchdog(app.handle().clone(), ready, attempts);
            match setup_tray(app) {
                Ok(()) => app.manage(TrayReady(true)),
                Err(e) => {
                    tracing::warn!(target: "signorebot::core", "Иконка в трее недоступна ({e}); закрытие окна завершит бота");
                    app.manage(TrayReady(false))
                }
            };
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                let tray_ok = window.app_handle().try_state::<TrayReady>().map(|t| t.0).unwrap_or(false);
                let to_tray = window.app_handle().try_state::<commands::CoreState>().map(|c| c.0.config.read().app.close_to_tray).unwrap_or(true);
                if tray_ok && to_tray && window.label() == "main" {
                    let _ = window.hide();
                    api.prevent_close();
                    tracing::info!(target: "signorebot::core", "Окно свёрнуто в трей, бот продолжает работать");
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::panel_ready,
            commands::status_get,
            commands::migration_dismiss,
            commands::log_history,
            commands::log_export,
            commands::config_get,
            commands::config_set_section,
            commands::config_export,
            commands::config_export_write,
            commands::config_import_file,
            commands::auth_start,
            commands::auth_cancel,
            commands::auth_logout,
            commands::auth_refresh,
            commands::auth_set_same_account,
            commands::media_list,
            commands::media_import,
            commands::media_delete,
            commands::media_delete_unused,
            commands::media_probe,
            commands::media_url,
            commands::event_test,
            commands::periodic_trigger,
            commands::periodic_status,
            commands::shoutout_status,
            commands::shoutout_trigger,
            commands::shoutout_remove,
            commands::shoutout_reset,
            commands::rewards_channel,
            commands::reward_create_managed_copy,
            commands::reward_finish_managed_copy,
            commands::reward_create_twitch,
            commands::reward_update_twitch,
            commands::reward_delete_twitch,
            commands::redemptions_list,
            commands::redemption_dismiss,
            commands::redemption_refund,
            commands::rewards_queue_url,
            commands::chat_send,
            commands::viewers_get,
            commands::overlay_clear,
            commands::response_test,
            commands::obs_test,
            commands::obs_refresh,
            commands::obs_set_url,
            commands::obs_match_sources,
            commands::overlay_key_regenerate,
            commands::app_open_data_dir,
            commands::updates_check,
            commands::data_dir_info,
            commands::data_dir_set,
            commands::app_restart,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            if let tauri::RunEvent::Exit = event {
                if let Some(state) = app.try_state::<commands::CoreState>() {
                    let core = std::sync::Arc::clone(&state.0);
                    tauri::async_runtime::block_on(core.shutdown());
                }
            }
        });
}
