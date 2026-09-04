//! Ломатели: нетипичные и «по незнанке» сценарии по осям — конфиг/миграция,
//! чат/движок, награды, шатауты, банворды, периодика, оверлеи, медиа.
//! Каждый тест — гипотеза «здесь можно сломать»; падение = улов.

use signorebot_lib::config::store::{load_or_create, parse_document};
use signorebot_lib::config::*;
use signorebot_lib::engine::banwords::{generate_aliases, Matcher};
use signorebot_lib::engine::message::{render, RenderCtx};
use signorebot_lib::engine::periodic::next_fire;
use signorebot_lib::engine::shoutout::{SendOutcome, ShoutoutQueue};
use signorebot_lib::engine::{Engine, RewardSource};
use signorebot_lib::overlay::hub::OverlayHub;
use signorebot_lib::paths::AppPaths;
use signorebot_lib::secrets::Secrets;
use signorebot_lib::twitch::accounts::AuthManager;
use signorebot_lib::twitch::eventsub::{ChatMessage, TwitchEvent};
use signorebot_lib::twitch::helix::Helix;
use std::sync::Arc;
use std::time::{Duration, Instant};

// ============================================================ конфиг / миграция

#[test]
fn v1_with_garbage_shapes_survives() {
    // Типы полей перепутаны, ключи странные — мигратор не должен паниковать.
    let doc = r#"{
      "tokens": null,
      "commands": {
        "!": {"name": ""},
        "!!двойной": {"name": "!!двойной", "aliases": [" !ALIAS ", "", "!"], "permissions": "moderators"},
        "!my cmd": {"name": "my cmd", "response": {"chat": {"enabled": true, "components": [{"type": "unknownType"}, {"type": "static"}]},
                                                    "media": {"enabled": true, "file": "x.mp3", "volume": "громко", "overlay": {}}}},
        "!strnum": {"response": {"media": {"volume": 1000}}}
      },
      "banwords": {"words": "нет"},
      "periodicEvents": {"e": {"interval": "300", "offset": -5}, "f": {"interval": 0}, "g": {"interval": 1e12}},
      "overlays": {"not": "array"},
      "rewards": {"r": {"rewardId": "u1"}, "r2": {"rewardId": "u1"}},
      "events": {"hype": {"enabled": true}},
      "autoshoutout": ["@Alice", " bob ", "", 5],
      "notes": [{"status": "weird"}]
    }"#;
    let (cfg, report) = parse_document(doc).expect("мусорный v1 должен читаться");
    assert_eq!(report.from_version, 1);
    // команда с пустым именем отбрасывается нормализацией? — нет, остаётся пустой: проверим, что такого нет
    assert!(cfg.commands.iter().all(|c| !c.name.is_empty()), "команды с пустым именем должны быть выброшены");
    let d = cfg.commands.iter().find(|c| c.name.contains("двойной")).unwrap();
    assert_eq!(d.name, "двойной", "ведущие ! в имени срезаются");
    assert_eq!(d.aliases, vec!["alias"], "алиасы: trim, lower, без пустых и без «!»");
    assert!(d.permissions.is_empty(), "permissions-строка вместо массива → пусто");
    let m = cfg.commands.iter().find(|c| c.name == "mycmd").expect("пробел в имени схлопывается");
    assert_eq!(m.response.chat.components.len(), 1, "неизвестный тип компонента пропущен, static без value — остался");
    assert_eq!(m.response.media.volume, 100, "нечисловая громкость → 100");
    assert_eq!(m.response.media.overlay, None, "overlay {{}} → все оверлеи");
    assert_eq!(cfg.commands.iter().find(|c| c.name == "strnum").unwrap().response.media.volume, 100, "громкость 1000 → 100");
    assert!(cfg.banwords.words.is_empty());
    let e = cfg.periodic_events.iter().find(|p| p.name == "e").unwrap();
    assert_eq!((e.interval_sec, e.offset_sec), (300, 0), "interval-строка → число, отрицательное смещение → 0");
    assert_eq!(cfg.periodic_events.iter().find(|p| p.name == "f").unwrap().interval_sec, 10);
    assert!(cfg.periodic_events.iter().find(|p| p.name == "g").unwrap().interval_sec >= 10);
    assert!(cfg.overlays.is_empty());
    assert_eq!(cfg.rewards.len(), 1, "две реакции на одну награду → остаётся одна");
    assert!(cfg.events.contains_key("hype"), "неизвестное событие сохраняется (не теряем данные)");
    assert_eq!(cfg.shoutout.auto_list, vec!["alice", "bob"]);
    assert_eq!(cfg.notes[0].status, NoteStatus::Active);
}

