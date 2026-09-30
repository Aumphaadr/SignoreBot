//! Проверка обновлений по-настоящему, через локальный HTTP-сервер: version.json
//! сайта, страж «файлы ещё не выложены», формат GitHub и сайт без version.json.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::get;
use axum::Router;
use signorebot_lib::updates::check;

const NEWER: &str = "1.0.99"; // заведомо новее CARGO_PKG_VERSION

async fn serve() -> (String, Arc<AtomicBool>) {
    let files_ready = Arc::new(AtomicBool::new(true));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}/", listener.local_addr().unwrap());
    let b = base.clone();
    let version_json = move || {
        let b = b.clone();
        async move {
            serde_json::json!({
                "version": NEWER, "date": "2026-10-01", "notes": "- что нового",
                "page": format!("{b}#download"), "release": format!("{b}release"),
                "files": { "setup": format!("{b}files/s.exe"), "portable": format!("{b}files/p.exe"), "deb": format!("{b}files/a.deb"), "appimage": format!("{b}files/a.AppImage") },
                "sizes": { "deb": 7 }
            })
            .to_string()
        }
    };
    let b = base.clone();
    let latest_json = move || {
        let b = b.clone();
        async move {
            serde_json::json!({ "tag_name": format!("v{NEWER}"), "html_url": format!("{b}index.html"), "body": "релиз", "assets": [{ "name": "a.deb", "browser_download_url": format!("{b}files/a.deb"), "size": 7 }] }).to_string()
        }
    };
    async fn file(State(ready): State<Arc<AtomicBool>>) -> StatusCode {
        if ready.load(Ordering::SeqCst) { StatusCode::OK } else { StatusCode::NOT_FOUND }
    }
    let app = Router::new()
        .route("/version.json", get(version_json))
        .route("/latest.json", get(latest_json))
        .route("/files/{name}", get(file).head(file))
        .with_state(files_ready.clone());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (base, files_ready)
}

#[tokio::test]
async fn site_version_json_and_pending_files() {
    let (base, ready) = serve().await;
    // сайт с version.json: версия новее, кнопки ведут на страницу скачивания
    let info = check(&base).await.unwrap();
    assert!(info.is_newer);
    assert_eq!(info.latest.as_deref(), Some(NEWER));
    assert_eq!(info.url.as_deref(), Some(format!("{base}#download").as_str()));
    assert_eq!(info.release_url.as_deref(), Some(format!("{base}release").as_str()));
    assert_eq!(info.published_at.as_deref(), Some("2026-10-01"));
    assert_eq!(info.notes.as_deref(), Some("- что нового"));
    assert_eq!(info.assets.len(), 4);
    assert_eq!(info.assets.iter().find(|a| a.name == "a.deb").unwrap().size, 7);
    assert!(info.pending.is_none());
    // адрес с хвостом «#download» и без косой черты — тот же сайт
    assert!(check(&format!("{}#download", base.trim_end_matches('/'))).await.unwrap().is_newer);

    // файлы ещё не выложены: молчим, но говорим, что версия объявлена
    ready.store(false, Ordering::SeqCst);
    let info = check(&base).await.unwrap();
    assert!(!info.is_newer);
    assert_eq!(info.latest, None);
    assert_eq!(info.pending.as_deref(), Some(NEWER));
    assert!(info.notes.is_none());
    ready.store(true, Ordering::SeqCst);
}

#[tokio::test]
async fn github_format_json_and_missing_version_json() {
    let (base, _ready) = serve().await;
    // прямая ссылка на JSON в формате GitHub (форк со своим сервером)
    let info = check(&format!("{base}latest.json")).await.unwrap();
    assert!(info.is_newer);
    assert_eq!(info.latest.as_deref(), Some(format!("v{NEWER}").as_str()));
    assert_eq!(info.url.as_deref(), Some(format!("{base}index.html").as_str()));
    assert_eq!(info.assets.len(), 1);
    // сайт, на котором нет version.json
    let err = check(&format!("{base}nowhere/")).await.unwrap_err();
    assert!(err.contains("нет version.json"), "{err}");
    // не адрес
    assert!(check("abc").await.unwrap_err().contains("Некорректный адрес"));
}
