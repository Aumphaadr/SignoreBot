//! Проверка обновлений: `version.json` сайта (по умолчанию), релизы GitHub
//! (репозиторий форка) или прямая ссылка на JSON любого из двух форматов.

use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "api.ts")]
pub struct UpdateInfo {
    pub current: String,
    pub latest: Option<String>,
    pub is_newer: bool,
    /// Куда вести за обновлением: страница скачивания сайта (или страница релиза).
    pub url: Option<String>,
    /// Страница релиза на GitHub, если известна.
    pub release_url: Option<String>,
    pub published_at: Option<String>,
    pub notes: Option<String>,
    /// Ссылки на файлы релиза.
    pub assets: Vec<UpdateAsset>,
    /// Версия объявлена на сайте, но её файлы ещё не выложены — обновление не предлагается.
    pub pending: Option<String>,
    /// Unix-время проверки, мс.
    #[ts(type = "number")]
    pub checked_at: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "api.ts")]
pub struct UpdateAsset {
    pub name: String,
    pub url: String,
    #[ts(type = "number")]
    pub size: u64,
}

/// Откуда брать сведения о версиях.
#[derive(Debug, Clone, PartialEq)]
pub enum Source {
    /// Сайт с `version.json`; `base` кончается на `/`.
    Site { base: String },
    /// Репозиторий GitHub: API «последний релиз».
    GitHub { owner: String, repo: String },
    /// Прямая ссылка на JSON — формат по содержимому (`version` или `tag_name`).
    Json { url: String },
}

/// `https://github.com/owner/repo[/...]` → `(owner, repo)`.
pub fn parse_repo(url: &str) -> Option<(String, String)> {
    let u = url.trim().trim_end_matches('/');
    let rest = u.strip_prefix("https://github.com/").or_else(|| u.strip_prefix("http://github.com/")).or_else(|| u.strip_prefix("github.com/"))?;
    let mut it = rest.split('/');
    let owner = it.next()?.to_string();
    let repo = it.next()?.trim_end_matches(".git").to_string();
    if owner.is_empty() || repo.is_empty() {
        return None;
    }
    Some((owner, repo))
}

/// Прежний адрес по умолчанию — репозиторий SignoreBot (мигрирует на сайт).
pub fn is_default_repo(url: &str) -> bool {
    matches!(parse_repo(url), Some((o, r)) if o.eq_ignore_ascii_case("Aumphaadr") && r.eq_ignore_ascii_case("SignoreBot"))
}

pub fn parse_source(s: &str) -> Result<Source, String> {
    let s = s.trim();
    if let Some((owner, repo)) = parse_repo(s) {
        return Ok(Source::GitHub { owner, repo });
    }
    if !(s.starts_with("http://") || s.starts_with("https://")) {
        return Err(format!("Некорректный адрес для проверки обновлений: {s}"));
    }
    let path = s.split('#').next().unwrap_or(s).split('?').next().unwrap_or(s);
    if path.to_ascii_lowercase().ends_with(".json") {
        return Ok(Source::Json { url: path.to_string() });
    }
    Ok(Source::Site { base: format!("{}/", path.trim_end_matches('/')) })
}

/// Сравнение версий вида `v1.2.3` / `1.2.3-beta`: только числовая часть.
pub fn parse_version(v: &str) -> Vec<u64> {
    v.trim().trim_start_matches(['v', 'V']).split(['-', '+']).next().unwrap_or("").split('.').map(|p| p.parse::<u64>().unwrap_or(0)).collect()
}

pub fn is_newer(latest: &str, current: &str) -> bool {
    let (a, b) = (parse_version(latest), parse_version(current));
    let n = a.len().max(b.len());
    for i in 0..n {
        let (x, y) = (a.get(i).copied().unwrap_or(0), b.get(i).copied().unwrap_or(0));
        if x != y {
            return x > y;
        }
    }
    false
}

/// Разобранный ответ источника — без сравнения с текущей версией.
#[derive(Debug, Clone, PartialEq)]
pub struct Release {
    pub version: String,
    pub page: Option<String>,
    pub release_url: Option<String>,
    pub published_at: Option<String>,
    pub notes: Option<String>,
    pub assets: Vec<UpdateAsset>,
    /// Файлы по видам (setup, portable, zip, deb, appimage) — для проверки, что они уже выложены.
    pub files: Vec<(String, String)>,
}

/// Виды файлов в порядке, в котором их показывать.
const KINDS: &[&str] = &["setup", "portable", "zip", "deb", "appimage"];

