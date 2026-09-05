//! HTTP/WebSocket-сервер оверлеев (axum). Единственная сетевая поверхность
//! приложения. Все страницы, медиа и WS защищены ключом из конфига.

use super::hub::OverlayHub;
use crate::config::SharedConfig;
use axum::{

    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        ConnectInfo, Path, Query, State,
    },
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use serde::Deserialize;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use tokio_util::sync::CancellationToken;

const OVERLAY_HTML_TEMPLATE: &str = include_str!("../../overlay/overlay.html");
/// Общая раскладка (та же, что импортирует панель для предпросмотра).
pub const OVERLAY_CSS: &str = include_str!("../../overlay/overlay.css");
pub const OVERLAY_SW: &str = include_str!("../../overlay/overlay-sw.js");

/// Страница оверлея со встроенным общим CSS.
pub fn overlay_html() -> &'static str {
    static HTML: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    HTML.get_or_init(|| OVERLAY_HTML_TEMPLATE.replace("/*__SHARED_CSS__*/", &format!("{}\n{}", super::fonts_gen::FONT_FACE_CSS, OVERLAY_CSS)))
}

#[derive(Clone)]
pub struct ServerState {
    pub config: SharedConfig,
    pub hub: OverlayHub,
    pub media_dir: PathBuf,
    pub start_time: i64,
}

#[derive(Deserialize)]
pub struct KeyQuery {
    #[serde(default)]
    key: String,
    #[serde(default)]
    path: String,
}

fn key_ok(state: &ServerState, key: &str) -> bool {
    let cfg = state.config.read();
    !cfg.network.overlay_key.is_empty() && cfg.network.overlay_key == key
}

fn forbidden() -> Response {
    (StatusCode::FORBIDDEN, "SignoreBot: неверный или отсутствующий ключ оверлея").into_response()
}

fn no_cache(mut r: Response) -> Response {
    let h = r.headers_mut();
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache, no-store, must-revalidate"));
    h.insert(header::PRAGMA, HeaderValue::from_static("no-cache"));
    h.insert(header::EXPIRES, HeaderValue::from_static("0"));
    r
}

async fn overlay_page(State(st): State<ServerState>, Path(path): Path<String>, Query(q): Query<KeyQuery>) -> Response {
    if !key_ok(&st, &q.key) {
        st.hub.note_page_request(&path, false);
        tracing::warn!(target: "signorebot::overlay", "Страница оверлея «{path}» запрошена {} — отказ. В адресе источника должен быть ?key=…: скопируйте его кнопкой «Копировать URL» на вкладке «Оверлеи»", if q.key.is_empty() { "без ключа" } else { "с неверным ключом" });
        return forbidden();
    }
    st.hub.note_page_request(&path, true);
    tracing::debug!(target: "signorebot::overlay", "Страница оверлея «{path}» запрошена");
    if st.config.read().overlay_by_path(&path).is_none() {
        return (StatusCode::NOT_FOUND, "SignoreBot: оверлей не найден").into_response();
    }
    no_cache(([(header::CONTENT_TYPE, "text/html; charset=utf-8")], overlay_html()).into_response())
}

async fn overlay_sw() -> Response {
    no_cache(([(header::CONTENT_TYPE, "application/javascript; charset=utf-8")], OVERLAY_SW).into_response())
}

async fn health(State(st): State<ServerState>) -> Response {
    no_cache(axum::Json(serde_json::json!({ "ok": true, "startTime": st.start_time })).into_response())
}

async fn overlay_info(State(st): State<ServerState>, Path(path): Path<String>, Query(q): Query<KeyQuery>) -> Response {
    if !key_ok(&st, &q.key) {
        return forbidden();
    }
    let cfg = st.config.read();
    match cfg.overlay_by_path(&path) {
        Some(o) => axum::Json(serde_json::json!({
            "id": o.id, "name": o.name, "path": o.path,
            "settings": cfg.overlay_settings,
        }))
        .into_response(),
        None => (StatusCode::NOT_FOUND, "оверлей не найден").into_response(),
    }
}

/// Встроенные шрифты для текста на оверлее. Без ключа: файлы открытые (OFL),
/// а @font-face в странице не может подставить ключ.
async fn font_file(Path(file): Path<String>) -> Response {
    let Some((_, bytes)) = super::fonts_gen::FONTS.iter().find(|(n, _)| *n == file) else {
        return (StatusCode::NOT_FOUND, "шрифт не найден").into_response();
    };
    let mime = if file.ends_with(".otf") { "font/otf" } else { "font/ttf" };
    ([(header::CONTENT_TYPE, HeaderValue::from_static(mime)), (header::CACHE_CONTROL, HeaderValue::from_static("public, max-age=31536000, immutable")), (header::ACCESS_CONTROL_ALLOW_ORIGIN, HeaderValue::from_static("*"))], *bytes).into_response()
}

