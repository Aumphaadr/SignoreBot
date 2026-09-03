//! Движок без сети: чат-команда → медиа в очередь оверлея; антиспам; права; кулдаун.

use signorebot_lib::config::*;
use signorebot_lib::engine::Engine;
use signorebot_lib::overlay::hub::OverlayHub;
use signorebot_lib::paths::AppPaths;
use signorebot_lib::secrets::Secrets;
use signorebot_lib::twitch::accounts::AuthManager;
use signorebot_lib::twitch::eventsub::{ChatMessage, TwitchEvent};
use signorebot_lib::twitch::helix::Helix;
use std::sync::Arc;

fn chat(login: &str, text: &str) -> TwitchEvent {
    TwitchEvent::Chat(ChatMessage {
        message_id: "m".into(),
        user_id: "1".into(),
        user_login: login.into(),
        user_name: login.into(),
        text: text.into(),
        is_broadcaster: false,
        is_moderator: false,
        is_vip: false,
        is_subscriber: false,
        reward_id: None,
    })
}

#[tokio::test]
async fn command_sends_media_with_antispam_and_permissions() {
    let dir = tempfile::tempdir().unwrap();
    let paths = AppPaths::new(dir.path());
    paths.ensure_dirs().unwrap();
    let mut cfg = Config::default();
    cfg.normalize();
    cfg.overlays.push(Overlay { id: "o1".into(), name: "Аудио".into(), path: "audio".into() });
    let mut cmd = Command { name: "звук".into(), aliases: vec!["snd".into()], ..Default::default() };
    cmd.response.media.enabled = true;
    cmd.response.media.file = "a.mp3".into();
    cmd.response.media.overlay = Some("o1".into());
    cmd.response.media.text.enabled = true;
    cmd.response.media.text.content = "от {user}: {message}".into();
    cfg.commands.push(cmd);
    let mut modcmd = Command { name: "mod".into(), permissions: vec!["moderators".into()], ..Default::default() };
    modcmd.response.media.enabled = true;
    modcmd.response.media.file = "m.mp3".into();
    cfg.commands.push(modcmd);
    let config: SharedConfig = Arc::new(parking_lot::RwLock::new(cfg));

    let auth = AuthManager::new("cid".into(), Secrets::file_only(&paths));
    let helix = Arc::new(Helix::new(Arc::clone(&auth)));
    let hub = OverlayHub::new();
    let engine = Engine::new(config, auth, helix, hub.clone(), paths.deleted_messages_log());
    let (_id, mut rx) = hub.connect("audio", "test".into());

    // команда по алиасу, с аргументами → медиа с подставленным текстом
    engine.dispatch(chat("alice", "!SND привет мир")).await;
    let msg = rx.recv().await.unwrap();
    let v: serde_json::Value = serde_json::from_str(&msg).unwrap();
    assert_eq!(v["command"], "playVideo");
    assert_eq!(v["videoFile"], "a.mp3");
    assert_eq!(v["text"]["content"], "от alice: привет мир");

    // антиспам: тот же файл от той же alice сразу — отброшен; от bob — доставлен
    engine.dispatch(chat("alice", "!звук")).await;
    engine.dispatch(chat("bob", "!звук")).await;
    let msg = tokio::time::timeout(std::time::Duration::from_millis(200), rx.recv()).await.unwrap().unwrap();
    assert!(msg.contains("a.mp3"));
    assert!(tokio::time::timeout(std::time::Duration::from_millis(100), rx.recv()).await.is_err(), "лишнее сообщение");

    // права: не модератор — ничего
    engine.dispatch(chat("carol", "!mod")).await;
    assert!(tokio::time::timeout(std::time::Duration::from_millis(100), rx.recv()).await.is_err());

    // неизвестная команда и обычный текст — ничего
    engine.dispatch(chat("carol", "!nope")).await;
    engine.dispatch(chat("carol", "просто текст")).await;
    assert!(tokio::time::timeout(std::time::Duration::from_millis(100), rx.recv()).await.is_err());
}
