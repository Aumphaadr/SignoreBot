//! Проверка обновлений по релизам GitHub.

use serde::Serialize;

#[derive(Debug, Clone, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "api.ts")]
pub struct UpdateInfo {
    pub current: String,
    pub latest: Option<String>,
    pub is_newer: bool,
    pub url: Option<String>,
    pub published_at: Option<String>,
    pub notes: Option<String>,
    /// Ссылки на файлы релиза.
    pub assets: Vec<UpdateAsset>,
    /// Unix-время проверки, мс.
    #[ts(type = "number")]
    pub checked_at: i64,
}

#[derive(Debug, Clone, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "api.ts")]
pub struct UpdateAsset {
    pub name: String,
    pub url: String,
    #[ts(type = "number")]
    pub size: u64,
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

pub async fn check(repo_url: &str) -> Result<UpdateInfo, String> {
    let current = env!("CARGO_PKG_VERSION").to_string();
    let (owner, repo) = parse_repo(repo_url).ok_or_else(|| format!("Некорректная ссылка на репозиторий: {repo_url}"))?;
    let api = format!("https://api.github.com/repos/{owner}/{repo}/releases/latest");
    let client = reqwest::Client::builder().user_agent("SignoreBot/0.1").timeout(std::time::Duration::from_secs(15)).build().map_err(|e| e.to_string())?;
    let resp = client.get(&api).header("Accept", "application/vnd.github+json").send().await.map_err(|e| format!("сеть: {e}"))?;
    let checked_at = chrono::Utc::now().timestamp_millis();
    if resp.status().as_u16() == 404 {
        return Ok(UpdateInfo { current, latest: None, is_newer: false, url: Some(format!("https://github.com/{owner}/{repo}/releases")), published_at: None, notes: Some("В репозитории пока нет релизов".into()), assets: vec![], checked_at });
    }
    if !resp.status().is_success() {
        return Err(format!("GitHub ответил {}", resp.status()));
    }
    let v: serde_json::Value = resp.json().await.map_err(|e| format!("ответ GitHub не разобран: {e}"))?;
    let tag = v["tag_name"].as_str().unwrap_or("").to_string();
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
    Ok(UpdateInfo {
        is_newer: !tag.is_empty() && is_newer(&tag, &current),
        current,
        latest: if tag.is_empty() { None } else { Some(tag) },
        url: v["html_url"].as_str().map(String::from),
        published_at: v["published_at"].as_str().map(String::from),
        notes: v["body"].as_str().map(|s| s.chars().take(4000).collect()),
        assets,
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
    }
}
