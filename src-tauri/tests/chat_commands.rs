//! Команды в наградах и уведомлениях: реальные payload → парсер → движок → оверлей.

use signorebot_lib::config::*;
use signorebot_lib::engine::{Engine, Ids};
use signorebot_lib::overlay::hub::OverlayHub;
use signorebot_lib::paths::AppPaths;
use signorebot_lib::secrets::Secrets;
use signorebot_lib::twitch::accounts::AuthManager;
use signorebot_lib::twitch::eventsub::parse_notifications;
use signorebot_lib::twitch::helix::Helix;
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::sync::mpsc::Receiver;

fn response(file: &str) -> Response {
    let mut r = Response::default();
    r.media.enabled = true;
    r.media.file = file.into();
    r.media.overlay = Some("o".into());
    r.media.text.enabled = true;
    r.media.text.content = "{user}: {message}".into();
    r
}

fn engine() -> (Arc<Engine>, Receiver<String>, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let paths = AppPaths::new(dir.path());
    paths.ensure_dirs().unwrap();
    let mut cfg = Config::default();
    cfg.normalize();
    cfg.overlay_settings.antispam_window_ms = 0;
    cfg.overlays.push(Overlay { id: "o".into(), name: "Аудио".into(), path: "audio".into(), ..Default::default() });
    cfg.commands.push(Command { name: "snd".into(), aliases: vec!["sound".into()], response: response("command.mp3"), ..Default::default() });
    let config: SharedConfig = Arc::new(parking_lot::RwLock::new(cfg));
    let auth = AuthManager::new("cid".into(), Secrets::file_only(&paths));
    let helix = Arc::new(Helix::new(Arc::clone(&auth)));
    let hub = OverlayHub::new();
    let engine = Engine::new(config, auth, helix, hub.clone(), paths.deleted_messages_log());
    let (_, rx) = hub.connect("audio", "test".into());
    (engine, rx, dir)
}

fn payload(id: &str, text: &str) -> Value {
    json!({
        "message_id": id, "chatter_user_id": "2", "chatter_user_login": "alice",
        "chatter_user_name": "Алиса", "chatter_is_anonymous": false,
        "message": { "text": text }, "badges": [], "channel_points_custom_reward_id": null,
        "notice_type": "watch_streak", "watch_streak": { "streak_count": 5, "channel_points_awarded": 10 },
        "system_message": "Алиса смотрит пятый стрим подряд"
    })
}

async fn deliver(engine: &Engine, topic: &str, payload: &Value) {
    for event in parse_notifications(topic, payload) {
        engine.dispatch(event).await;
    }
}

fn alerts(rx: &mut Receiver<String>) -> Vec<Value> {
    let mut result = vec![];
    while let Ok(text) = rx.try_recv() {
        result.push(serde_json::from_str(&text).unwrap());
    }
    result
}

#[tokio::test]
async fn reward_and_command_both_run_once_in_either_delivery_order() {
    for redemption_first in [false, true] {
        let (engine, mut rx, _dir) = engine();
        engine.config.write().rewards.push(Reward { reward_id: "rw".into(), reward_title: "Награда".into(), response: response("reward.mp3"), ..Default::default() });
        let mut chat = payload("chat-1", "!sound привет");
        chat["channel_points_custom_reward_id"] = json!("rw");
        let redemption = json!({ "id": "red-1", "reward": { "id": "rw", "title": "Награда" }, "user_name": "Алиса", "user_login": "alice", "user_input": "!sound привет" });
        let topics = ["channel.chat.message", "channel.channel_points_custom_reward_redemption.add"];
        let inputs = [&chat, &redemption];
        let order = if redemption_first { [1, 0] } else { [0, 1] };
        for i in order {
            deliver(&engine, topics[i], inputs[i]).await;
        }
        let output = alerts(&mut rx);
        assert_eq!(output.len(), 2, "порядок: redemption_first={redemption_first}");
        assert_eq!(output.iter().filter(|v| v["videoFile"] == "reward.mp3").count(), 1);
        let command = output.iter().find(|v| v["videoFile"] == "command.mp3").unwrap();
        assert_eq!(command["text"]["content"], "Алиса: привет");
        // Повторная доставка обоих событий не повторяет ни команду, ни награду.
        for i in order {
            deliver(&engine, topics[i], inputs[i]).await;
        }
        assert!(alerts(&mut rx).is_empty());
    }
}

#[tokio::test]
async fn missing_or_disabled_reward_does_not_swallow_a_command() {
    for configured in [false, true] {
        let (engine, mut rx, _dir) = engine();
        if configured {
            engine.config.write().rewards.push(Reward { reward_id: "rw".into(), enabled: false, response: response("reward.mp3"), ..Default::default() });
        }
        let mut chat = payload("reward-reply", "@parent !sound @bob привет");
        chat["channel_points_custom_reward_id"] = json!("rw");
        chat["reply"] = json!({ "parent_user_login": "parent", "parent_user_name": "Parent" });
        deliver(&engine, "channel.chat.message", &chat).await;
        let output = alerts(&mut rx);
        assert_eq!(output.len(), 1);
        assert_eq!(output[0]["videoFile"], "command.mp3");
        assert_eq!(output[0]["text"]["content"], "Алиса: @bob привет");
    }
}

