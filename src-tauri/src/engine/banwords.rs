//! Банворды: генерация обходных написаний (с ограничением) и матчер,
//! компилируемый один раз на конфиг.

use crate::config::{BanWordKind, BanwordSettings};
use std::collections::BTreeSet;

/// Максимум вариантов на слово: дальше комбинаторика бессмысленна —
/// нормализация сообщения и так сводит латиницу к кириллице.
pub const MAX_ALIASES: usize = 512;

fn char_map(c: char) -> &'static [char] {
    match c {
        'а' => &['a'],
        'б' => &['6'],
        'в' => &['b'],
        'г' => &['r'],
        'е' => &['e'],
        'к' => &['k'],
        'м' => &['m'],
        'н' => &['h'],
        'о' => &['o'],
        'р' => &['p'],
        'с' => &['c'],
        'т' => &['t'],
        'у' => &['y'],
        'х' => &['x'],
        'ь' => &['b'],
        'a' => &['а'],
        'b' => &['в', 'ь'],
        'c' => &['с'],
        'e' => &['е'],
        'h' => &['н'],
        'k' => &['к'],
        'm' => &['м'],
        'o' => &['о'],
        'p' => &['р'],
        't' => &['т'],
        'r' => &['г'],
        'x' => &['х'],
        'y' => &['у'],
        _ => &[],
    }
}

