//! Интеграционный тест сервера оверлеев: ключ, страницы, медиа, WS.

use futures_util::StreamExt;
use signorebot_lib::config::{Config, Overlay, SharedConfig};
use signorebot_lib::overlay::hub::OverlayHub;
use signorebot_lib::overlay::server::{serve, ServerState};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

#[tokio::test]
async fn server_end_to_end() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a.mp3"), b"ID3fake").unwrap();
    std::fs::write(dir.path().join("evil.html"), b"<script>1</script>").unwrap();
    let mut cfg = Config::default();
    cfg.normalize();
    cfg.network.overlay_key = "secret".into();
    cfg.overlays.push(Overlay { id: "o1".into(), name: "Аудио".into(), path: "audio".into() });
    let config: SharedConfig = Arc::new(parking_lot::RwLock::new(cfg));
    let hub = OverlayHub::new();
    let state = ServerState { config, hub: hub.clone(), media_dir: dir.path().to_path_buf(), start_time: 42 };
    let cancel = CancellationToken::new();
    let (addr, handle) = serve(state, 0, false, cancel.clone()).await.unwrap();
    let base = format!("http://{addr}");
    let http = reqwest::Client::new();

    // health — публичный
    let h: serde_json::Value = http.get(format!("{base}/api/health")).send().await.unwrap().json().await.unwrap();
    assert_eq!(h["startTime"], 42);

    // страница: без ключа 403, с ключом 200, неизвестный path 404
    assert_eq!(http.get(format!("{base}/overlay/audio")).send().await.unwrap().status(), 403);
    let page = http.get(format!("{base}/overlay/audio?key=secret")).send().await.unwrap();
    assert_eq!(page.status(), 200);
    assert!(page.text().await.unwrap().contains("SignoreBot Overlay"));
    assert_eq!(http.get(format!("{base}/overlay/nope?key=secret")).send().await.unwrap().status(), 404);
    // встроенные шрифты: страница объявляет @font-face, файл отдаётся без ключа, чужое имя — 404
    let page_text = http.get(format!("{base}/overlay/audio?key=secret")).send().await.unwrap().text().await.unwrap();
    assert!(page_text.contains("@font-face{font-family:\"Inter\""), "нет @font-face в странице оверлея");
    let font = http.get(format!("{base}/fonts/Inter.ttf")).send().await.unwrap();
    assert_eq!(font.status(), 200);
    assert_eq!(font.headers().get("content-type").unwrap(), "font/ttf");
    assert!(font.bytes().await.unwrap().len() > 100_000);
    assert_eq!(http.get(format!("{base}/fonts/../Cargo.toml")).send().await.unwrap().status(), 404);
    assert_eq!(http.get(format!("{base}/fonts/nope.ttf")).send().await.unwrap().status(), 404);

    // медиа: ключ, traversal, nosniff для не-медиа
    assert_eq!(http.get(format!("{base}/media/a.mp3")).send().await.unwrap().status(), 403);
    let m = http.get(format!("{base}/media/a.mp3?key=secret")).send().await.unwrap();
    assert_eq!(m.status(), 200);
    assert!(m.headers()["content-type"].to_str().unwrap().starts_with("audio/"));
    assert_eq!(http.get(format!("{base}/media/..%2Fconfig.json?key=secret")).send().await.unwrap().status(), 400);
    let e = http.get(format!("{base}/media/evil.html?key=secret")).send().await.unwrap();
    assert_eq!(e.headers()["content-type"], "application/octet-stream");
    assert_eq!(e.headers()["content-disposition"], "attachment");

    // WS: без ключа отказ; с ключом — config, затем pushed-сообщение
    let ws_base = format!("ws://{addr}/ws");
    assert!(tokio_tungstenite::connect_async(format!("{ws_base}?path=audio&key=bad")).await.is_err());
    let (mut ws, _) = tokio_tungstenite::connect_async(format!("{ws_base}?path=audio&key=secret")).await.unwrap();
    let first = ws.next().await.unwrap().unwrap().into_text().unwrap();
    let v: serde_json::Value = serde_json::from_str(&first).unwrap();
    assert_eq!(v["command"], "config");
    assert_eq!(v["pauseBetweenMs"], 3000);
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    assert_eq!(hub.connections().len(), 1);
    assert!(hub.send_to_path("audio", r#"{"command":"playVideo"}"#));
    let second = ws.next().await.unwrap().unwrap().into_text().unwrap();
    assert!(second.contains("playVideo"));
    drop(ws);
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    assert!(hub.connections().is_empty());

    cancel.cancel();
    let _ = tokio::time::timeout(std::time::Duration::from_secs(5), handle).await;
}