/// `version.json` сайта (`version`, `date`, `notes`, `page`, `release`, `files`, `sizes`)
/// или ответ GitHub «последний релиз» (`tag_name`, `html_url`, `assets`…).
pub fn parse_release(v: &Value, source: &Source) -> Result<Release, String> {
    let s = |k: &str| v[k].as_str().map(|x| x.trim()).filter(|x| !x.is_empty()).map(String::from);
    if let Some(version) = s("version") {
        let base = match source {
            Source::Site { base } => base.clone(),
            Source::Json { url } => format!("{}/", url.rsplit_once('/').map(|(d, _)| d).unwrap_or(url)),
            Source::GitHub { .. } => String::new(),
        };
        let files: Vec<(String, String)> = KINDS.iter().filter_map(|k| v["files"][*k].as_str().map(|u| (k.to_string(), u.to_string()))).collect();
        let assets = files
            .iter()
            .map(|(k, u)| UpdateAsset { name: u.rsplit('/').next().unwrap_or(u).to_string(), url: u.clone(), size: v["sizes"][k.as_str()].as_u64().unwrap_or(0) })
            .collect();
        return Ok(Release {
            version,
            page: s("page").or_else(|| if base.is_empty() { None } else { Some(format!("{base}#download")) }),
            release_url: s("release"),
            published_at: s("date"),
            notes: v["notes"].as_str().map(|t| t.chars().take(4000).collect()),
            assets,
            files,
        });
    }
    if let Some(tag) = s("tag_name") {
        let assets = v["assets"]
            .as_array()
            .map(|a| {
                a.iter()
                    .map(|x| UpdateAsset {
                        name: x["name"].as_str().unwrap_or("").into(),
                        url: x["browser_download_url"].as_str().unwrap_or("").into(),
                        size: x["size"].as_u64().unwrap_or(0),
                    })
                    .collect()
            })
            .unwrap_or_default();
        return Ok(Release { version: tag, page: s("html_url"), release_url: s("html_url"), published_at: s("published_at"), notes: v["body"].as_str().map(|t| t.chars().take(4000).collect()), assets, files: vec![] });
    }
    Err("в ответе нет ни «version» (version.json сайта), ни «tag_name» (релиз GitHub)".into())
}

/// Какой файл релиза проверять на наличие: свой для этой системы, иначе первый.
pub fn probe_file(files: &[(String, String)]) -> Option<&str> {
    let prefer: &[&str] = if cfg!(windows) { &["setup", "portable", "zip"] } else { &["deb", "appimage"] };
    prefer.iter().find_map(|k| files.iter().find(|(kind, _)| kind == k)).or_else(|| files.first()).map(|(_, u)| u.as_str())
}

