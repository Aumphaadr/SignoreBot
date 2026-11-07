//! Official Twitch popout in a child webview. No IPC grants or OAuth token injection.
use parking_lot::Mutex;
use serde::Deserialize;
use std::path::Path;
use tauri::{Manager, PhysicalPosition, PhysicalSize, WebviewBuilder, WebviewUrl};

#[derive(Default)]
pub struct NativeChat {
    channel: Mutex<String>,
    incognito: bool,
}
impl NativeChat {
    /// Smoke checks must never reuse the user's WKWebView store on macOS.
    pub fn ephemeral() -> Self {
        Self {
            channel: Mutex::new(String::new()),
            incognito: true,
        }
    }
}
#[derive(Deserialize)]
pub struct ViewBounds {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}
fn twitch_url(url: &url::Url) -> bool {
    url.scheme() == "https"
        && url
            .host_str()
            .is_some_and(|h| h == "twitch.tv" || h.ends_with(".twitch.tv"))
}
pub fn update(
    app: &tauri::AppHandle,
    state: &NativeChat,
    root: &Path,
    login: Option<String>,
    bounds: Option<ViewBounds>,
    reload: bool,
) -> Result<(), String> {
    let mut channel = state.channel.lock();
    let existing = app.get_webview("twitch-chat");
    let Some(bounds) = bounds else {
        if let Some(w) = existing {
            w.hide().map_err(|e| e.to_string())?;
        }
        return Ok(());
    };
    let login = login
        .filter(|s| !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'))
        .ok_or("Сначала авторизуйте стримера")?;
    if ![bounds.x, bounds.y, bounds.width, bounds.height]
        .iter()
        .all(|n| n.is_finite() && *n >= 0.0 && *n <= 100_000.0)
        || bounds.width < 100.0
        || bounds.height < 100.0
    {
        return Err("Область чата слишком мала".into());
    }
    let position = PhysicalPosition::new(bounds.x.round() as i32, bounds.y.round() as i32);
    let size = PhysicalSize::new(bounds.width.round() as u32, bounds.height.round() as u32);
    if let Some(w) = existing {
        if *channel == login && !reload {
            #[cfg(target_os = "linux")]
            place_linux(&w, &bounds)?;
            #[cfg(not(target_os = "linux"))]
            {
                w.set_position(position).map_err(|e| e.to_string())?;
                w.set_size(size).map_err(|e| e.to_string())?;
            }
            return w.show().map_err(|e| e.to_string());
        }
        w.close().map_err(|e| e.to_string())?;
    }
    let profile = root.join("twitch-webview");
    std::fs::create_dir_all(&profile).map_err(|e| e.to_string())?;
    let url = format!("https://www.twitch.tv/popout/{login}/chat?popout=")
        .parse()
        .map_err(|e: url::ParseError| e.to_string())?;
    let handle = app.clone();
    let popup_profile = profile.clone();
    let incognito = state.incognito;
    let builder = WebviewBuilder::new("twitch-chat", WebviewUrl::External(url))
        .data_directory(profile)
        .incognito(incognito)
        .on_page_load(|_, payload| tracing::debug!(target: "signorebot::reader", host = payload.url().host_str().unwrap_or(""), event = ?payload.event(), "Twitch webview page load"))
        .additional_browser_args("--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection --proxy-server=direct://")
        .on_navigation(|url| twitch_url(url) || url.as_str() == "about:blank")
        .on_new_window(move |url, features| {
            if !twitch_url(&url) {
                if ["http", "https"].contains(&url.scheme()) { let _ = open::that_detached(url.as_str()); }
                return tauri::webview::NewWindowResponse::Deny;
            }
            let result = tauri::WebviewWindowBuilder::new(&handle, format!("twitch-login-{}", uuid::Uuid::new_v4()), WebviewUrl::External("about:blank".parse().unwrap()))
                .title("Twitch").inner_size(520.0, 740.0).data_directory(popup_profile.clone()).incognito(incognito)
                .window_features(features).on_navigation(|url| twitch_url(url) || url.as_str() == "about:blank").build();
            match result { Ok(window) => tauri::webview::NewWindowResponse::Create { window }, Err(e) => { tracing::warn!("Twitch login window: {e}"); tauri::webview::NewWindowResponse::Deny } }
        });
    let view = app
        .get_window("main")
        .ok_or("Основное окно недоступно")?
        .add_child(builder, position, size)
        .map_err(|e| e.to_string())?;
    #[cfg(target_os = "linux")]
    if let Err(error) = place_linux(&view, &bounds) {
        let _ = view.close();
        return Err(error);
    }
    #[cfg(not(target_os = "linux"))]
    let _ = view;
    *channel = login;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_official_https_navigation_is_internal() {
        for s in [
            "https://www.twitch.tv/login",
            "https://id.twitch.tv/oauth2/authorize",
        ] {
            assert!(twitch_url(&s.parse().unwrap()));
        }
        for s in [
            "http://twitch.tv/",
            "https://twitch.tv.attacker.test/",
            "https://nottwitch.tv/",
            "file:///etc/passwd",
            "tauri://localhost/",
        ] {
            assert!(!twitch_url(&s.parse().unwrap()));
        }
    }
}

/// Tauri 2.11 puts Linux child views into GtkBox, where Wry ignores set_bounds.
/// Wrap the original panel in GtkOverlay and allocate just the native chat rectangle.
#[cfg(target_os = "linux")]
fn place_linux(view: &tauri::Webview, bounds: &ViewBounds) -> Result<(), String> {
    use gtk::prelude::*;
    let rect = (bounds.x, bounds.y, bounds.width, bounds.height);
    let (tx, rx) = std::sync::mpsc::sync_channel(1);
    view.with_webview(move |platform| {
        let result = (|| -> Result<(), String> {
            let inner = platform.inner();
            let parent = inner.parent().ok_or("Twitch view has no GTK parent")?;
            let overlay = if let Ok(overlay) = parent.clone().downcast::<gtk::Overlay>() {
                overlay
            } else {
                let container = parent
                    .downcast::<gtk::Box>()
                    .map_err(|_| "Unexpected GTK container")?;
                let existing = container
                    .children()
                    .into_iter()
                    .filter_map(|w| w.downcast::<gtk::Overlay>().ok())
                    .find(|w| w.widget_name() == "signorebot-twitch-container");
                let overlay = if let Some(overlay) = existing {
                    overlay
                } else {
                    let panel = container
                        .children()
                        .into_iter()
                        .find(|w| {
                            w.type_().name() == "WebKitWebView"
                                && w != inner.upcast_ref::<gtk::Widget>()
                        })
                        .ok_or("Main panel GTK widget not found")?;
                    let overlay = gtk::Overlay::new();
                    overlay.set_widget_name("signorebot-twitch-container");
                    overlay.set_hexpand(true);
                    overlay.set_vexpand(true);
                    container.remove(&panel);
                    overlay.add(&panel);
                    container.pack_start(&overlay, true, true, 0);
                    overlay
                };
                container.remove(&inner);
                overlay.add_overlay(&inner);
                overlay.show_all();
                overlay
            };
            let scale = inner.scale_factor() as f64;
            inner.set_halign(gtk::Align::Start);
            inner.set_valign(gtk::Align::Start);
            inner.set_margin_start((rect.0 / scale).round() as i32);
            inner.set_margin_top((rect.1 / scale).round() as i32);
            inner.set_size_request(
                (rect.2 / scale).round() as i32,
                (rect.3 / scale).round() as i32,
            );
            overlay.set_overlay_pass_through(&inner, false);
            Ok(())
        })();
        let _ = tx.send(result);
    })
    .map_err(|e| e.to_string())?;
    rx.recv_timeout(std::time::Duration::from_secs(5))
        .map_err(|e| e.to_string())?
}