#[test]
fn v2_with_string_version_is_not_treated_as_v1() {
    // «version»: "2" (строка) — раньше падало в миграцию v1 и теряло команды-массив.
    let mut cfg = Config::default();
    cfg.normalize();
    cfg.commands.push(Command { name: "a".into(), ..Default::default() });
    let mut doc = serde_json::to_value(&cfg).unwrap();
    doc["version"] = serde_json::Value::String("2".into());
    let (back, r) = parse_document(&doc.to_string()).unwrap();
    assert_eq!(r.from_version, 2);
    assert_eq!(back.commands.len(), 1, "команды не должны потеряться");
}

#[test]
fn v2_unknown_fields_and_future_version() {
    let mut cfg = Config::default();
    cfg.normalize();
    let mut doc = serde_json::to_value(&cfg).unwrap();
    doc["futureField"] = serde_json::json!({"x": 1});
    doc["commands"] = serde_json::json!([{"id": "c", "name": "a", "extra": true}]);
    let (back, _) = parse_document(&doc.to_string()).expect("неизвестные поля игнорируются");
    assert_eq!(back.commands[0].name, "a");
    doc["version"] = serde_json::json!(3);
    assert!(parse_document(&doc.to_string()).is_err(), "конфиг из будущего — честный отказ");
}

#[test]
fn broken_config_file_does_not_block_startup() {
    let dir = tempfile::tempdir().unwrap();
    let paths = AppPaths::new(dir.path());
    paths.ensure_dirs().unwrap();
    for garbage in ["", "null", "[]", "{", "\u{feff}{\"version\":2}\r\n", "\"строка\""] {
        std::fs::write(paths.config_file(), garbage).unwrap();
        let l = load_or_create(&paths).unwrap_or_else(|e| panic!("«{garbage:?}» уронил загрузку: {e}"));
        assert_eq!(l.config.version, 2);
    }
    // сломанный файл отложен в резервную копию
    let backups: Vec<_> = std::fs::read_dir(paths.backups_dir()).unwrap().flatten().collect();
    assert!(backups.iter().any(|b| b.file_name().to_string_lossy().contains("broken")), "битый файл должен сохраниться как *.broken.*");
}

#[test]
fn overlay_paths_are_unique_and_never_empty() {
    let mut cfg = Config::default();
    cfg.overlays.push(Overlay { id: "1".into(), name: "Аудио".into(), path: "Аудио".into(), fallback: None, fallback_enabled: false }); // кириллица → пусто
    cfg.overlays.push(Overlay { id: "2".into(), name: "Audio".into(), path: "Audio".into(), fallback: None, fallback_enabled: false });
    cfg.overlays.push(Overlay { id: "3".into(), name: "audio".into(), path: "audio".into(), fallback: None, fallback_enabled: false });
    cfg.normalize();
    let paths: Vec<&str> = cfg.overlays.iter().map(|o| o.path.as_str()).collect();
    assert!(paths.iter().all(|p| !p.is_empty()), "пустой path: {paths:?}");
    let mut uniq = paths.clone();
    uniq.sort();
    uniq.dedup();
    assert_eq!(uniq.len(), paths.len(), "пути должны быть уникальны: {paths:?}");
}

#[test]
fn duplicate_command_names_and_aliases_are_resolved() {
    let mut cfg = Config::default();
    cfg.commands.push(Command { name: "Кусь".into(), aliases: vec!["bite".into()], ..Default::default() });
    cfg.commands.push(Command { name: "кусь".into(), ..Default::default() });
    cfg.commands.push(Command { name: "bite".into(), aliases: vec!["кусь".into(), "x".into()], ..Default::default() });
    cfg.normalize();
    let names: Vec<&str> = cfg.commands.iter().map(|c| c.name.as_str()).collect();
    let mut all: Vec<String> = cfg.commands.iter().flat_map(|c| std::iter::once(c.name.clone()).chain(c.aliases.iter().cloned())).collect();
    let n = all.len();
    all.sort();
    all.dedup();
    assert_eq!(all.len(), n, "имена и алиасы не должны пересекаться: {names:?} / {all:?}");
}