pub async fn check(source_url: &str) -> Result<UpdateInfo, String> {
    let current = env!("CARGO_PKG_VERSION").to_string();
    let source = parse_source(source_url)?;
    let (api, not_found) = match &source {
        Source::Site { base } => (format!("{base}version.json"), format!("На сайте нет version.json ({base}version.json) — проверьте адрес в настройках")),
        Source::GitHub { owner, repo } => (format!("https://api.github.com/repos/{owner}/{repo}/releases/latest"), String::new()),
        Source::Json { url } => (url.clone(), format!("По адресу {url} ничего нет — проверьте адрес в настройках")),
    };
    let client = reqwest::Client::builder().user_agent("SignoreBot/0.1").timeout(std::time::Duration::from_secs(15)).build().map_err(|e| e.to_string())?;
    let resp = client.get(&api).header("Accept", "application/json").send().await.map_err(|e| format!("сеть: {e}"))?;
    let checked_at = chrono::Utc::now().timestamp_millis();
    if resp.status().as_u16() == 404 {
        if let Source::GitHub { owner, repo } = &source {
            let releases = format!("https://github.com/{owner}/{repo}/releases");
            return Ok(UpdateInfo { current, latest: None, is_newer: false, url: Some(releases.clone()), release_url: Some(releases), published_at: None, notes: Some("В репозитории пока нет релизов".into()), assets: vec![], pending: None, checked_at });
        }
        return Err(not_found);
    }
    if !resp.status().is_success() {
        return Err(format!("Сервер обновлений ответил {}", resp.status()));
    }
    let v: Value = resp.json().await.map_err(|e| format!("ответ не разобран: {e}"))?;
    let r = parse_release(&v, &source)?;
    let mut is_newer = is_newer(&r.version, &current);
    let mut pending = None;
    // Сайт может объявить версию раньше, чем её файлы попадут в релиз (push раньше
    // публикации релиза). Пока файла нет — молчим и не зовём обновляться.
    if is_newer {
        if let Some(file) = probe_file(&r.files) {
            let available = match client.head(file).send().await {
                Ok(h) => h.status().is_success() || h.status().as_u16() == 405,
                Err(e) => {
                    tracing::warn!(target: "signorebot::updates", "Не удалось проверить файл релиза {file}: {e}");
                    true
                }
            };
            if !available {
                tracing::info!(target: "signorebot::updates", "Версия {} объявлена на сайте, но файл {file} ещё не выложен — ждём", r.version);
                is_newer = false;
                pending = Some(r.version.clone());
            }
        }
    }
    Ok(UpdateInfo {
        latest: if pending.is_some() { None } else { Some(r.version) },
        is_newer,
        current,
        url: r.page,
        release_url: r.release_url,
        published_at: r.published_at,
        notes: if pending.is_some() { None } else { r.notes },
        assets: r.assets,
        pending,
        checked_at,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_and_repo() {
        assert!(is_newer("v0.2.0", "0.1.0"));
        assert!(is_newer("1.0", "0.9.9"));
        assert!(!is_newer("v0.1.0", "0.1.0"));
        assert!(!is_newer("0.1.0-beta", "0.1.0"));
        assert!(is_newer("0.1.1", "0.1.0-rc1"));
        assert_eq!(parse_repo("https://github.com/Aumphaadr/SignoreBot"), Some(("Aumphaadr".into(), "SignoreBot".into())));
        assert_eq!(parse_repo("https://github.com/Aumphaadr/SignoreBot.git/"), Some(("Aumphaadr".into(), "SignoreBot".into())));
        assert_eq!(parse_repo("https://gitlab.com/x/y"), None);
        assert_eq!(parse_repo("https://github.com/only"), None);
        assert!(is_default_repo("https://github.com/Aumphaadr/SignoreBot/"));
        assert!(!is_default_repo("https://github.com/someone/SignoreBot"));
    }

    #[test]
    fn sources() {
        assert_eq!(parse_source("https://aumphaadr.github.io/SignoreBot").unwrap(), Source::Site { base: "https://aumphaadr.github.io/SignoreBot/".into() });
        assert_eq!(parse_source(" https://aumphaadr.github.io/SignoreBot/#download ").unwrap(), Source::Site { base: "https://aumphaadr.github.io/SignoreBot/".into() });
        assert_eq!(parse_source("http://127.0.0.1:8765/version.json").unwrap(), Source::Json { url: "http://127.0.0.1:8765/version.json".into() });
        assert_eq!(parse_source("https://github.com/x/y").unwrap(), Source::GitHub { owner: "x".into(), repo: "y".into() });
        assert!(parse_source("abc").is_err());
    }

    #[test]
    fn parses_site_and_github_formats() {
        let site = Source::Site { base: "https://x.example/SignoreBot/".into() };
        let v = serde_json::json!({ "version": "1.0.6", "date": "2026-10-01", "notes": "- что-то", "release": "https://github.com/x/y/releases/tag/v1.0.6",
            "files": { "deb": "https://github.com/x/y/releases/download/v1.0.6/SignoreBot_1.0.6-linux-amd64.deb", "setup": "https://github.com/x/y/releases/download/v1.0.6/SignoreBot_1.0.6-windows-x64-setup.exe" },
            "sizes": { "deb": 12000000 } });
        let r = parse_release(&v, &site).unwrap();
        assert_eq!(r.version, "1.0.6");
        assert_eq!(r.page.as_deref(), Some("https://x.example/SignoreBot/#download"));
        assert_eq!(r.release_url.as_deref(), Some("https://github.com/x/y/releases/tag/v1.0.6"));
        assert_eq!(r.files.iter().map(|(k, _)| k.as_str()).collect::<Vec<_>>(), vec!["setup", "deb"]);
        assert_eq!(r.assets[1].name, "SignoreBot_1.0.6-linux-amd64.deb");
        assert_eq!(r.assets[1].size, 12000000);
        assert_eq!(r.assets[0].size, 0);
        let expect = if cfg!(windows) { "setup.exe" } else { "amd64.deb" };
        assert!(probe_file(&r.files).unwrap().ends_with(expect));
        // прямой JSON без page — страница рядом с файлом
        let j = Source::Json { url: "http://127.0.0.1:8765/version.json".into() };
        assert_eq!(parse_release(&serde_json::json!({ "version": "2.0.0" }), &j).unwrap().page.as_deref(), Some("http://127.0.0.1:8765/#download"));
        // формат GitHub
        let g = serde_json::json!({ "tag_name": "v1.0.3", "html_url": "https://github.com/x/y/releases/tag/v1.0.3", "body": "текст", "assets": [{ "name": "a.deb", "browser_download_url": "https://x/a.deb", "size": 5 }] });
        let r = parse_release(&g, &Source::GitHub { owner: "x".into(), repo: "y".into() }).unwrap();
        assert_eq!((r.version.as_str(), r.page.as_deref(), r.assets.len(), r.files.len()), ("v1.0.3", Some("https://github.com/x/y/releases/tag/v1.0.3"), 1, 0));
        assert!(parse_release(&serde_json::json!({ "foo": 1 }), &site).is_err());
    }
}