async fn media(State(st): State<ServerState>, Path(file): Path<String>, Query(q): Query<KeyQuery>, req: axum::extract::Request) -> Response {
    if !key_ok(&st, &q.key) {
        return forbidden();
    }
    let Some(name) = crate::paths::safe_file_name(&file) else {
        return (StatusCode::BAD_REQUEST, "недопустимое имя файла").into_response();
    };
    let full = st.media_dir.join(&name);
    if !full.is_file() {
        return (StatusCode::NOT_FOUND, "файл не найден").into_response();
    }
    // tower-http ServeFile: поддержка Range для видео.
    let svc = tower_http::services::ServeFile::new(&full);
    let mut resp = match tower::ServiceExt::oneshot(svc, req).await {
        Ok(r) => r.into_response(),
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "ошибка чтения файла").into_response(),
    };
    let h = resp.headers_mut();
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("private, max-age=3600"));
    h.insert("X-Content-Type-Options", HeaderValue::from_static("nosniff"));
    let mime = mime_guess::from_path(&full).first_or_octet_stream();
    // Всё, что не медиа, отдаём как скачивание — от XSS на origin оверлея.
    let t = mime.type_();
    if t != mime::IMAGE && t != mime::VIDEO && t != mime::AUDIO {
        h.insert(header::CONTENT_TYPE, HeaderValue::from_static("application/octet-stream"));
        h.insert(header::CONTENT_DISPOSITION, HeaderValue::from_static("attachment"));
    }
    resp
}

async fn ws_upgrade(
    State(st): State<ServerState>,
    Query(q): Query<KeyQuery>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    ws: WebSocketUpgrade,
) -> Response {
    if !key_ok(&st, &q.key) {
        return forbidden();
    }
    let path = q.path.clone();
    if path.is_empty() || st.config.read().overlay_by_path(&path).is_none() {
        return (StatusCode::NOT_FOUND, "оверлей не найден").into_response();
    }
    ws.on_upgrade(move |socket| ws_session(st, socket, path, addr.ip().to_string()))
}

async fn ws_session(st: ServerState, mut socket: WebSocket, path: String, remote: String) {
    let (id, mut rx) = st.hub.connect(&path, remote);
    // Настройки оверлея — первым сообщением.
    let settings = st.config.read().overlay_settings.clone();
    let hello = serde_json::json!({ "command": "config", "pauseBetweenMs": settings.pause_between_ms, "imageDurationSec": settings.image_duration_sec });
    if socket.send(Message::Text(hello.to_string().into())).await.is_err() {
        st.hub.disconnect(id);
        return;
    }
    loop {
        tokio::select! {
            out = rx.recv() => {
                match out {
                    Some(text) => {
                        if socket.send(Message::Text(text.into())).await.is_err() { break; }
                    }
                    None => break,
                }
            }
            inc = socket.recv() => {
                match inc {
                    Some(Ok(Message::Close(_))) | None | Some(Err(_)) => break,
                    Some(Ok(Message::Ping(d))) => { let _ = socket.send(Message::Pong(d)).await; }
                    _ => {} // от оверлея ничего не ждём
                }
            }
        }
    }
    st.hub.disconnect(id);
}

pub fn router(state: ServerState) -> Router {
    Router::new()
        .route("/", get(|| async { "SignoreBot overlay server" }))
        .route("/overlay/{path}", get(overlay_page))
        .route("/overlay-sw.js", get(overlay_sw))
        .route("/media/{file}", get(media))
        .route("/fonts/{file}", get(font_file))
        .route("/ws", get(ws_upgrade))
        .route("/api/health", get(health))
        .route("/api/overlay-info/{path}", get(overlay_info))
        .fallback(|| async { (StatusCode::NOT_FOUND, "SignoreBot: не найдено") })
        .with_state(state)
}

#[derive(Debug, thiserror::Error)]
pub enum ServeError {
    #[error("не удалось занять порт {0}: {1}")]
    Bind(u16, std::io::Error),
}

/// Запустить сервер; завершается по `cancel`. Возвращает фактический адрес.
pub async fn serve(state: ServerState, port: u16, allow_lan: bool, cancel: CancellationToken) -> Result<(SocketAddr, tokio::task::JoinHandle<()>), ServeError> {
    let ip = if allow_lan { IpAddr::V4(Ipv4Addr::UNSPECIFIED) } else { IpAddr::V4(Ipv4Addr::LOCALHOST) };
    let listener = tokio::net::TcpListener::bind(SocketAddr::new(ip, port)).await.map_err(|e| ServeError::Bind(port, e))?;
    let addr = listener.local_addr().map_err(|e| ServeError::Bind(port, e))?;
    let app = router(state).into_make_service_with_connect_info::<SocketAddr>();
    let handle = tokio::spawn(async move {
        let _ = axum::serve(listener, app).with_graceful_shutdown(async move { cancel.cancelled().await }).await;
    });
    Ok((addr, handle))
}

/// Публичный адрес (LAN IPv4) для URL оверлеев.
pub fn local_ip() -> String {
    local_ip_address::local_ip().map(|ip| ip.to_string()).unwrap_or_else(|_| "127.0.0.1".into())
}

pub fn overlay_url(host: &str, port: u16, path: &str, key: &str) -> String {
    format!("http://{host}:{port}/overlay/{path}?key={key}")
}

