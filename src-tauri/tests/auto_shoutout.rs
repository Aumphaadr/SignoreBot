//! Действия EventSub → общий авто-шатаут; отдельный режим для рейдов.

use serde_json::{json, Value};
use signorebot_lib::config::{Config, RaidShoutoutMode, SharedConfig};
use signorebot_lib::engine::shoutout::{SendOutcome, ShoutoutSource};
use signorebot_lib::engine::Engine;
use signorebot_lib::overlay::hub::OverlayHub;
use signorebot_lib::paths::AppPaths;
use signorebot_lib::secrets::Secrets;
use signorebot_lib::twitch::{accounts::AuthManager, eventsub::parse_notifications, helix::Helix};
use std::{sync::Arc, time::Duration};

fn engine(mode: RaidShoutoutMode) -> (Arc<Engine>, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let paths = AppPaths::new(dir.path());
    paths.ensure_dirs().unwrap();
    let mut cfg = Config::default();
    cfg.normalize();
    cfg.shoutout.auto_list = vec!["alice".into()];
    cfg.shoutout.raid_mode = mode;
    let config: SharedConfig = Arc::new(parking_lot::RwLock::new(cfg));
    let auth = AuthManager::new("cid".into(), Secrets::file_only(&paths));
    let helix = Arc::new(Helix::new(Arc::clone(&auth)));
    (Engine::new(config, auth, helix, OverlayHub::new(), paths.deleted_messages_log()), dir)
}

fn user_event() -> Value {
    // Отображаемое имя намеренно отличается: сопоставлять список нужно по логину.
    json!({ "user_id": "2", "user_login": "alice", "user_name": "ДругоеИмя", "tier": "1000", "is_gift": false, "is_anonymous": false, "total": 1, "bits": 100, "message": { "text": "" }, "id": "r1", "reward": { "id": "rw", "title": "Награда" }, "user_input": "" })
}

fn notice(kind: &str, text: &str) -> Value {
    json!({ "notice_type": kind, "message_id": uuid::Uuid::new_v4().to_string(), "chatter_user_id": "2", "chatter_user_login": "alice", "chatter_user_name": "ДругоеИмя", "chatter_is_anonymous": false, "message": { "text": text }, "badges": [], "system_message": "Событие канала", "watch_streak": { "streak_count": 5, "channel_points_awarded": 10 } })
}

async fn deliver(engine: &Engine, topic: &str, ev: &Value) {
    for event in parse_notifications(topic, ev) {
        engine.dispatch(event).await;
    }
}

fn finish(engine: &Engine, outcome: SendOutcome) {
    let (item, _) = engine.shoutout.next_ready().unwrap();
    assert!(engine.shoutout.begin(item.id));
    engine.shoutout.finish(item.id, outcome, Duration::ZERO);
}

#[tokio::test]
async fn listed_users_get_auto_shoutouts_for_events_without_chat_text() {
    for topic in ["channel.follow", "channel.subscribe", "channel.subscription.message", "channel.subscription.gift", "channel.cheer", "channel.channel_points_custom_reward_redemption.add", "channel.channel_points_automatic_reward_redemption.add"] {
        for with_text in [false, true] {
            let (engine, _dir) = engine(RaidShoutoutMode::None);
            let mut event = user_event();
            event["user_input"] = json!(if with_text { "Привет!" } else { "" });
            event["message"] = if topic == "channel.cheer" { json!(if with_text { "Привет!" } else { "" }) } else { json!({ "text": if with_text { "Привет!" } else { "" } }) };
            deliver(&engine, topic, &event).await;
            let status = engine.shoutout.status();
            assert_eq!(status.queue.len(), 1, "{topic}, with_text={with_text}");
            assert_eq!(status.queue[0].username, "alice");
            assert_eq!(status.queue[0].source, ShoutoutSource::Activity);
        }
    }
}

#[tokio::test]
async fn notifications_without_text_include_streaks_subscriptions_and_gifts() {
    for kind in ["watch_streak", "sub", "resub", "sub_gift", "community_sub_gift", "gift_paid_upgrade", "prime_paid_upgrade", "announcement", "bits_badge_tier", "modiversary"] {
        for with_text in [false, true] {
            let (engine, _dir) = engine(RaidShoutoutMode::None);
            deliver(&engine, "channel.chat.notification", &notice(kind, if with_text { "Привет!" } else { "" })).await;
            let status = engine.shoutout.status();
            assert_eq!(status.queue.len(), 1, "{kind}, with_text={with_text}");
            assert_eq!(status.queue[0].source, ShoutoutSource::Activity);
        }
    }
}

#[tokio::test]
async fn a_gift_can_shout_out_both_the_listed_donor_and_the_listed_recipient() {
    let (engine, _dir) = engine(RaidShoutoutMode::None);
    engine.config.write().shoutout.auto_list.push("bob".into());
    deliver(&engine, "channel.subscription.gift", &user_event()).await;
    let mut received = user_event();
    received["user_login"] = json!("bob");
    received["user_id"] = json!("3");
    received["is_gift"] = json!(true);
    deliver(&engine, "channel.subscribe", &received).await;
    let users: Vec<_> = engine.shoutout.status().queue.into_iter().map(|q| q.username).collect();
    assert_eq!(users, ["alice", "bob"]);
}