// ============================================================ движок / чат

fn engine(cfg: Config) -> (Arc<Engine>, OverlayHub, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let paths = AppPaths::new(dir.path());
    paths.ensure_dirs().unwrap();
    let config: SharedConfig = Arc::new(parking_lot::RwLock::new(cfg));
    let auth = AuthManager::new("cid".into(), Secrets::file_only(&paths));
    let helix = Arc::new(Helix::new(Arc::clone(&auth)));
    let hub = OverlayHub::new();
    let e = Engine::new(config, auth, helix, hub.clone(), paths.deleted_messages_log());
    (e, hub, dir)
}

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

fn media_cmd(name: &str, file: &str) -> Command {
    let mut c = Command { name: name.into(), ..Default::default() };
    c.response.media.enabled = true;
    c.response.media.file = file.into();
    c.response.media.overlay = Some("o1".into());
    c
}

fn base_cfg() -> Config {
    let mut cfg = Config::default();
    cfg.normalize();
    cfg.overlay_settings.antispam_window_ms = 0;
    cfg.overlays.push(Overlay { id: "o1".into(), name: "A".into(), path: "a".into(), fallback: None, fallback_enabled: false });
    cfg
}

async fn recv(rx: &mut tokio::sync::mpsc::Receiver<String>) -> Option<String> {
    tokio::time::timeout(Duration::from_millis(150), rx.recv()).await.ok().flatten()
}

#[tokio::test]
async fn weird_command_spellings() {
    let mut cfg = base_cfg();
    cfg.commands.push(media_cmd("кусь", "a.mp3"));
    cfg.commands.push(media_cmd("🐸", "frog.mp3"));
    let (e, hub, _d) = engine(cfg);
    let (_id, mut rx) = hub.connect("a", "t".into());

    let cases: &[(&str, bool)] = &[
        ("!КУСЬ", true),
        ("!кусь   ", true),
        ("  !кусь", false),          // не с начала строки — не команда (как в старой версии)
        ("!кусь\n", true),
        ("!", false),
        ("! кусь", false),
        ("！кусь", false),            // полноширинный «!»
        ("!🐸", true),
        ("!кусь@", false),           // нет такой команды
        ("!кусьь", false),
    ];
    for (text, expect) in cases {
        e.dispatch(chat("u", text)).await;
        let got = recv(&mut rx).await.is_some();
        assert_eq!(got, *expect, "текст {text:?}");
    }
}

#[tokio::test]
async fn reward_dedup_across_sources_and_repeats() {
    let mut cfg = base_cfg();
    let mut r = Reward { reward_id: "rw".into(), reward_title: "T".into(), ..Default::default() };
    r.response.media.enabled = true;
    r.response.media.file = "r.mp3".into();
    r.response.media.overlay = Some("o1".into());
    cfg.rewards.push(r);
    let (e, hub, _d) = engine(cfg);
    let (_id, mut rx) = hub.connect("a", "t".into());

    // EventSub + тот же чат-редемпшн в течение 5 с → одно медиа
    e.handle_reward("rw", "Alice", "привет", RewardSource::EventSub, Some("ev1")).await;
    e.handle_reward("rw", "alice", "  ПРИВЕТ ", RewardSource::Chat, None).await;
    assert!(recv(&mut rx).await.is_some());
    assert!(recv(&mut rx).await.is_none(), "кросс-источник в 5 с — дубликат");
    // тот же redemption id повторно (реконнект EventSub) → дубликат
    e.handle_reward("rw", "Alice", "привет", RewardSource::EventSub, Some("ev1")).await;
    assert!(recv(&mut rx).await.is_none());
    // тот же пользователь, другой redemption — честная вторая награда (антиспам выключен)
    e.handle_reward("rw", "Alice", "привет", RewardSource::EventSub, Some("ev2")).await;
    assert!(recv(&mut rx).await.is_some());
    // награда без реакции и с пустым именем — тишина, без паники
    e.handle_reward("nope", "", "", RewardSource::EventSub, None).await;
    assert!(recv(&mut rx).await.is_none());
}

