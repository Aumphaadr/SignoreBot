//! Сборка чат-сообщения из компонентов и подстановка переменных.

use crate::config::Component;
use std::collections::BTreeMap;

/// Лимит Twitch на длину сообщения в чате.
pub const MAX_CHAT_LEN: usize = 500;

#[derive(Debug, Default, Clone)]
pub struct RenderCtx {
    /// Автор (без `@`).
    pub author: String,
    /// Цель — первый аргумент команды (без `@`), если есть.
    pub target: Option<String>,
    /// Дополнительные переменные `{name}`.
    pub vars: BTreeMap<String, String>,
    /// Заранее выбранный случайный зритель (если нужен).
    pub random_viewer: Option<String>,
}

impl RenderCtx {
    pub fn needs_random_viewer(components: &[Component], target: &Option<String>) -> bool {
        components.iter().any(|c| match c {
            Component::RandomViewer => true,
            Component::Target => target.as_deref().map(|t| t.is_empty() || t == "someone").unwrap_or(true),
            _ => false,
        })
    }
}

/// Подставить `{user}`, `{target}` и `{var}` в текст — одним проходом, чтобы
/// текст, пришедший от зрителя (например, `{message}` = «{user}»), не
/// интерпретировался повторно.
pub fn substitute(text: &str, ctx: &RenderCtx) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find('{') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        match after.find('}') {
            Some(end) if after[..end].chars().all(|c| c.is_alphanumeric() || c == '_') && end > 0 => {
                let key = &after[..end];
                let val = match key {
                    "user" => Some(ctx.author.as_str()),
                    "target" => Some(ctx.target.as_deref().unwrap_or("")),
                    _ => ctx.vars.get(key).map(|s| s.as_str()),
                };
                match val {
                    Some(v) => out.push_str(v),
                    None => out.push_str(&rest[start..start + 1 + end + 1]),
                }
                rest = &after[end + 1..];
            }
            _ => {
                out.push('{');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

fn at(name: &str) -> String {
    if name.starts_with('@') {
        name.to_string()
    } else {
        format!("@{name}")
    }
}

/// Собрать сообщение. Пустая строка — нечего отправлять.
pub fn render(components: &[Component], ctx: &RenderCtx) -> String {
    use rand::seq::SliceRandom;
    use rand::Rng;
    let mut out = String::new();
    let fallback = ctx.random_viewer.clone().unwrap_or_else(|| "someone".into());
    for c in components {
        match c {
            Component::Static { value } => out.push_str(&substitute(value, ctx)),
            Component::Author => out.push_str(&at(&ctx.author)),
            Component::Target => match ctx.target.as_deref() {
                Some(t) if !t.is_empty() && t != "someone" => out.push_str(&at(t)),
                _ => out.push_str(&at(&fallback)),
            },
            Component::RandomViewer => out.push_str(&at(&fallback)),
            Component::Random { min, max } => {
                let (lo, hi) = if min <= max { (*min, *max) } else { (*max, *min) };
                let n = rand::thread_rng().gen_range(lo..=hi);
                out.push_str(&n.to_string());
            }
            Component::Phrase { phrases } => {
                let valid: Vec<&String> = phrases.iter().filter(|p| !p.trim().is_empty()).collect();
                if let Some(p) = valid.choose(&mut rand::thread_rng()) {
                    out.push_str(&substitute(p, ctx));
                }
            }
            Component::Space => out.push(' '),
            Component::Variable { name } => {
                if let Some(v) = ctx.vars.get(name) {
                    out.push_str(v);
                }
            }
        }
    }
    let trimmed = out.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    truncate_chars(trimmed, MAX_CHAT_LEN)
}

pub fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> RenderCtx {
        let mut vars = BTreeMap::new();
        vars.insert("viewers".into(), "42".into());
        RenderCtx { author: "Alice".into(), target: Some("bob".into()), vars, random_viewer: Some("carol".into()) }
    }

    #[test]
    fn renders_components() {
        let comps = vec![
            Component::Author,
            Component::Static { value: " покусал ".into() },
            Component::Target,
            Component::Space,
            Component::Static { value: "({viewers} чел., {user} → {target})".into() },
        ];
        assert_eq!(render(&comps, &ctx()), "@Alice покусал @bob (42 чел., Alice → bob)");
    }

    #[test]
    fn target_fallback_and_random() {
        let mut c = ctx();
        c.target = None;
        let comps = vec![Component::Target, Component::Space, Component::RandomViewer];
        assert_eq!(render(&comps, &c), "@carol @carol");
        assert!(RenderCtx::needs_random_viewer(&comps, &None));
        assert!(!RenderCtx::needs_random_viewer(&[Component::Target], &Some("x".into())));
        let n = render(&[Component::Random { min: 5, max: 5 }], &c);
        assert_eq!(n, "5");
        let p = render(&[Component::Phrase { phrases: vec!["".into(), "hi {user}".into()] }], &c);
        assert_eq!(p, "hi Alice");
        assert_eq!(render(&[Component::Variable { name: "viewers".into() }], &c), "42");
        assert_eq!(render(&[Component::Static { value: "  ".into() }], &c), "");
    }

    #[test]
    fn truncates() {
        let long = "я".repeat(600);
        let r = render(&[Component::Static { value: long }], &ctx());
        assert_eq!(r.chars().count(), 500);
        assert!(r.ends_with('…'));
    }
}