#[tokio::test]
async fn chat_events_and_repeat_deliveries_share_one_auto_shoutout() {
    for message_first in [false, true] {
        for already_done in [false, true] {
            let (engine, _dir) = engine(RaidShoutoutMode::None);
            let mut chat = notice("watch_streak", "Привет!");
            chat["channel_points_custom_reward_id"] = json!("rw");
            let first = if message_first { "channel.chat.message" } else { "channel.follow" };
            let payload = if message_first { chat.clone() } else { user_event() };
            deliver(&engine, first, &payload).await;
            assert_eq!(engine.shoutout.status().queue.len(), 1);
            if already_done { finish(&engine, SendOutcome::Ok); }
            for topic in ["channel.follow", "channel.subscribe", "channel.subscription.message", "channel.subscription.gift", "channel.cheer", "channel.channel_points_custom_reward_redemption.add"] {
                deliver(&engine, topic, &user_event()).await;
            }
            deliver(&engine, "channel.chat.message", &chat).await;
            deliver(&engine, "channel.chat.notification", &chat).await;
            deliver(&engine, "channel.chat.notification", &chat).await;
            assert_eq!(engine.shoutout.status().queue.len(), usize::from(!already_done));
        }
    }
}

#[tokio::test]
async fn unknown_anonymous_and_unlisted_users_are_not_enqueued() {
    for topic in ["channel.follow", "channel.subscribe", "channel.subscription.message", "channel.subscription.gift", "channel.cheer", "channel.channel_points_custom_reward_redemption.add", "channel.channel_points_automatic_reward_redemption.add"] {
        for login in [Value::Null, json!(""), json!("bob")] {
            let (engine, _dir) = engine(RaidShoutoutMode::None);
            let mut event = user_event();
            event["user_login"] = login;
            deliver(&engine, topic, &event).await;
            assert!(engine.shoutout.status().queue.is_empty(), "{topic}");
        }
    }
    for topic in ["channel.subscription.gift", "channel.cheer"] {
        let (engine, _dir) = engine(RaidShoutoutMode::None);
        let mut event = user_event();
        event["is_anonymous"] = json!(true);
        deliver(&engine, topic, &event).await;
        assert!(engine.shoutout.status().queue.is_empty(), "{topic}");
    }
    for field in ["chatter_is_anonymous", "chatter_user_login"] {
        let (engine, _dir) = engine(RaidShoutoutMode::None);
        let mut event = notice("watch_streak", "");
        event[field] = if field == "chatter_is_anonymous" { json!(true) } else { json!("bob") };
        deliver(&engine, "channel.chat.notification", &event).await;
        assert!(engine.shoutout.status().queue.is_empty());
    }
}

#[tokio::test]
async fn raid_notifications_cannot_bypass_any_of_the_four_raid_modes() {
    for listed in [false, true] {
        for mode in [RaidShoutoutMode::None, RaidShoutoutMode::Listed, RaidShoutoutMode::Unlisted, RaidShoutoutMode::All] {
            let expected = match mode { RaidShoutoutMode::None => false, RaidShoutoutMode::Listed => listed, RaidShoutoutMode::Unlisted => !listed, RaidShoutoutMode::All => true };
            for notice_first in [false, true] {
                let (engine, _dir) = engine(mode);
                if !listed { engine.config.write().shoutout.auto_list.clear(); }
                let raid = json!({ "from_broadcaster_user_id": "2", "from_broadcaster_user_name": "Alice", "viewers": 10 });
                // Даже непустой text в уведомлении о рейде не включает обычный авто-шатаут.
                if notice_first { deliver(&engine, "channel.chat.notification", &notice("raid", "Рейд!")).await; }
                deliver(&engine, "channel.raid", &raid).await;
                for kind in ["raid", "unraid", "shared_chat_raid", "shared_chat_unraid"] {
                    deliver(&engine, "channel.chat.notification", &notice(kind, "Рейд!")).await;
                }
                let status = engine.shoutout.status();
                assert_eq!(status.queue.len(), usize::from(expected), "listed={listed}, mode={mode:?}, notice_first={notice_first}");
                if expected { assert_eq!(status.queue[0].source, ShoutoutSource::Raid); }
            }
        }
    }
}

#[tokio::test]
async fn automatic_actions_keep_the_existing_relationship_with_raids() {
    let (engine, _dir) = engine(RaidShoutoutMode::All);
    let raid = json!({ "from_broadcaster_user_id": "2", "from_broadcaster_user_name": "Alice", "viewers": 10 });
    deliver(&engine, "channel.follow", &user_event()).await;
    finish(&engine, SendOutcome::Ok);
    deliver(&engine, "channel.raid", &raid).await;
    assert_eq!(engine.shoutout.status().queue[0].source, ShoutoutSource::Raid);
    finish(&engine, SendOutcome::Ok);
    engine.shoutout.reset_done();
    deliver(&engine, "channel.raid", &raid).await;
    deliver(&engine, "channel.follow", &user_event()).await;
    assert_eq!(engine.shoutout.status().queue.len(), 1, "рейд уже в очереди");
    finish(&engine, SendOutcome::Ok);
    deliver(&engine, "channel.follow", &user_event()).await;
    assert!(engine.shoutout.status().queue.is_empty(), "рейд уже выполнен");
}

#[tokio::test]
async fn already_done_and_reset_apply_to_all_automatic_actions() {
    let (engine, _dir) = engine(RaidShoutoutMode::None);
    deliver(&engine, "channel.follow", &user_event()).await;
    finish(&engine, SendOutcome::AlreadyDone);
    deliver(&engine, "channel.subscribe", &user_event()).await;
    assert!(engine.shoutout.status().queue.is_empty());
    assert_eq!(engine.shoutout.status().done[0].sources, [ShoutoutSource::Activity]);
    engine.shoutout.reset_done();
    deliver(&engine, "channel.subscribe", &user_event()).await;
    assert_eq!(engine.shoutout.status().queue.len(), 1);
}
