//! Движок без сети: чат-команда → медиа в очередь оверлея; антиспам; права; кулдаун.

use signorebot_lib::config::*;
use signorebot_lib::engine::{Engine, Ids};
use signorebot_lib::overlay::hub::OverlayHub;
use signorebot_lib::paths::AppPaths;
use signorebot_lib::secrets::Secrets;
use signorebot_lib::twitch::accounts::AuthManager;
use signorebot_lib::twitch::eventsub::{ChatMessage, TwitchEvent};
#[allow(unused_imports)]
use signorebot_lib::config::Response;
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
    cfg.overlays.push(Overlay { id: "o1".into(), name: "Аудио".into(), path: "audio".into(), fallback: None, fallback_enabled: false });
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

fn msg(message_id: &str, user_id: &str, text: &str) -> TwitchEvent {
    TwitchEvent::Chat(ChatMessage {
        message_id: message_id.into(),
        user_id: user_id.into(),
        user_login: "streamer".into(),
        user_name: "streamer".into(),
        text: text.into(),
        is_broadcaster: user_id == "1",
        is_moderator: false,
        is_vip: false,
        is_subscriber: false,
        reward_id: None,
    })
}

fn media_engine(dir: &std::path::Path) -> (Arc<Engine>, tokio::sync::mpsc::Receiver<String>) {
    let paths = AppPaths::new(dir);
    paths.ensure_dirs().unwrap();
    let mut cfg = Config::default();
    cfg.normalize();
    cfg.overlay_settings.antispam_window_ms = 0;
    cfg.overlays.push(Overlay { id: "o1".into(), name: "Аудио".into(), path: "audio".into(), fallback: None, fallback_enabled: false });
    let mut cmd = Command { name: "snd".into(), ..Default::default() };
    cmd.response.media.enabled = true;
    cmd.response.media.file = "a.mp3".into();
    cmd.response.media.overlay = Some("o1".into());
    cfg.commands.push(cmd);
    let config: SharedConfig = Arc::new(parking_lot::RwLock::new(cfg));
    let auth = AuthManager::new("cid".into(), Secrets::file_only(&paths));
    let helix = Arc::new(Helix::new(Arc::clone(&auth)));
    let hub = OverlayHub::new();
    let engine = Engine::new(config, auth, helix, hub.clone(), paths.deleted_messages_log());
    let (_id, rx) = hub.connect("audio", "test".into());
    (engine, rx)
}

/// Один аккаунт в обеих ролях: собственное эхо отсеивается по message_id,
/// а живые команды стримера работают.
#[tokio::test]
async fn same_account_echo_is_filtered_by_message_id() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, mut rx) = media_engine(dir.path());
    engine.set_ids(Some(Ids { broadcaster_id: "1".into(), bot_id: "1".into() }));

    // «наше» сообщение (id запомнен при отправке) — игнорируется, даже если похоже на команду
    engine.remember_sent("echo-1");
    engine.dispatch(msg("echo-1", "1", "!snd")).await;
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    assert!(rx.try_recv().is_err(), "эхо бота не должно исполнять команду");

    // живое сообщение стримера с тем же user_id — команда срабатывает
    engine.dispatch(msg("live-2", "1", "!snd")).await;
    let v: serde_json::Value = serde_json::from_str(&rx.recv().await.unwrap()).unwrap();
    assert_eq!(v["videoFile"], "a.mp3");
}

/// Разные аккаунты: всё от бота отсеивается по автору, как и раньше.
#[tokio::test]
async fn separate_bot_account_messages_are_ignored_by_author() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, mut rx) = media_engine(dir.path());
    engine.set_ids(Some(Ids { broadcaster_id: "1".into(), bot_id: "2".into() }));
    engine.dispatch(msg("x-1", "2", "!snd")).await;
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    assert!(rx.try_recv().is_err(), "сообщение от отдельного бота не команда");
    engine.dispatch(msg("x-2", "1", "!snd")).await;
    assert!(rx.recv().await.is_some(), "стример с отдельным ботом — команда работает");
}

fn overlay(id: &str, name: &str, path: &str, fallback: Option<Response>) -> Overlay {
    Overlay { id: id.into(), name: name.into(), path: path.into(), fallback_enabled: fallback.is_some(), fallback }
}