#[tokio::test]
async fn injection_in_user_text_is_harmless() {
    let mut cfg = base_cfg();
    let mut c = media_cmd("say", "a.mp3");
    c.response.media.text.enabled = true;
    c.response.media.text.content = "{user}: {message}".into();
    cfg.commands.push(c);
    let (e, hub, _d) = engine(cfg);
    let (_id, mut rx) = hub.connect("a", "t".into());
    e.dispatch(chat("evil", "!say {user} {message} {viewers} <b>x</b> \"quote\"")).await;
    let msg = recv(&mut rx).await.unwrap();
    let v: serde_json::Value = serde_json::from_str(&msg).unwrap();
    let content = v["text"]["content"].as_str().unwrap();
    // Текст зрителя вставляется буквально: его «{user}» не превращается в имя автора.
    assert_eq!(content, "evil: {user} {message} {viewers} <b>x</b> \"quote\"", "текст зрителя должен вставляться как есть (экранирует textContent оверлея)");
}

#[test]
fn message_rendering_edge_cases() {
    let ctx = RenderCtx { author: "A".into(), target: None, vars: Default::default(), random_viewer: None };
    assert_eq!(render(&[Component::Random { min: 10, max: 1 }], &ctx).parse::<i64>().map(|n| (1..=10).contains(&n)), Ok(true));
    assert!(!render(&[Component::Random { min: i64::MIN, max: i64::MAX }], &ctx).is_empty());
    assert_eq!(render(&[Component::Phrase { phrases: vec![] }], &ctx), "");
    assert_eq!(render(&[Component::Target], &ctx), "@someone");
    assert_eq!(render(&[Component::Variable { name: "nope".into() }], &ctx), "");
    assert_eq!(render(&[Component::Space, Component::Space], &ctx), "");
}

// ============================================================ шатауты

#[test]
fn shoutout_names_with_at_and_case() {
    let q = ShoutoutQueue::new();
    let list = vec!["alice".to_string()];
    assert!(q.enqueue_message("Alice", &list));
    assert!(q.enqueue_manual("@alice").is_err(), "@ и регистр не должны обходить проверку «уже в очереди»");
    assert!(q.enqueue_manual("  ").is_err());
    assert!(q.remove(u64::MAX).is_err());
    q.reset_done(); // во время обработки — не паникует
    let (item, _) = q.next_ready().unwrap();
    assert_eq!(item.username, "Alice");
    q.begin(item.id);
    q.reset_done();
    q.finish(item.id, SendOutcome::Ok, Duration::from_secs(1));
    assert!(q.status().queue.is_empty());
}

// ============================================================ банворды

#[test]
fn banwords_with_regex_chars_spaces_and_scale() {
    let mk = |w: &str, kind| BanwordSettings { words: vec![BanWord { word: w.into(), kind, aliases: vec![] }], skip_privileged: false };
    let m = Matcher::compile(&mk("c++", BanWordKind::Soft));
    assert!(m.check("учу c++ сейчас").is_some());
    assert!(m.check("cpp").is_none());
    let m = Matcher::compile(&mk("(x)", BanWordKind::Hard));
    assert!(m.check("a(x)b").is_some());
    let m = Matcher::compile(&mk("плохое слово", BanWordKind::Soft));
    assert!(m.check("это плохое слово!").is_some());
    assert!(m.check("плохое  слово").is_some(), "двойной пробел схлопывается — обход не проходит");
    let m = Matcher::compile(&mk("", BanWordKind::Hard));
    assert!(m.check("что угодно").is_none(), "пустое слово не банит всё подряд");
    // масштаб: 200 слов × сообщение 5000 символов — быстро
    let words: Vec<BanWord> = (0..200).map(|i| BanWord { word: format!("слово{i}"), kind: if i % 2 == 0 { BanWordKind::Hard } else { BanWordKind::Soft }, aliases: vec![] }).collect();
    let m = Matcher::compile(&BanwordSettings { words, skip_privileged: false });
    let msg = "абв ".repeat(1250);
    let t = Instant::now();
    for _ in 0..20 {
        assert!(m.check(&msg).is_none());
    }
    assert!(t.elapsed() < Duration::from_millis(500), "медленно: {:?}", t.elapsed());
    assert!(generate_aliases("").is_empty() || generate_aliases("") == vec![""], "пустое слово");
    assert_eq!(generate_aliases("🐸🐸").len(), 1);
}

// ============================================================ периодика