/// Варианты написания слова (декартово произведение замен), не более `MAX_ALIASES`.
pub fn generate_aliases(word: &str) -> Vec<String> {
    let original: Vec<char> = word.to_lowercase().chars().collect();
    let mut out: BTreeSet<String> = BTreeSet::new();
    out.insert(original.iter().collect());
    let positions: Vec<(usize, &'static [char])> =
        original.iter().enumerate().filter_map(|(i, c)| { let m = char_map(*c); if m.is_empty() { None } else { Some((i, m)) } }).collect();
    let mut stack: Vec<(Vec<char>, usize)> = vec![(original.clone(), 0)];
    while let Some((cur, pos)) = stack.pop() {
        if out.len() >= MAX_ALIASES {
            break;
        }
        if pos == positions.len() {
            out.insert(cur.iter().collect());
            continue;
        }
        let (idx, reps) = positions[pos];
        for r in reps {
            let mut n = cur.clone();
            n[idx] = *r;
            stack.push((n, pos + 1));
        }
        stack.push((cur, pos + 1));
    }
    let mut v: Vec<String> = out.into_iter().collect();
    // оригинал первым
    let orig: String = original.iter().collect();
    v.retain(|x| *x != orig);
    v.insert(0, orig);
    v
}

/// Латиница/цифры → похожая кириллица.
pub fn normalize(msg: &str) -> String {
    msg.chars()
        .map(|c| match c {
            '@' | 'a' => 'а',
            'b' => 'в',
            '6' => 'б',
            'c' => 'с',
            'e' | '3' => 'е',
            'h' => 'н',
            'x' => 'х',
            'i' | '1' => 'и',
            'o' | '0' => 'о',
            'p' => 'р',
            'y' => 'у',
            'k' => 'к',
            'm' => 'м',
            't' => 'т',
            other => other,
        })
        .collect()
}

/// Схлопнуть повторы символов: «прииивееет» → «привет».
pub fn dedupe_repeats(msg: &str) -> String {
    let mut out = String::new();
    let mut prev: Option<char> = None;
    for c in msg.chars() {
        if prev != Some(c) {
            out.push(c);
        }
        prev = Some(c);
    }
    out
}

#[derive(Debug, Clone)]
pub struct Hit {
    pub word: String,
    pub kind: BanWordKind,
}

/// Матчер: один автомат Ахо-Корасик по всем вариантам всех слов; для
/// «мягких» слов дополнительно проверяются границы слова вокруг совпадения.
pub struct Matcher {
    ac: Option<aho_corasick::AhoCorasick>,
    /// Для каждого паттерна — индекс слова.
    owner: Vec<usize>,
    words: Vec<(String, BanWordKind)>,
}

fn is_boundary(c: Option<char>) -> bool {
    match c {
        None => true,
        Some(c) => c.is_whitespace() || ".,!?;:()[]{}\"'_-«»—".contains(c),
    }
}

impl Matcher {
    pub fn compile(settings: &BanwordSettings) -> Self {
        let mut patterns: Vec<String> = Vec::new();
        let mut owner = Vec::new();
        let mut words = Vec::new();
        for w in &settings.words {
            let word = w.word.trim().to_lowercase();
            if word.is_empty() {
                continue;
            }
            let idx = words.len();
            words.push((word.clone(), w.kind));
            let mut needles = generate_aliases(&word);
            for a in &w.aliases {
                let a = a.trim().to_lowercase();
                if !a.is_empty() && !needles.contains(&a) {
                    needles.push(a);
                }
            }
            for n in needles {
                if !n.is_empty() {
                    patterns.push(n);
                    owner.push(idx);
                }
            }
        }
        let ac = if patterns.is_empty() {
            None
        } else {
            aho_corasick::AhoCorasick::builder()
                .match_kind(aho_corasick::MatchKind::Standard)
                .build(&patterns)
                .ok()
        };
        Self { ac, owner, words }
    }

    fn check_variant(&self, v: &str) -> Option<Hit> {
        let ac = self.ac.as_ref()?;
        for m in ac.find_overlapping_iter(v) {
            let (word, kind) = &self.words[self.owner[m.pattern().as_usize()]];
            match kind {
                BanWordKind::Hard => return Some(Hit { word: word.clone(), kind: *kind }),
                BanWordKind::Soft => {
                    let before = v[..m.start()].chars().next_back();
                    let after = v[m.end()..].chars().next();
                    if is_boundary(before) && is_boundary(after) {
                        return Some(Hit { word: word.clone(), kind: *kind });
                    }
                }
            }
        }
        None
    }

    pub fn check(&self, message: &str) -> Option<Hit> {
        if self.ac.is_none() || message.is_empty() {
            return None;
        }
        let lower = message.to_lowercase();
        let mut variants = vec![lower.clone(), normalize(&lower), dedupe_repeats(&lower)];
        variants.push(normalize(&dedupe_repeats(&lower)));
        variants.dedup();
        variants.iter().find_map(|v| self.check_variant(v))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::BanWord;

    #[test]
    fn aliases_capped_and_include_original() {
        let a = generate_aliases("спам");
        assert_eq!(a[0], "спам");
        assert!(a.contains(&"cпam".to_string()));
        assert_eq!(a.len(), 8); // с, а, м подменяются: 2³
        let big = generate_aliases("абвгекмнорстухьабвгекмнорс");
        assert_eq!(big.len(), MAX_ALIASES);
    }

    fn settings(word: &str, kind: BanWordKind) -> BanwordSettings {
        BanwordSettings { words: vec![BanWord { word: word.into(), kind, aliases: vec![] }], skip_privileged: false }
    }

    #[test]
    fn hard_matches_substrings_and_obfuscation() {
        let m = Matcher::compile(&settings("спам", BanWordKind::Hard));
        assert!(m.check("это СПАМ!").is_some());
        assert!(m.check("cпaм").is_some()); // латиница
        assert!(m.check("спаааам").is_some()); // повторы
        assert!(m.check("антиспамер").is_some()); // подстрока
        assert!(m.check("привет").is_none());
    }

    #[test]
    fn soft_requires_word_boundary() {
        let m = Matcher::compile(&settings("кот", BanWordKind::Soft));
        assert!(m.check("кот").is_some());
        assert!(m.check("а кот, да!").is_some());
        assert!(m.check("котлета").is_none());
        assert!(m.check("скот").is_none());
    }

    #[test]
    fn empty_settings_never_match() {
        let m = Matcher::compile(&BanwordSettings::default());
        assert!(m.check("anything").is_none());
    }
}