/// Оверлей с резервной реакцией: медиа не ждёт в очереди, резерв уходит на
/// другой оверлей; без резерва — старое поведение (очередь до подключения).
#[tokio::test]
async fn overlay_fallback_replaces_pending_queue() {
    let dir = tempfile::tempdir().unwrap();
    let paths = AppPaths::new(dir.path());
    paths.ensure_dirs().unwrap();
    let mut cfg = Config::default();
    cfg.normalize();
    cfg.overlay_settings.antispam_window_ms = 0;
    let mut fb = Response::default();
    fb.media.enabled = true;
    fb.media.file = "wait.gif".into();
    fb.media.overlay = Some("o-b".into());
    cfg.overlays.push(overlay("o-a", "Видео", "video", Some(fb)));
    cfg.overlays.push(overlay("o-b", "Аудио", "audio", None));
    cfg.overlays.push(overlay("o-c", "VIPS", "vips", None));
    let mut a = Command { name: "a".into(), ..Default::default() };
    a.response.media.enabled = true; a.response.media.file = "a.mp4".into(); a.response.media.overlay = Some("o-a".into());
    let mut c = Command { name: "c".into(), ..Default::default() };
    c.response.media.enabled = true; c.response.media.file = "c.mp4".into(); c.response.media.overlay = Some("o-c".into());
    cfg.commands.push(a); cfg.commands.push(c);
    let config: SharedConfig = Arc::new(parking_lot::RwLock::new(cfg));
    let auth = AuthManager::new("cid".into(), Secrets::file_only(&paths));
    let helix = Arc::new(Helix::new(Arc::clone(&auth)));
    let hub = OverlayHub::new();
    let engine = Engine::new(config, auth, helix, hub.clone(), paths.deleted_messages_log());
    let (_ib, mut rx_b) = hub.connect("audio", "t".into());

    // «Видео» не подключён, у него резерв → резервное медиа уходит на «Аудио»
    engine.dispatch(chat("alice", "!a")).await;
    let v: serde_json::Value = serde_json::from_str(&rx_b.recv().await.unwrap()).unwrap();
    assert_eq!(v["videoFile"], "wait.gif");
    // и в отложенной очереди «Видео» ничего нет: подключившись, он не получит a.mp4
    let (_ia, mut rx_a) = hub.connect("video", "t".into());
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    assert!(rx_a.try_recv().is_err(), "с резервом медиа не должно ждать в очереди");

    // «VIPS» без резерва: медиа ждёт и приходит при подключении
    engine.dispatch(chat("bob", "!c")).await;
    let (_ic, mut rx_c) = hub.connect("vips", "t".into());
    let v: serde_json::Value = serde_json::from_str(&rx_c.recv().await.unwrap()).unwrap();
    assert_eq!(v["videoFile"], "c.mp4");
}

/// Награда с медиа на выключенный оверлей попадает в список невыполненных
/// погашений; закрытие погашения в Twitch (EventSub) меняет статус.
#[tokio::test]
async fn unavailable_overlay_records_pending_redemption() {
    let dir = tempfile::tempdir().unwrap();
    let paths = AppPaths::new(dir.path());
    paths.ensure_dirs().unwrap();
    let mut cfg = Config::default();
    cfg.normalize();
    cfg.overlays.push(overlay("o-a", "Видео", "video", None));
    let mut rw = Reward { reward_id: "rw-1".into(), reward_title: "Бу!".into(), ..Default::default() };
    rw.response.media.enabled = true; rw.response.media.file = "boo.mp4".into(); rw.response.media.overlay = Some("o-a".into());
    cfg.rewards.push(rw);
    let config: SharedConfig = Arc::new(parking_lot::RwLock::new(cfg));
    let auth = AuthManager::new("cid".into(), Secrets::file_only(&paths));
    let helix = Arc::new(Helix::new(Arc::clone(&auth)));
    let hub = OverlayHub::new();
    let engine = Engine::new(config, auth, helix, hub.clone(), paths.deleted_messages_log());
    engine.dispatch(TwitchEvent::Redemption { redemption_id: "red-1".into(), reward_id: "rw-1".into(), reward_title: "Бу!".into(), user_name: "carol".into(), user_input: String::new() }).await;
    let list = engine.redemptions();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].status, "pending");
    assert_eq!(list[0].user, "carol");
    assert!(list[0].reason.contains("Видео"));
    // файл записан и переживает перезапуск движка
    assert!(dir.path().join("redemptions.json").exists());
    // модератор вернул баллы в очереди Twitch → статус canceled
    engine.dispatch(TwitchEvent::RedemptionUpdate { redemption_id: "red-1".into(), reward_id: "rw-1".into(), status: "canceled".into() }).await;
    assert_eq!(engine.redemptions()[0].status, "canceled");
    engine.dismiss_redemption("red-1");
    assert_eq!(engine.redemptions()[0].status, "dismissed");
}