#[test]
fn periodic_grid_edges() {
    let epoch = Instant::now();
    let ev = PeriodicEvent { interval_sec: 10, offset_sec: 9, ..Default::default() };
    assert_eq!(next_fire(epoch, epoch, &ev, false), epoch + Duration::from_secs(9));
    let ev = PeriodicEvent { interval_sec: 10, offset_sec: 10, ..Default::default() };
    assert_eq!(next_fire(epoch, epoch, &ev, false), epoch + Duration::from_secs(10), "offset == interval → как 0");
    let ev = PeriodicEvent { interval_sec: 0, offset_sec: 0, ..Default::default() };
    assert_eq!(next_fire(epoch, epoch, &ev, false), epoch + Duration::from_secs(10), "interval 0 → минимум 10");
    // далёкое прошлое: k большой, без переполнения
    let ev = PeriodicEvent { interval_sec: 10, offset_sec: 3, ..Default::default() };
    let now = epoch + Duration::from_secs(1_000_003);
    let n = next_fire(epoch, now, &ev, false);
    assert!(n > now && n <= now + Duration::from_secs(10));
}

// ============================================================ оверлеи

#[tokio::test]
async fn hub_pending_limits_and_multi_clients() {
    let hub = OverlayHub::new();
    for i in 0..30 {
        hub.send_to_path("v", &format!("m{i}"));
    }
    assert_eq!(hub.pending_count("v"), 20, "очередь ограничена 20 последними");
    let (_a, mut ra) = hub.connect("v", "1".into());
    let (_b, mut rb) = hub.connect("v", "2".into());
    let mut got = 0;
    while recv(&mut ra).await.is_some() {
        got += 1;
    }
    assert_eq!(got, 20, "отложенные уходят первому подключившемуся");
    assert!(recv(&mut rb).await.is_none());
    assert!(hub.send_to_path("v", "live"));
    assert_eq!(recv(&mut ra).await.as_deref(), Some("live"));
    assert_eq!(recv(&mut rb).await.as_deref(), Some("live"), "оба клиента одного path получают медиа");
    assert_eq!(hub.pending_count("nope"), 0);
}

#[test]
fn media_names_are_sanitized_but_readable() {
    let dir = tempfile::tempdir().unwrap();
    let paths = AppPaths::new(dir.path().join("d"));
    paths.ensure_dirs().unwrap();
    let src = dir.path().join("мой файл (1).PNG");
    std::fs::write(&src, [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 0]).unwrap();
    let f = signorebot_lib::media::import(&paths, &src).unwrap();
    assert_eq!(f.name, "мой_файл__1_.png");
    // файл без расширения, но с magic bytes mp3 (ID3)
    let src2 = dir.path().join("noext");
    std::fs::write(&src2, b"ID3\x03\x00\x00\x00\x00\x00\x00rest").unwrap();
    let f2 = signorebot_lib::media::import(&paths, &src2).unwrap();
    assert_eq!(f2.name, "noext.mp3");
    assert!(signorebot_lib::media::probe(&paths, "../x").is_err());
    assert!(signorebot_lib::media::probe(&paths, "nope.mp3").is_err());
}

#[test]
fn command_names_with_spaces_become_reachable() {
    let mut cfg = Config::default();
    cfg.commands.push(Command { name: "!My  Cmd ".into(), aliases: vec!["my alias".into()], ..Default::default() });
    cfg.normalize();
    assert_eq!(cfg.commands[0].name, "mycmd");
    assert_eq!(cfg.commands[0].aliases, vec!["myalias"]);
}

#[tokio::test]
async fn event_with_empty_user_gets_placeholder() {
    let mut cfg = base_cfg();
    let mut r = EventReaction { enabled: true, ..Default::default() };
    r.response.media.enabled = true;
    r.response.media.file = "f.mp3".into();
    r.response.media.overlay = Some("o1".into());
    r.response.media.text.enabled = true;
    r.response.media.text.content = "Привет, {user}!".into();
    cfg.events.insert("follow".into(), r);
    let (e, hub, _d) = engine(cfg);
    let (_id, mut rx) = hub.connect("a", "t".into());
    e.dispatch(TwitchEvent::Follow { user_name: "   ".into(), user_id: "".into() }).await;
    let msg = recv(&mut rx).await.expect("реакция должна сработать");
    let v: serde_json::Value = serde_json::from_str(&msg).unwrap();
    assert_eq!(v["text"]["content"], "Привет, someone!");
}