#[tokio::test]
async fn watch_streak_runs_both_the_event_reaction_and_the_command() {
    let (engine, mut rx, _dir) = engine();
    engine.config.write().events.insert("watchStreak".into(), EventReaction { enabled: true, response: response("streak.mp3"), ..Default::default() });
    deliver(&engine, "channel.chat.notification", &payload("streak-1", "!snd привет")).await;
    let output = alerts(&mut rx);
    assert_eq!(output.len(), 2);
    assert_eq!(output[0]["videoFile"], "streak.mp3");
    assert_eq!(output[0]["text"]["content"], "Алиса: !snd привет");
    assert_eq!(output[1]["videoFile"], "command.mp3");
    assert_eq!(output[1]["text"]["content"], "Алиса: привет");
}

#[tokio::test]
async fn notification_commands_are_deduplicated_by_message_id_not_by_text() {
    for notice in ["watch_streak", "resub", "announcement", "bits_badge_tier", "modiversary"] {
        for notification_first in [false, true] {
            let (engine, mut rx, _dir) = engine();
            let mut chat = payload("shared-id", "!snd");
            chat["notice_type"] = json!(notice);
            let topics = if notification_first { ["channel.chat.notification", "channel.chat.message"] } else { ["channel.chat.message", "channel.chat.notification"] };
            for topic in topics {
                deliver(&engine, topic, &chat).await;
            }
            assert_eq!(alerts(&mut rx).len(), 1, "{notice}, notification_first={notification_first}");
            chat["message_id"] = json!("new-id");
            deliver(&engine, "channel.chat.notification", &chat).await;
            assert_eq!(alerts(&mut rx).len(), 1, "новая такая же команда должна срабатывать");
        }
    }
}

#[tokio::test]
async fn commands_keep_the_authors_permissions_in_rewards_and_notifications() {
    let cases = [("moderators", "moderator"), ("moderators", "broadcaster"), ("broadcaster", "broadcaster"), ("vips", "vip"), ("subscribers", "subscriber"), ("subscribers", "founder"), ("user:alice", "")];
    for (permission, badge) in cases {
        for topic in ["channel.chat.message", "channel.chat.notification"] {
            for allowed in [false, true] {
                let (engine, mut rx, _dir) = engine();
                engine.config.write().commands[0].permissions = vec![permission.into()];
                let mut chat = payload("permission", "!snd");
                if topic == "channel.chat.message" { chat["channel_points_custom_reward_id"] = json!("rw"); }
                if allowed && !badge.is_empty() { chat["badges"] = json!([{ "set_id": badge, "id": "1", "info": "" }]); }
                if !allowed && permission == "user:alice" { chat["chatter_user_login"] = json!("bob"); }
                deliver(&engine, topic, &chat).await;
                assert_eq!(alerts(&mut rx).len(), usize::from(allowed), "{topic}, {permission}, badge={badge}, allowed={allowed}");
            }
        }
    }
}

#[tokio::test]
async fn notification_commands_share_the_regular_per_user_cooldown() {
    let (engine, mut rx, _dir) = engine();
    engine.config.write().commands[0].cooldown_user_sec = 60;
    deliver(&engine, "channel.chat.notification", &payload("first", "!snd")).await;
    deliver(&engine, "channel.chat.message", &payload("second", "!sound")).await;
    let mut other = payload("third", "!snd");
    other["chatter_user_id"] = json!("3");
    other["chatter_user_login"] = json!("bob");
    deliver(&engine, "channel.chat.notification", &other).await;
    assert_eq!(alerts(&mut rx).len(), 2);
}

#[tokio::test]
async fn duplicate_echo_cannot_become_a_command_in_shared_account_mode() {
    for notification_first in [false, true] {
        let (engine, mut rx, _dir) = engine();
        engine.set_ids(Some(Ids { broadcaster_id: "2".into(), bot_id: "2".into() }));
        engine.remember_sent("echo");
        let chat = payload("echo", "!snd");
        let topics = if notification_first { ["channel.chat.notification", "channel.chat.message"] } else { ["channel.chat.message", "channel.chat.notification"] };
        for topic in topics { deliver(&engine, topic, &chat).await; }
        assert!(alerts(&mut rx).is_empty(), "повторное эхо бота");
        deliver(&engine, "channel.chat.notification", &payload("manual", "!snd")).await;
        assert_eq!(alerts(&mut rx).len(), 1, "ручная команда стримера");
    }
}

#[tokio::test]
async fn anonymous_or_incomplete_notifications_do_not_execute_commands() {
    for field in ["chatter_is_anonymous", "chatter_user_id", "chatter_user_login", "message_id"] {
        let (engine, mut rx, _dir) = engine();
        let mut chat = payload("invalid", "!snd");
        chat[field] = if field == "chatter_is_anonymous" { json!(true) } else { json!("") };
        deliver(&engine, "channel.chat.notification", &chat).await;
        assert!(alerts(&mut rx).is_empty(), "{field}");
    }
}