/// Награда с возвратом баллов: медиа для выключенного оверлея не ждёт в
/// очереди (иначе зритель получил бы и баллы, и медиа через полминуты).
#[tokio::test]
async fn refund_reward_does_not_queue_media() {
    let dir = tempfile::tempdir().unwrap();
    let paths = AppPaths::new(dir.path());
    paths.ensure_dirs().unwrap();
    let mut cfg = Config::default();
    cfg.normalize();
    cfg.overlays.push(overlay("o-a", "Аудио", "audio", None));
    let mut rw = Reward { reward_id: "rw-1".into(), reward_title: "Привет".into(), managed: true, refund_if_unavailable: true, ..Default::default() };
    rw.response.media.enabled = true; rw.response.media.file = "hi.mp3".into(); rw.response.media.overlay = Some("o-a".into());
    let mut plain = Reward { reward_id: "rw-2".into(), reward_title: "Обычная".into(), ..Default::default() };
    plain.response.media.enabled = true; plain.response.media.file = "plain.mp3".into(); plain.response.media.overlay = Some("o-a".into());
    cfg.rewards.push(rw); cfg.rewards.push(plain);
    let config: SharedConfig = Arc::new(parking_lot::RwLock::new(cfg));
    let auth = AuthManager::new("cid".into(), Secrets::file_only(&paths));
    let helix = Arc::new(Helix::new(Arc::clone(&auth)));
    let hub = OverlayHub::new();
    let engine = Engine::new(config, auth, helix, hub.clone(), paths.deleted_messages_log());
    engine.dispatch(TwitchEvent::Redemption { redemption_id: "r1".into(), reward_id: "rw-1".into(), reward_title: "Привет".into(), user_name: "dan".into(), user_input: String::new() }).await;
    engine.dispatch(TwitchEvent::Redemption { redemption_id: "r2".into(), reward_id: "rw-2".into(), reward_title: "Обычная".into(), user_name: "eve".into(), user_input: String::new() }).await;
    // возврат без прав не прошёл, но запись есть; обычная награда — тоже в списке
    assert_eq!(engine.redemptions().len(), 2);
    // оверлей подключился: приходит только медиа обычной награды
    let (_i, mut rx) = hub.connect("audio", "t".into());
    let v: serde_json::Value = serde_json::from_str(&rx.recv().await.unwrap()).unwrap();
    assert_eq!(v["videoFile"], "plain.mp3");
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    assert!(rx.try_recv().is_err(), "медиа награды с возвратом не должно быть в очереди");
}

/// Чат опередил EventSub: реакция выполнена по чату, но учёт погашения
/// (список невыполненных) всё равно происходит по событию EventSub.
#[tokio::test]
async fn chat_first_redemption_still_recorded() {
    let dir = tempfile::tempdir().unwrap();
    let paths = AppPaths::new(dir.path());
    paths.ensure_dirs().unwrap();
    let mut cfg = Config::default();
    cfg.normalize();
    cfg.overlays.push(overlay("o-a", "Видео", "video", None));
    let mut rw = Reward { reward_id: "rw-1".into(), reward_title: "Скажи".into(), ..Default::default() };
    rw.response.media.enabled = true; rw.response.media.file = "say.mp4".into(); rw.response.media.overlay = Some("o-a".into());
    cfg.rewards.push(rw);
    let config: SharedConfig = Arc::new(parking_lot::RwLock::new(cfg));
    let auth = AuthManager::new("cid".into(), Secrets::file_only(&paths));
    let helix = Arc::new(Helix::new(Arc::clone(&auth)));
    let hub = OverlayHub::new();
    let engine = Engine::new(config, auth, helix, hub.clone(), paths.deleted_messages_log());
    // сообщение чата с признаком награды пришло первым
    let mut m = chat("frank", "привет всем");
    if let TwitchEvent::Chat(ref mut c) = m { c.reward_id = Some("rw-1".into()); }
    engine.dispatch(m).await;
    assert!(engine.redemptions().is_empty(), "по чату id погашения нет — записи быть не должно");
    // следом — событие погашения с тем же зрителем и текстом
    engine.dispatch(TwitchEvent::Redemption { redemption_id: "red-9".into(), reward_id: "rw-1".into(), reward_title: "Скажи".into(), user_name: "frank".into(), user_input: "привет всем".into() }).await;
    let list = engine.redemptions();
    assert_eq!(list.len(), 1, "учёт погашения должен довершиться по EventSub");
    assert_eq!(list[0].redemption_id, "red-9");
    assert_eq!(list[0].status, "pending");
    // и медиа при этом отправлено один раз (лежит в очереди одно сообщение)
    assert_eq!(hub.pending_count("video"), 1);
}

