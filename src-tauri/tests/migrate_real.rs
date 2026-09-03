//! Миграция конфига v1 (фикстура — вымышленный «стартовый набор начинающего
//! стримера»: служебные команды, соцсети, развлечения, звуки, награды, события).

use signorebot_lib::config::store::parse_document;
use signorebot_lib::config::*;

#[test]
fn v1_config_migrates_cleanly() {
    let text = include_str!("fixtures/v1-config.json");
    let (cfg, report) = parse_document(text).unwrap();
    assert_eq!(report.from_version, 1);
    assert_eq!(cfg.commands.len(), 15);
    assert_eq!(cfg.rewards.len(), 8);
    assert_eq!(cfg.overlays.len(), 3);
    assert_eq!(cfg.periodic_events.len(), 3);
    assert_eq!(cfg.events.len(), 5);
    assert_eq!(cfg.banwords.words[0].aliases.len(), 8);
    assert_eq!(cfg.shoutout.auto_list.len(), 2);
    assert_eq!(cfg.accounts.broadcaster.as_ref().unwrap().login, "cozy_stream");
    assert_eq!(cfg.accounts.bot.as_ref().unwrap().login, "cozy_stream_bot");
    let hug = cfg.commands.iter().find(|c| c.name == "обнять").unwrap();
    assert_eq!(hug.aliases, vec!["hug"]);
    assert_eq!(hug.response.chat.components.len(), 4);
    let sound = cfg.commands.iter().find(|c| c.name == "звук").unwrap();
    assert_eq!(sound.permissions, vec!["moderators"]);
    assert_eq!(sound.response.media.volume, 40);
    let boo = cfg.rewards.iter().find(|r| r.reward_title == "Бу!").unwrap();
    assert_eq!(boo.response.media.queue_mode, QueueMode::Immediate);
    let vip = cfg.rewards.iter().find(|r| r.reward_title == "Показать VIP-алерт").unwrap();
    assert_eq!(vip.response.media.text.position, "below");
    assert_eq!(cfg.periodic_events.iter().map(|p| p.offset_sec).max(), Some(1500));
    // единственная ожидаемая заметка — про токены и про периодику
    for n in &report.notes {
        assert!(n.contains("Токены") || n.contains("Периодические"), "неожиданная заметка: {n}");
    }
    let follow = &cfg.events["follow"];
    assert_eq!(follow.response.media.text.font.font_family.as_deref(), Some("'Fira Code', sans-serif"));
}
