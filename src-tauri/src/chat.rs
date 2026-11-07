//! Session-local chat history. EventSub is the source; rendering never affects the bot engine.
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet, VecDeque};
use tokio::sync::broadcast;
use ts_rs::TS;

const LIMIT: usize = 500;
#[derive(Debug, Clone, Serialize, Deserialize, TS, Default)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "api.ts")]
pub struct ChatFragment {
    pub text: String,
    pub user_id: Option<String>,
    pub image_url: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS, Default)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "api.ts")]
pub struct ChatBadge {
    pub set_id: String,
    pub id: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS, Default)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "api.ts")]
pub struct ChatReply {
    pub message_id: String,
    pub user_id: String,
    pub name: String,
    pub text: String,
    pub deleted: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS, Default)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "api.ts")]
pub struct ChatEntry {
    pub id: String,
    pub message_id: String,
    #[ts(type = "number")]
    pub timestamp: i64,
    pub user_id: String,
    pub login: String,
    pub name: String,
    pub color: String,
    pub text: String,
    pub fragments: Vec<ChatFragment>,
    pub badges: Vec<ChatBadge>,
    pub reply: Option<ChatReply>,
    // Empty for ordinary messages; localized event heading otherwise.
    pub event: String,
    pub reward_id: String,
    pub is_reward: bool,
    pub deleted: bool,
    pub read: bool,
    pub sent_by_bot: bool,
    #[serde(skip)]
    #[ts(skip)]
    pub notice: bool,
}
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "api.ts")]
pub struct ChatHistory {
    pub channel_id: String,
    pub generation: u32,
    pub revision: u32,
    pub entries: Vec<ChatEntry>,
}
#[derive(Default)]
struct History {
    channel_id: String,
    generation: u32,
    revision: u32,
    entries: VecDeque<ChatEntry>,
    removed: VecDeque<String>,
    sent: VecDeque<String>,
    cleared_users: HashMap<String, i64>,
    cleared_at: i64,
    hype_levels: HashMap<String, u64>,
    rewards: HashMap<String, (String, u64)>,
}
pub struct ChatHub {
    state: Mutex<History>,
    changed: broadcast::Sender<()>,
}
impl Default for ChatHub {
    fn default() -> Self {
        let (changed, _) = broadcast::channel(16);
        Self {
            state: Mutex::new(History::default()),
            changed,
        }
    }
}
impl ChatHub {
    pub fn subscribe(&self) -> broadcast::Receiver<()> {
        self.changed.subscribe()
    }
    fn changed(&self, h: &mut History) {
        h.revision = h.revision.wrapping_add(1);
        let _ = self.changed.send(());
    }
    pub fn set_channel(&self, channel_id: &str) {
        let mut h = self.state.lock();
        if h.channel_id != channel_id {
            let generation = h.generation.wrapping_add(1);
            *h = History {
                channel_id: channel_id.into(),
                generation,
                ..Default::default()
            };
            self.changed(&mut h);
        }
    }
    pub fn snapshot(&self) -> ChatHistory {
        let h = self.state.lock();
        ChatHistory {
            channel_id: h.channel_id.clone(),
            generation: h.generation,
            revision: h.revision,
            entries: h.entries.iter().cloned().collect(),
        }
    }
    pub fn entry(&self, id: &str) -> Option<ChatEntry> {
        self.state
            .lock()
            .entries
            .iter()
            .find(|e| e.id == id)
            .cloned()
    }
    pub fn mark(&self, generation: u32, ids: &[String], read: bool) {
        let ids: HashSet<_> = ids.iter().collect();
        let mut h = self.state.lock();
        if h.generation != generation {
            return;
        }
        for e in &mut h.entries {
            if ids.contains(&e.id) {
                e.read = read;
            }
        }
        self.changed(&mut h);
    }
    pub fn note_sent(&self, id: &str) {
        if id.is_empty() {
            return;
        }
        let mut h = self.state.lock();
        if h.sent.len() >= LIMIT {
            h.sent.pop_front();
        }
        h.sent.push_back(id.into());
        for e in &mut h.entries {
            if e.message_id == id {
                e.sent_by_bot = true;
            }
        }
        self.changed(&mut h);
    }
    pub fn delete(&self, id: &str) {
        let mut h = self.state.lock();
        remove_message(&mut h, id);
        self.changed(&mut h);
    }
    pub fn ingest(&self, channel: &str, kind: &str, envelope_id: &str, timestamp: &str, v: &Value) {
        let ts = chrono::DateTime::parse_from_rfc3339(timestamp)
            .map(|d| d.timestamp_millis())
            .unwrap_or_else(|_| chrono::Utc::now().timestamp_millis());
        let mut h = self.state.lock();
        if h.channel_id != channel {
            return;
        } // late notification from a canceled session
        let reward = if kind == "channel.channel_points_custom_reward_redemption.add" {
            Some(&v["reward"])
        } else if [
            "channel.channel_points_custom_reward.add",
            "channel.channel_points_custom_reward.update",
        ]
        .contains(&kind)
        {
            Some(v)
        } else {
            None
        };
        if let Some(reward) = reward {
            let id = s(reward, "id");
            let title = s(reward, "title");
            let cost = n(reward, "cost");
            if !id.is_empty() && !title.is_empty() {
                h.rewards.insert(id.clone(), (title.clone(), cost));
                let mut updated = false;
                for e in &mut h.entries {
                    if e.reward_id == id {
                        e.event = format!("Награда · {title} · {cost} баллов");
                        updated = true;
                    }
                }
                if updated {
                    self.changed(&mut h);
                }
            }
        }
        match kind {
            "channel.chat.message_delete" => {
                remove_message(&mut h, &s(v, "message_id"));
                self.changed(&mut h);
                return;
            }
            "channel.chat.clear_user_messages" | "channel.chat.clear" => {
                let user = s(v, "target_user_id");
                if user.is_empty() {
                    h.cleared_at = h.cleared_at.max(ts);
                } else {
                    h.cleared_users
                        .entry(user.clone())
                        .and_modify(|t| *t = (*t).max(ts))
                        .or_insert(ts);
                }
                let ids: Vec<_> = h
                    .entries
                    .iter()
                    .filter(|e| (user.is_empty() || e.user_id == user) && e.timestamp <= ts)
                    .map(|e| e.message_id.clone())
                    .filter(|id| !id.is_empty())
                    .collect();
                for id in ids {
                    remove_message(&mut h, &id);
                }
                for entry in &mut h.entries {
                    if entry.timestamp <= ts {
                        if let Some(reply) = &mut entry.reply {
                            if user.is_empty() || reply.user_id == user {
                                reply.deleted = true;
                                reply.text.clear();
                            }
                        }
                    }
                }
                self.changed(&mut h);
                return;
            }
            _ => {}
        }
        let Some(mut e) = parse(kind, envelope_id, ts, v, &mut h.hype_levels) else {
            return;
        };
        if let Some((title, cost)) = h.rewards.get(&e.reward_id) {
            e.event = format!("Награда · {title} · {cost} баллов");
        }
        if !e.message_id.is_empty()
            && (h.removed.contains(&e.message_id)
                || ts <= h.cleared_at
                || ts <= *h.cleared_users.get(&e.user_id).unwrap_or(&0))
        {
            redact(&mut e);
        }
        if let Some(reply) = &mut e.reply {
            let parent = h.entries.iter().find(|e| e.message_id == reply.message_id);
            if h.removed.contains(&reply.message_id)
                || parent.is_some_and(|e| e.deleted)
                || (parent.is_none()
                    && (h.cleared_at > 0 || h.cleared_users.contains_key(&reply.user_id)))
            {
                reply.deleted = true;
                reply.text.clear();
            }
        }
        e.sent_by_bot = h.sent.contains(&e.message_id);
        if let Some(old) = h.entries.iter_mut().find(|x| x.id == e.id) {
            // Notification enriches the same message, regardless of delivery order.
            if !old.notice && e.notice {
                e.read = old.read;
                e.timestamp = old.timestamp;
                e.sent_by_bot |= old.sent_by_bot;
                if old.deleted {
                    redact(&mut e);
                }
                *old = e;
                self.changed(&mut h);
            }
            return;
        }
        let pos = h
            .entries
            .iter()
            .position(|x| x.timestamp > ts)
            .unwrap_or(h.entries.len());
        h.entries.insert(pos, e);
        while h.entries.len() > LIMIT {
            h.entries.pop_front();
        }
        // User clears only need to outlive the bounded history and EventSub replay window.
        h.cleared_users.retain(|_, t| *t > ts - 3_600_000);
        self.changed(&mut h);
    }
}
fn redact(e: &mut ChatEntry) {
    e.deleted = true;
    e.text.clear();
    e.fragments.clear();
}
fn remove_message(h: &mut History, id: &str) {
    if id.is_empty() {
        return;
    }
    if !h.removed.iter().any(|x| x == id) {
        if h.removed.len() >= 2000 {
            h.removed.pop_front();
        }
        h.removed.push_back(id.into());
    }
    for e in &mut h.entries {
        if e.message_id == id {
            redact(e);
        }
        if let Some(r) = &mut e.reply {
            if r.message_id == id {
                r.deleted = true;
                r.text.clear();
            }
        }
    }
}
fn s(v: &Value, key: &str) -> String {
    v.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .into()
}
fn n(v: &Value, key: &str) -> u64 {
    v.get(key).and_then(Value::as_u64).unwrap_or(0)
}
fn fragments(v: &Value) -> Vec<ChatFragment> {
    v.as_array()
        .into_iter()
        .flatten()
        .map(|f| {
            let mut out = ChatFragment {
                text: s(f, "text"),
                ..Default::default()
            };
            if let Some(m) = f.get("mention") {
                let id = s(m, "user_id");
                if !id.is_empty() {
                    out.user_id = Some(id);
                }
            }
            if let Some(emote) = f.get("emote") {
                let id = s(emote, "id");
                if !id.is_empty() {
                    let id: String = url::form_urlencoded::byte_serialize(id.as_bytes()).collect();
                    let format = if emote["format"]
                        .as_array()
                        .is_some_and(|a| a.iter().any(|v| v == "animated"))
                    {
                        "animated"
                    } else {
                        "static"
                    };
                    out.image_url = Some(format!(
                        "https://static-cdn.jtvnw.net/emoticons/v2/{id}/{format}/dark/2.0"
                    ));
                }
            }
            out
        })
        .collect()
}
fn parse(
    kind: &str,
    envelope_id: &str,
    timestamp: i64,
    v: &Value,
    hype: &mut HashMap<String, u64>,
) -> Option<ChatEntry> {
    let mut e = ChatEntry {
        id: format!("event:{envelope_id}"),
        timestamp,
        user_id: s(v, "user_id"),
        login: s(v, "user_login"),
        name: s(v, "user_name"),
        ..Default::default()
    };
    match kind {
        "channel.chat.message" | "channel.chat.notification" => {
            e.message_id = s(v, "message_id");
            if e.message_id.is_empty() {
                return None;
            }
            e.id = format!("chat:{}", e.message_id);
            e.user_id = s(v, "chatter_user_id");
            e.login = s(v, "chatter_user_login");
            e.name = s(v, "chatter_user_name");
            e.color = s(v, "color");
            e.text = s(&v["message"], "text");
            e.fragments = fragments(&v["message"]["fragments"]);
            e.badges = v["badges"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|b| ChatBadge {
                    set_id: s(b, "set_id"),
                    id: s(b, "id"),
                })
                .collect();
            if let Some(r) = v.get("reply").filter(|r| !r.is_null()) {
                e.reply = Some(ChatReply {
                    message_id: s(r, "parent_message_id"),
                    user_id: s(r, "parent_user_id"),
                    name: s(r, "parent_user_name"),
                    text: s(r, "parent_message_body"),
                    deleted: false,
                });
            }
            if kind == "channel.chat.notification" {
                e.notice = true;
                let typ = s(v, "notice_type");
                let data = &v[&typ];
                e.event = match typ.as_str() {
                    "sub" | "shared_chat_sub" => "Новая подписка".into(),
                    "resub" | "shared_chat_resub" => {
                        format!("Подписка · {} мес.", n(data, "cumulative_months"))
                    }
                    "sub_gift" | "shared_chat_sub_gift" => {
                        format!("Подаренная подписка → {}", s(data, "recipient_user_name"))
                    }
                    "community_sub_gift" | "shared_chat_community_sub_gift" => {
                        format!("Подарено подписок: {}", n(data, "total"))
                    }
                    "raid" | "shared_chat_raid" => {
                        format!("Рейд · {} зрителей", n(data, "viewer_count"))
                    }
                    "unraid" => "Рейд отменён".into(),
                    "view_milestone" if s(data, "category") == "watch_streak" => {
                        format!("Серия просмотров · {} трансляций подряд", n(data, "value"))
                    }
                    "announcement" => "Объявление".into(),
                    "bits_badge_tier" => format!("Новый значок Bits · {}", n(data, "tier")),
                    "gift_paid_upgrade" | "prime_paid_upgrade" | "pay_it_forward" => {
                        "Продолжение подписки".into()
                    }
                    _ => s(v, "system_message"),
                };
                if e.event.is_empty() {
                    e.event = "Событие Twitch".into();
                }
            } else if !s(v, "channel_points_custom_reward_id").is_empty() {
                e.is_reward = true;
                e.reward_id = s(v, "channel_points_custom_reward_id");
                e.event = "Награда за баллы канала".into();
            } else if s(v, "message_type") == "channel_points_highlighted" {
                e.is_reward = true; e.reward_id = "twitch:send_highlighted_message".into(); e.event = "Награда · Выделить сообщение".into();
            } else if !v["cheer"].is_null() {
                e.event = format!("Bits · {}", n(&v["cheer"], "bits"));
            }
        }
        "channel.follow" => e.event = "Новый фолловер".into(),
        "channel.cheer" => {
            // Identified cheers use chat.message (fragments and deletable message ID).
            if v["is_anonymous"] != true {
                return None;
            }
            e.event = format!("Bits · {}", n(v, "bits"));
            e.text = s(v, "message");
            if v["is_anonymous"] == true {
                e.name = "Аноним".into();
                e.user_id.clear();
                e.login.clear();
            }
        }
        "channel.channel_points_custom_reward_redemption.add" => {
            // Text-bearing redemptions arrive in chat.message with the actual message ID.
            if !s(v, "user_input").is_empty() {
                return None;
            }
            e.is_reward = true;
            e.reward_id = s(&v["reward"], "id");
            e.id = format!("redemption:{}", s(v, "id"));
            e.event = format!(
                "Награда · {} · {} баллов",
                s(&v["reward"], "title"),
                n(&v["reward"], "cost")
            );
        }
        "channel.channel_points_automatic_reward_redemption.add" => {
            // Message effects and highlighted messages already arrive as chat.message.
            let typ = s(&v["reward"], "type");
            if [
                "send_highlighted_message",
                "random_sub_emote_unlock",
                "chosen_sub_emote_unlock",
                "send_gigantified_emote",
                "send_message_effect",
                "gigantify_an_emote",
                "message_effect",
            ]
            .contains(&typ.as_str())
            {
                return None;
            }
            e.is_reward = true;
            e.reward_id = format!("twitch:{typ}");
            e.id = format!("redemption:{}", s(v, "id"));
            e.event = format!("Награда Twitch · {typ}");
            e.text = s(&v["message"], "text");
            e.fragments = fragments(&v["message"]["fragments"]);
        }
        "channel.hype_train.begin" | "channel.hype_train.progress" | "channel.hype_train.end" => {
            let id = s(v, "id");
            let level = n(v, "level");
            let old = hype.insert(id.clone(), level);
            e.event = if kind.ends_with(".end") {
                hype.remove(&id);
                format!("Хайповоз завершён · уровень {level}")
            } else if kind.ends_with(".begin") {
                format!("Хайповоз начался · уровень {level}")
            } else if old.map(|old| level > old).unwrap_or(true) {
                format!("Хайповоз · уровень {level}")
            } else {
                return None;
            };
        }
        _ => return None,
    }
    if e.name.is_empty() {
        e.name = e.login.clone();
    }
    Some(e)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn message(id: &str) -> Value {
        json!({"message_id":id,"chatter_user_id":"u","chatter_user_login":"viewer","chatter_user_name":"Viewer","message":{"text":"@host Kappa","fragments":[{"type":"mention","text":"@host","mention":{"user_id":"host"}},{"type":"text","text":" "},{"type":"emote","text":"Kappa","emote":{"id":"25","format":["static"]}}]}})
    }
    fn put(h: &ChatHub, kind: &str, id: &str, v: Value) {
        h.ingest("host", kind, id, "2026-11-07T10:00:00Z", &v);
    }
    #[test]
    fn point_rewards_keep_identity_for_messages_and_pure_redemptions() {
        let h = ChatHub::default(); h.set_channel("host");
        let mut v = message("text"); v["channel_points_custom_reward_id"] = json!("reward");
        put(&h,"channel.chat.message","a",v);
        put(&h,"channel.channel_points_custom_reward_redemption.add","b",json!({"id":"r","user_id":"viewer","reward":{"id":"reward","title":"Question","cost":100}}));
        put(&h,"channel.channel_points_automatic_reward_redemption.add","c",json!({"id":"auto","reward":{"type":"single_message_bypass_sub_mode"}}));
        let rows=h.snapshot().entries; assert_eq!(rows.len(),3); assert!(rows.iter().all(|e|e.is_reward));
        assert_eq!(rows[0].reward_id,"reward"); assert_eq!(rows[1].reward_id,"reward"); assert_eq!(rows[2].reward_id,"twitch:single_message_bypass_sub_mode");
    }
    #[test]
    fn notification_enriches_message_and_preserves_read_state() {
        let h = ChatHub::default();
        h.set_channel("host");
        put(&h, "channel.chat.message", "a", message("1"));
        h.mark(1, &["chat:1".into()], true);
        let mut v = message("1");
        v["notice_type"] = json!("resub");
        v["resub"] = json!({"cumulative_months":12});
        put(&h, "channel.chat.notification", "b", v.clone());
        put(&h, "channel.chat.message", "c", message("1"));
        let rows = h.snapshot().entries;
        assert_eq!(rows.len(), 1);
        assert!(rows[0].read);
        assert_eq!(rows[0].event, "Подписка · 12 мес.");
        assert_eq!(rows[0].fragments[0].user_id.as_deref(), Some("host"));
        assert!(rows[0].fragments[2]
            .image_url
            .as_ref()
            .unwrap()
            .ends_with("/25/static/dark/2.0"));
    }
    #[test]
    fn removed_messages_and_parent_quotes_cannot_resurrect() {
        let h = ChatHub::default();
        h.set_channel("host");
        h.delete("parent");
        let mut v = message("reply");
        v["reply"] = json!({"parent_message_id":"parent","parent_user_id":"host","parent_user_name":"Host","parent_message_body":"secret","thread_user_id":"someone"});
        put(&h, "channel.chat.message", "r", v);
        put(&h, "channel.chat.message", "p", message("parent"));
        let rows = h.snapshot().entries;
        let reply = rows[0].reply.as_ref().unwrap();
        assert!(reply.deleted);
        assert!(reply.text.is_empty());
        assert_eq!(reply.user_id, "host");
        assert!(rows[1].deleted);
        assert!(rows[1].fragments.is_empty());
    }
    #[test]
    fn channel_and_generation_isolate_history_and_actions() {
        let h = ChatHub::default();
        h.set_channel("host");
        put(&h, "channel.chat.message", "a", message("1"));
        h.set_channel("other");
        put(&h, "channel.chat.message", "late", message("2"));
        assert!(h.snapshot().entries.is_empty());
        h.set_channel("host");
        put(&h, "channel.chat.message", "b", message("1"));
        h.mark(1, &["chat:1".into()], true);
        assert!(!h.snapshot().entries[0].read);
    }
    #[test]
    fn sent_echo_is_visible_and_corrected_even_if_it_arrives_first() {
        let h = ChatHub::default();
        h.set_channel("host");
        put(&h, "channel.chat.message", "a", message("1"));
        h.note_sent("1");
        assert!(h.snapshot().entries[0].sent_by_bot);
        h.note_sent("2");
        put(&h, "channel.chat.message", "b", message("2"));
        assert!(h.snapshot().entries[1].sent_by_bot);
    }
    #[test]
    fn pure_events_and_cheers_have_one_canonical_source() {
        let h = ChatHub::default();
        h.set_channel("host");
        let mut v = message("1");
        v["cheer"] = json!({"bits":100});
        put(&h, "channel.chat.message", "a", v);
        put(
            &h,
            "channel.cheer",
            "b",
            json!({"is_anonymous":true,"bits":100,"message":"Nice"}),
        );
        put(
            &h,
            "channel.subscription.message",
            "c",
            json!({"user_id":"u"}),
        );
        put(
            &h,
            "channel.channel_points_custom_reward_redemption.add",
            "d",
            json!({"id":"r","user_input":"hello"}),
        );
        put(
            &h,
            "channel.channel_points_custom_reward_redemption.add",
            "e",
            json!({"id":"s","reward":{"title":"Hello","cost":50}}),
        );
        assert_eq!(h.snapshot().entries.len(), 3);
        assert_eq!(h.snapshot().entries[1].name, "Аноним");
    }
    #[test]
    fn clear_user_redacts_quotes_but_does_not_remove_future_messages() {
        let h = ChatHub::default();
        h.set_channel("host");
        put(&h, "channel.chat.message", "a", message("1"));
        put(
            &h,
            "channel.chat.clear_user_messages",
            "b",
            json!({"target_user_id":"u"}),
        );
        put(&h, "channel.chat.message", "late", message("2"));
        h.ingest(
            "host",
            "channel.chat.message",
            "future",
            "2026-11-07T10:00:01Z",
            &message("3"),
        );
        let rows = h.snapshot().entries;
        assert!(rows[0].deleted);
        assert!(rows[1].deleted);
        assert!(!rows[2].deleted);
    }
    #[test]
    fn reward_text_and_metadata_join_in_either_order_without_duplicate_rows() {
        for metadata_first in [true, false] {
            let h = ChatHub::default();
            h.set_channel("host");
            let mut v = message("1");
            v["channel_points_custom_reward_id"] = json!("reward");
            let redemption = json!({"id":"redemption","user_input":"@host Kappa","reward":{"id":"reward","title":"Говорить шёпотом","cost":100}});
            if metadata_first {
                put(
                    &h,
                    "channel.channel_points_custom_reward_redemption.add",
                    "r",
                    redemption.clone(),
                );
            }
            put(&h, "channel.chat.message", "a", v);
            if !metadata_first {
                put(
                    &h,
                    "channel.channel_points_custom_reward_redemption.add",
                    "r",
                    redemption,
                );
            }
            let rows = h.snapshot().entries;
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0].event, "Награда · Говорить шёпотом · 100 баллов");
            assert_eq!(rows[0].text, "@host Kappa");
        }
    }
    #[test]
    fn clear_user_redacts_quotes_even_if_the_parent_has_left_history() {
        let h = ChatHub::default();
        h.set_channel("host");
        let mut v = message("reply");
        v["reply"] = json!({"parent_message_id":"old","parent_user_id":"u","parent_user_name":"Viewer","parent_message_body":"secret"});
        put(&h, "channel.chat.message", "a", v.clone());
        put(
            &h,
            "channel.chat.clear_user_messages",
            "clear",
            json!({"target_user_id":"u"}),
        );
        assert!(h.snapshot().entries[0].reply.as_ref().unwrap().deleted);
        v["message_id"] = json!("late");
        h.ingest(
            "host",
            "channel.chat.message",
            "late",
            "2026-11-07T10:00:01Z",
            &v,
        );
        assert!(h.snapshot().entries[1].reply.as_ref().unwrap().deleted);
    }
    #[test]
    fn ring_is_bounded_and_mark_all_does_not_consume_later_arrivals() {
        let h = ChatHub::default();
        h.set_channel("host");
        for i in 0..510 {
            put(
                &h,
                "channel.chat.message",
                &i.to_string(),
                message(&i.to_string()),
            );
        }
        let snap = h.snapshot();
        assert_eq!(snap.entries.len(), 500);
        let ids = snap
            .entries
            .iter()
            .map(|e| e.id.clone())
            .collect::<Vec<_>>();
        put(&h, "channel.chat.message", "new", message("new"));
        h.mark(snap.generation, &ids, true);
        assert!(!h.snapshot().entries.last().unwrap().read);
    }
}