/// Кулдаун на зрителя: тот же зритель ждёт, другой — проходит.
#[tokio::test]
async fn per_user_cooldown() {
    let dir = tempfile::tempdir().unwrap();
    let paths = AppPaths::new(dir.path());
    paths.ensure_dirs().unwrap();
    let mut cfg = Config::default();
    cfg.normalize();
    cfg.overlay_settings.antispam_window_ms = 0;
    cfg.overlays.push(overlay("o-a", "Аудио", "audio", None));
    let mut cmd = Command { name: "дзынь".into(), cooldown_user_sec: 60, ..Default::default() };
    cmd.response.media.enabled = true; cmd.response.media.file = "ding.mp3".into(); cmd.response.media.overlay = Some("o-a".into());
    cfg.commands.push(cmd);
    let config: SharedConfig = Arc::new(parking_lot::RwLock::new(cfg));
    let auth = AuthManager::new("cid".into(), Secrets::file_only(&paths));
    let helix = Arc::new(Helix::new(Arc::clone(&auth)));
    let hub = OverlayHub::new();
    let engine = Engine::new(config, auth, helix, hub.clone(), paths.deleted_messages_log());
    let (_i, mut rx) = hub.connect("audio", "t".into());
    engine.dispatch(chat("alice", "!дзынь")).await;
    engine.dispatch(chat("alice", "!дзынь")).await; // кулдаун — игнор
    engine.dispatch(chat("bob", "!дзынь")).await;   // другой зритель — проходит
    let mut got = 0;
    while rx.try_recv().is_ok() { got += 1; }
    assert_eq!(got, 2);
}

/// «Стоп» у неподключённого оверлея выбрасывает отложенную очередь.
#[test]
fn clear_pending_drops_queue() {
    let hub = OverlayHub::new();
    assert!(!hub.send_to_path("audio", "{\"a\":1}"));
    assert!(!hub.send_to_path("audio", "{\"a\":2}"));
    assert_eq!(hub.pending_count("audio"), 2);
    assert_eq!(hub.clear_pending("audio"), 2);
    assert_eq!(hub.pending_count("audio"), 0);
}

/// Алерт без файла: включённый текст уходит на оверлей и без медиафайла.
#[tokio::test]
async fn text_only_alert_is_sent() {
    let dir = tempfile::tempdir().unwrap();
    let paths = AppPaths::new(dir.path());
    paths.ensure_dirs().unwrap();
    let mut cfg = Config::default();
    cfg.normalize();
    cfg.overlays.push(overlay("o-a", "Видео", "video", None));
    let mut cmd = Command { name: "привет".into(), ..Default::default() };
    cmd.response.media.enabled = true;
    cmd.response.media.overlay = Some("o-a".into());
    cmd.response.media.text.enabled = true;
    cmd.response.media.text.content = "Привет, {user}!".into();
    cmd.response.media.image_duration_sec = Some(3.0);
    let mut empty = Command { name: "пусто".into(), ..Default::default() };
    empty.response.media.enabled = true; // ни файла, ни текста — на оверлей ничего не идёт
    cfg.commands.push(cmd); cfg.commands.push(empty);
    let config: SharedConfig = Arc::new(parking_lot::RwLock::new(cfg));
    let auth = AuthManager::new("cid".into(), Secrets::file_only(&paths));
    let helix = Arc::new(Helix::new(Arc::clone(&auth)));
    let hub = OverlayHub::new();
    let engine = Engine::new(config, auth, helix, hub.clone(), paths.deleted_messages_log());
    let (_i, mut rx) = hub.connect("video", "t".into());
    engine.dispatch(chat("gina", "!привет")).await;
    let v: serde_json::Value = serde_json::from_str(&rx.recv().await.unwrap()).unwrap();
    assert_eq!(v["videoFile"], "");
    assert_eq!(v["text"]["content"], "Привет, gina!");
    assert_eq!(v["duration"], 3.0);
    engine.dispatch(chat("gina", "!пусто")).await;
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    assert!(rx.try_recv().is_err(), "пустое медиа отправляться не должно");
}
