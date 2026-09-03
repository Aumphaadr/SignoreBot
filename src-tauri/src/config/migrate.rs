//! Миграция конфигов.
//!
//! v1 — `config.json` старой версии (без поля `version`):
//! команды/награды/периодика — объекты с ключами, `overlay` — объект
//! `{id,path}` или строка, токены в открытом виде. Токены **не переносятся**
//! (старая версия — Confidential-клиент Twitch, его refresh без секрета
//! невозможен); переносятся только логины аккаунтов.

use super::schema::*;
use serde_json::Value;

#[derive(Debug, Default, Clone, serde::Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "config.ts")]
pub struct MigrationReport {
    pub from_version: u32,
    pub notes: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum MigrateError {
    #[error("конфиг не является JSON-объектом")]
    NotObject,
    #[error("неизвестная версия конфига: {0}")]
    UnknownVersion(u64),
    #[error("ошибка разбора: {0}")]
    Parse(#[from] serde_json::Error),
}

/// Разобрать документ любой известной версии и привести к текущей схеме.
pub fn parse_any(doc: &Value) -> Result<(Config, MigrationReport), MigrateError> {
    let obj = doc.as_object().ok_or(MigrateError::NotObject)?;
    // Версия может быть числом или числовой строкой; v2 узнаётся и по форме
    // (команды — массив), чтобы не прогнать v2 через мигратор v1 и не потерять данные.
    let version = match obj.get("version") {
        Some(Value::Number(n)) => n.as_u64().unwrap_or(1),
        Some(Value::String(s)) => s.trim().parse::<u64>().unwrap_or(1),
        _ if obj.get("commands").map(|c| c.is_array()).unwrap_or(false) => 2,
        _ => 1,
    };
    match version {
        1 => Ok(migrate_v1(doc)),
        2 => {
            let mut doc = doc.clone();
            doc["version"] = Value::from(2u32);
            let mut cfg: Config = serde_json::from_value(doc)?;
            cfg.normalize();
            Ok((cfg, MigrationReport { from_version: 2, notes: vec![] }))
        }
        other => Err(MigrateError::UnknownVersion(other)),
    }
}

fn s(v: Option<&Value>) -> String {
    v.and_then(|x| x.as_str()).unwrap_or("").to_string()
}
fn b(v: Option<&Value>, default: bool) -> bool {
    v.and_then(|x| x.as_bool()).unwrap_or(default)
}
fn f(v: Option<&Value>, default: f64) -> f64 {
    v.and_then(|x| x.as_f64()).unwrap_or(default)
}

fn migrate_response(v: Option<&Value>, notes: &mut Vec<String>, ctx: &str) -> Response {
    let mut r = Response::default();
    let Some(v) = v else { return r };
    if let Some(chat) = v.get("chat") {
        r.chat.enabled = b(chat.get("enabled"), false);
        if let Some(arr) = chat.get("components").and_then(|c| c.as_array()) {
            for c in arr {
                match serde_json::from_value::<Component>(c.clone()) {
                    Ok(comp) => r.chat.components.push(comp),
                    Err(e) => notes.push(format!("{ctx}: пропущен компонент {c}: {e}")),
                }
            }
        }
    }
    if let Some(m) = v.get("media") {
        let md = &mut r.media;
        md.enabled = b(m.get("enabled"), false);
        md.file = s(m.get("file"));
        md.secondary_file = s(m.get("secondaryFile"));
        md.volume = f(m.get("volume"), 100.0).clamp(0.0, 100.0) as u8;
        md.overlay = match m.get("overlay") {
            Some(Value::String(id)) if !id.is_empty() => Some(id.clone()),
            Some(Value::Object(o)) => o.get("id").and_then(|x| x.as_str()).map(String::from),
            _ => None,
        };
        md.queue_mode = match m.get("queueMode").and_then(|x| x.as_str()) {
            Some("immediate") => QueueMode::Immediate,
            _ => QueueMode::Queue,
        };
        let ck = s(m.get("chromakey"));
        md.chromakey = if ck.is_empty() { "none".into() } else { ck };
        if let Some(a) = m.get("animation") {
            let enter = s(a.get("enter"));
            let exit = s(a.get("exit"));
            md.animation.enter = if enter.is_empty() { "none".into() } else { enter };
            md.animation.exit = if exit.is_empty() { "none".into() } else { exit };
            md.animation.enter_duration = f(a.get("enterDuration"), 0.5) as f32;
            md.animation.exit_duration = f(a.get("exitDuration"), 0.5) as f32;
        }
        if let Some(t) = m.get("text") {
            md.text.enabled = b(t.get("enabled"), false);
            md.text.content = s(t.get("content"));
            let pos = s(t.get("position"));
            md.text.position = if pos.is_empty() { "overlay".into() } else { pos };
            let anim = s(t.get("animation"));
            md.text.animation = if anim.is_empty() { "none".into() } else { anim };
            md.text.animation_amplitude = f(t.get("animationAmplitude"), 1.0) as f32;
            if let Some(font) = t.get("font").and_then(|x| x.as_object()) {
                md.text.font.font_family = font.get("fontFamily").and_then(|x| x.as_str()).map(String::from);
                md.text.font.font_size = font.get("fontSize").and_then(|x| x.as_u64()).map(|x| x as u32);
                md.text.font.font_weight = font.get("fontWeight").and_then(|x| x.as_str()).map(String::from);
                md.text.font.font_style = font.get("fontStyle").and_then(|x| x.as_str()).map(String::from);
                md.text.font.color = font.get("color").and_then(|x| x.as_str()).map(String::from);
            }
        }
    }
    r
}

pub fn migrate_v1(doc: &Value) -> (Config, MigrationReport) {
    let mut cfg = Config::default();
    let mut notes = Vec::new();
    let obj = doc.as_object().cloned().unwrap_or_default();

    // --- аккаунты (только логины) ---
    if let Some(t) = obj.get("tokens") {
        let ch = s(t.get("channelName"));
        let bot = s(t.get("botUsername"));
        if !ch.is_empty() {
            cfg.accounts.broadcaster = Some(AccountInfo { login: ch.clone(), display_name: ch, ..Default::default() });
        }
        if !bot.is_empty() {
            cfg.accounts.bot = Some(AccountInfo { login: bot.clone(), display_name: bot, ..Default::default() });
        }
        if t.get("accessToken").is_some() || t.get("broadcasterAccessToken").is_some() {
            notes.push("Токены Twitch из старого конфига не переносятся: авторизуйте аккаунты заново.".into());
        }
    }

    // --- команды ---
    if let Some(cmds) = obj.get("commands").and_then(|c| c.as_object()) {
        for (key, v) in cmds {
            let name_raw = s(v.get("name"));
            let name = if name_raw.is_empty() { key.trim_start_matches('!').to_string() } else { name_raw };
            let mut c = Command { name: name.to_lowercase(), ..Default::default() };
            c.enabled = b(v.get("enabled"), true);
            c.aliases = v
                .get("aliases")
                .and_then(|a| a.as_array())
                .map(|a| a.iter().filter_map(|x| x.as_str()).map(|x| x.trim_start_matches('!').to_lowercase()).collect())
                .unwrap_or_default();
            c.permissions = v
                .get("permissions")
                .and_then(|a| a.as_array())
                .map(|a| a.iter().filter_map(|x| x.as_str()).map(String::from).collect())
                .unwrap_or_default();
            c.response = migrate_response(v.get("response"), &mut notes, &format!("команда {key}"));
            cfg.commands.push(c);
        }
    }

    // --- награды ---
    if let Some(rw) = obj.get("rewards").and_then(|c| c.as_object()) {
        for (key, v) in rw {
            let mut r = Reward { id: key.clone(), ..Default::default() };
            r.enabled = b(v.get("enabled"), true);
            r.reward_id = s(v.get("rewardId"));
            r.reward_title = s(v.get("rewardTitle"));
            r.response = migrate_response(v.get("response"), &mut notes, &format!("награда {key}"));
            cfg.rewards.push(r);
        }
    }

    // --- события ---
    if let Some(ev) = obj.get("events").and_then(|c| c.as_object()) {
        for (key, v) in ev {
            let e = EventReaction {
                enabled: b(v.get("enabled"), false),
                skip_gifted: false,
                response: migrate_response(v.get("response"), &mut notes, &format!("событие {key}")),
            };
            cfg.events.insert(key.clone(), e);
        }
    }

    // --- периодика ---
    if let Some(pe) = obj.get("periodicEvents").and_then(|c| c.as_object()) {
        for (key, v) in pe {
            let mut p = PeriodicEvent { name: key.clone(), ..Default::default() };
            p.enabled = b(v.get("enabled"), true);
            p.interval_sec = f(v.get("interval"), 300.0).max(10.0) as u32;
            p.offset_sec = f(v.get("offset"), 0.0).max(0.0) as u32;
            p.color = s(v.get("color"));
            p.response = migrate_response(v.get("response"), &mut notes, &format!("периодическое {key}"));
            cfg.periodic_events.push(p);
        }
        if !cfg.periodic_events.is_empty() {
            notes.push("Периодические события больше не срабатывают немедленно при запуске (см. флаг «сработать при старте»).".into());
        }
    }

    // --- shoutout ---
    if let Some(list) = obj.get("autoshoutout").and_then(|a| a.as_array()) {
        cfg.shoutout.auto_list = list.iter().filter_map(|x| x.as_str()).map(|x| x.to_lowercase()).collect();
    }
    if let Some(ss) = obj.get("shoutoutSettings") {
        cfg.shoutout.raid_mode = match ss.get("raidMode").and_then(|x| x.as_str()) {
            Some("listed") => RaidShoutoutMode::Listed,
            Some("unlisted") => RaidShoutoutMode::Unlisted,
            Some("all") => RaidShoutoutMode::All,
            _ => RaidShoutoutMode::None,
        };
    }

    // --- банворды ---
    if let Some(words) = obj.get("banwords").and_then(|x| x.get("words")).and_then(|a| a.as_array()) {
        for w in words {
            let word = s(w.get("word")).to_lowercase();
            if word.is_empty() {
                continue;
            }
            let kind = if s(w.get("type")) == "soft" { BanWordKind::Soft } else { BanWordKind::Hard };
            let aliases = w
                .get("aliases")
                .and_then(|a| a.as_array())
                .map(|a| a.iter().filter_map(|x| x.as_str()).map(String::from).collect())
                .unwrap_or_default();
            cfg.banwords.words.push(BanWord { word, kind, aliases });
        }
    }

    // --- оверлеи ---
    if let Some(ovs) = obj.get("overlays").and_then(|a| a.as_array()) {
        for o in ovs {
            let mut ov = Overlay::default();
            let id = s(o.get("id"));
            if !id.is_empty() {
                ov.id = id;
            }
            ov.name = s(o.get("name"));
            ov.path = s(o.get("path"));
            cfg.overlays.push(ov);
        }
    }

    // --- OBS ---
    if let Some(o) = obj.get("obs") {
        cfg.obs.enabled = b(o.get("enabled"), false);
        let url = s(o.get("url"));
        if !url.is_empty() {
            cfg.obs.url = url;
        }
        cfg.obs.password = s(o.get("password"));
        cfg.obs.auto_refresh = b(o.get("autoRefresh"), true);
        if let Some(bs) = o.get("browserSources").and_then(|a| a.as_array()) {
            for x in bs {
                cfg.obs.browser_sources.push(ObsBrowserSource {
                    overlay_path: s(x.get("overlayPath")),
                    input_name: s(x.get("inputName")),
                });
            }
        }
    }

    // --- заметки ---
    if let Some(notes_arr) = obj.get("notes").and_then(|a| a.as_array()) {
        for n in notes_arr {
            let mut note = Note::default();
            let id = s(n.get("id"));
            if !id.is_empty() {
                note.id = id;
            }
            note.text = s(n.get("text"));
            note.status = match s(n.get("status")).as_str() {
                "done" => NoteStatus::Done,
                "cancelled" => NoteStatus::Cancelled,
                _ => NoteStatus::Active,
            };
            let ca = s(n.get("createdAt"));
            if !ca.is_empty() {
                note.created_at = ca;
            }
            let ua = s(n.get("updatedAt"));
            if !ua.is_empty() {
                note.updated_at = ua;
            }
            cfg.notes.push(note);
        }
    }

    // Ссылки на оверлеи по id, которых больше нет — сбрасываем на «все».
    let ids: Vec<String> = cfg.overlays.iter().map(|o| o.id.clone()).collect();
    let mut fix = |r: &mut Response, ctx: String| {
        if let Some(id) = &r.media.overlay {
            if !ids.contains(id) {
                notes.push(format!("{ctx}: оверлей «{id}» не найден, медиа будет идти на все оверлеи."));
                r.media.overlay = None;
            }
        }
    };
    for c in &mut cfg.commands {
        fix(&mut c.response, format!("команда !{}", c.name));
    }
    for r in &mut cfg.rewards {
        fix(&mut r.response, format!("награда «{}»", r.reward_title));
    }
    for (k, e) in &mut cfg.events {
        fix(&mut e.response, format!("событие {k}"));
    }
    for p in &mut cfg.periodic_events {
        fix(&mut p.response, format!("периодическое «{}»", p.name));
    }

    cfg.normalize();
    (cfg, MigrationReport { from_version: 1, notes })
}

#[cfg(test)]
mod tests {
    use super::*;

    const OLD: &str = r##"{
      "tokens": {"channelName":"streamer","botUsername":"bot","accessToken":"x","refreshToken":"y"},
      "commands": {
        "!кусь": {"enabled":true,"name":"кусь","permissions":[],"aliases":["!bite"],
          "response":{"chat":{"enabled":true,"components":[{"type":"author"},{"type":"static","value":" покусал "},{"type":"target"}]},
                      "media":{"enabled":false,"file":"","volume":100,"overlay":null,"text":{"enabled":false,"content":"","position":"overlay"}}}},
        "!звук": {"enabled":true,"name":"звук","permissions":["moderators"],"aliases":[],
          "response":{"chat":{"enabled":false,"components":[]},
                      "media":{"enabled":true,"file":"a.mp3","volume":80,"overlay":{"id":"overlay_1","path":"audio"},"queueMode":"immediate",
                               "text":{"enabled":true,"content":"{user}","position":"below","animation":"wave","font":{"fontFamily":"'Fira Code', sans-serif"}},
                               "animation":{"enter":"fadeInTop","exit":"fadeOutTop"},"secondaryFile":"b.png"}}},
        "!legacy": {"enabled":true,"name":"legacy","response":{"media":{"enabled":true,"file":"c.mp4","overlay":"overlay_missing"}}}
      },
      "banwords": {"words":[{"word":"Bad","type":"hard","aliases":["bad","6ad"]}]},
      "periodicEvents": {"e1":{"enabled":true,"interval":1800,"offset":780,"color":"#fff","response":{"chat":{"enabled":true,"components":[{"type":"static","value":"123"}]}}}},
      "overlays": [{"id":"overlay_1","name":"Аудио","path":"audio"}],
      "rewards": {"reward_x":{"enabled":true,"rewardId":"uuid","rewardTitle":"T","response":{"media":{"enabled":true,"file":"r.mp3","overlay":{"id":"overlay_1","path":"audio"}}}}},
      "events": {"follow":{"enabled":true,"response":{"chat":{"enabled":true,"components":[{"type":"static","value":"hi {user}"}]}}}},
      "autoshoutout": ["Alice","bob"],
      "shoutoutSettings": {"raidMode":"listed"},
      "obs": {"enabled":true,"url":"ws://127.0.0.1:4455","password":"","autoRefresh":true,"browserSources":[{"overlayPath":"audio","inputName":"Overlay Audio"}]},
      "notes": [{"id":"note_1","text":"todo","status":"done","createdAt":"2026-01-01T00:00:00Z","updatedAt":"2026-01-02T00:00:00Z"}]
    }"##;

    #[test]
    fn migrates_v1() {
        let doc: Value = serde_json::from_str(OLD).unwrap();
        let (cfg, report) = parse_any(&doc).unwrap();
        assert_eq!(report.from_version, 1);
        assert_eq!(cfg.version, 2);
        assert_eq!(cfg.accounts.broadcaster.as_ref().unwrap().login, "streamer");
        assert_eq!(cfg.commands.len(), 3);
        let bite = cfg.commands.iter().find(|c| c.name == "кусь").unwrap();
        assert_eq!(bite.aliases, vec!["bite"]);
        assert_eq!(bite.response.chat.components.len(), 3);
        let snd = cfg.commands.iter().find(|c| c.name == "звук").unwrap();
        assert_eq!(snd.response.media.overlay.as_deref(), Some("overlay_1"));
        assert_eq!(snd.response.media.queue_mode, QueueMode::Immediate);
        assert_eq!(snd.response.media.secondary_file, "b.png");
        assert_eq!(snd.response.media.volume, 80);
        assert_eq!(snd.response.media.text.font.font_family.as_deref(), Some("'Fira Code', sans-serif"));
        assert_eq!(snd.response.media.animation.enter, "fadeInTop");
        let legacy = cfg.commands.iter().find(|c| c.name == "legacy").unwrap();
        assert_eq!(legacy.response.media.overlay, None);
        assert!(report.notes.iter().any(|n| n.contains("overlay_missing")));
        assert_eq!(cfg.banwords.words[0].word, "bad");
        assert_eq!(cfg.periodic_events[0].name, "e1");
        assert_eq!(cfg.periodic_events[0].offset_sec, 780);
        assert_eq!(cfg.rewards[0].id, "reward_x");
        assert!(cfg.events["follow"].enabled);
        assert_eq!(cfg.shoutout.auto_list, vec!["alice", "bob"]);
        assert_eq!(cfg.shoutout.raid_mode, RaidShoutoutMode::Listed);
        assert_eq!(cfg.obs.browser_sources[0].input_name, "Overlay Audio");
        assert_eq!(cfg.notes[0].status, NoteStatus::Done);
        assert!(!cfg.network.overlay_key.is_empty());
        // и обратно читается как v2 без потерь
        let json = serde_json::to_value(&cfg).unwrap();
        let (again, r2) = parse_any(&json).unwrap();
        assert_eq!(r2.from_version, 2);
        assert_eq!(again, cfg);
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse_any(&Value::Null).is_err());
        assert!(matches!(parse_any(&serde_json::json!({"version": 99})), Err(MigrateError::UnknownVersion(99))));
    }
}
