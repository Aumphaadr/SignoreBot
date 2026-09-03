//! Каталог медиа: список, импорт (с проверкой magic bytes), удаление,
//! проверка кодеков, поиск «сирот».

use crate::config::Config;
use crate::paths::{safe_file_name, AppPaths};
use serde::Serialize;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

pub const MAX_FILE_SIZE: u64 = 500 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "api.ts")]
pub struct MediaFile {
    pub name: String,
    #[ts(type = "number")]
    pub size: u64,
    /// Unix-время изменения, мс.
    #[ts(type = "number")]
    pub modified: i64,
    /// image | video | audio | unknown
    pub kind: String,
    /// Используется ли в конфиге.
    pub used: bool,
}

#[derive(Debug, Clone, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "api.ts")]
pub struct ProbeResult {
    pub file: String,
    #[ts(type = "number")]
    pub size: u64,
    pub kind: String,
    pub extension: String,
    pub codec: Option<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum MediaError {
    #[error("недопустимое имя файла")]
    BadName,
    #[error("файл не найден")]
    NotFound,
    #[error("файл слишком большой (максимум 500 МБ)")]
    TooBig,
    #[error("неподдерживаемый тип файла: {0}")]
    Unsupported(String),
    #[error("ошибка ввода-вывода: {0}")]
    Io(#[from] std::io::Error),
}

pub fn kind_of(name: &str) -> &'static str {
    let ext = Path::new(name).extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
    match ext.as_str() {
        "jpg" | "jpeg" | "png" | "gif" | "webp" | "bmp" | "apng" => "image",
        "mp4" | "webm" | "mov" | "m4v" | "mkv" | "avi" | "flv" | "ogv" => "video",
        "mp3" | "wav" | "ogg" | "oga" | "m4a" | "flac" | "aac" | "opus" => "audio",
        _ => "unknown",
    }
}

/// Все имена файлов, на которые ссылается конфиг.
pub fn referenced_files(cfg: &Config) -> HashSet<String> {
    let mut set = HashSet::new();
    let mut add = |r: &crate::config::Response| {
        if !r.media.file.is_empty() {
            set.insert(r.media.file.clone());
        }
        if !r.media.secondary_file.is_empty() {
            set.insert(r.media.secondary_file.clone());
        }
    };
    cfg.commands.iter().for_each(|c| add(&c.response));
    cfg.rewards.iter().for_each(|r| add(&r.response));
    cfg.events.values().for_each(|e| add(&e.response));
    cfg.periodic_events.iter().for_each(|p| add(&p.response));
    set
}

pub fn list(paths: &AppPaths, cfg: &Config) -> Result<Vec<MediaFile>, MediaError> {
    let used = referenced_files(cfg);
    let mut out = Vec::new();
    for e in std::fs::read_dir(paths.media_dir())?.flatten() {
        let Ok(meta) = e.metadata() else { continue };
        if !meta.is_file() {
            continue;
        }
        let name = e.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        let modified = meta.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_millis() as i64).unwrap_or(0);
        out.push(MediaFile { used: used.contains(&name), kind: kind_of(&name).into(), name, size: meta.len(), modified });
    }
    out.sort_by_key(|f| std::cmp::Reverse(f.modified));
    Ok(out)
}

pub fn delete(paths: &AppPaths, name: &str) -> Result<(), MediaError> {
    let name = safe_file_name(name).ok_or(MediaError::BadName)?;
    let p = paths.media_dir().join(name);
    if !p.is_file() {
        return Err(MediaError::NotFound);
    }
    std::fs::remove_file(p)?;
    Ok(())
}

/// Уникальное имя в каталоге медиа (при коллизии — суффикс `-2`, `-3`…).
fn unique_name(dir: &Path, wanted: &str) -> String {
    if !dir.join(wanted).exists() {
        return wanted.to_string();
    }
    let p = Path::new(wanted);
    let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("file");
    let ext = p.extension().and_then(|s| s.to_str()).map(|e| format!(".{e}")).unwrap_or_default();
    for i in 2..10_000 {
        let cand = format!("{stem}-{i}{ext}");
        if !dir.join(&cand).exists() {
            return cand;
        }
    }
    format!("{stem}-{}{ext}", chrono::Utc::now().timestamp_millis())
}

/// Скопировать файл в каталог медиа. Тип определяется по содержимому;
/// расширение приводится к реальному типу.
pub fn import(paths: &AppPaths, source: &Path) -> Result<MediaFile, MediaError> {
    let meta = std::fs::metadata(source)?;
    if !meta.is_file() {
        return Err(MediaError::NotFound);
    }
    if meta.len() > MAX_FILE_SIZE {
        return Err(MediaError::TooBig);
    }
    let detected = infer::get_from_path(source)?;
    let Some(t) = detected else {
        return Err(MediaError::Unsupported("не удалось распознать формат".into()));
    };
    let mime = t.mime_type();
    let real_ext = t.extension();
    let top = mime.split('/').next().unwrap_or("");
    if !matches!(top, "image" | "video" | "audio") {
        return Err(MediaError::Unsupported(mime.to_string()));
    }
    let orig = source.file_name().and_then(|s| s.to_str()).unwrap_or("file");
    let mut base = Path::new(orig).file_stem().and_then(|s| s.to_str()).unwrap_or("file").to_string();
    // только безопасные символы в имени
    base = base.chars().map(|c| if c.is_alphanumeric() || c == '-' || c == '_' || c == '.' { c } else { '_' }).collect();
    if base.is_empty() {
        base = "file".into();
    }
    let ext = if real_ext == "mpga" { "mp3" } else { real_ext };
    let wanted = format!("{base}.{ext}");
    let dir = paths.media_dir();
    std::fs::create_dir_all(&dir)?;
    let name = unique_name(&dir, &wanted);
    let dest = dir.join(&name);
    std::fs::copy(source, &dest)?;
    let modified = chrono::Utc::now().timestamp_millis();
    Ok(MediaFile { kind: kind_of(&name).into(), name, size: meta.len(), modified, used: false })
}

pub fn probe(paths: &AppPaths, name: &str) -> Result<ProbeResult, MediaError> {
    let name = safe_file_name(name).ok_or(MediaError::BadName)?;
    let p = paths.media_dir().join(&name);
    let meta = std::fs::metadata(&p).map_err(|_| MediaError::NotFound)?;
    let ext = Path::new(&name).extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
    let kind = kind_of(&name).to_string();
    let mut warnings = Vec::new();
    let mut codec = None;
    if kind == "video" {
        let head = read_head(&p, 16384).unwrap_or_default();
        let ascii: String = head.iter().map(|b| if b.is_ascii_graphic() { *b as char } else { '.' }).collect();
        if ascii.contains("hvc1") || ascii.contains("hev1") {
            codec = Some("H.265/HEVC".into());
            warnings.push("Видео в H.265/HEVC: большинство браузеров (и OBS) его не воспроизводят. Перекодируйте в H.264 (MP4).".into());
        } else if ascii.contains("avc1") || ascii.contains("avc3") {
            codec = Some("H.264/AVC".into());
        } else if ascii.contains("vp08") {
            codec = Some("VP8".into());
        } else if ascii.contains("vp09") {
            codec = Some("VP9".into());
        } else if ascii.contains("av01") {
            codec = Some("AV1".into());
            warnings.push("Видео в AV1: поддержка в OBS может быть ограничена.".into());
        }
        match ext.as_str() {
            "mkv" => warnings.push("MKV плохо поддерживается браузерами. Рекомендуется MP4 или WebM.".into()),
            "avi" | "flv" => warnings.push(format!("{} не воспроизводится браузерами. Конвертируйте в MP4 или WebM.", ext.to_uppercase())),
            "mov" => warnings.push("MOV может не воспроизводиться. Рекомендуется MP4.".into()),
            _ => {}
        }
    } else if kind == "audio" {
        match ext.as_str() {
            "flac" => warnings.push("FLAC может не поддерживаться. Рекомендуется MP3 или OGG.".into()),
            "aac" => warnings.push("AAC без контейнера может не воспроизводиться; лучше M4A или MP3.".into()),
            _ => {}
        }
    } else if kind == "unknown" {
        warnings.push("Неизвестный тип файла.".into());
    }
    Ok(ProbeResult { file: name, size: meta.len(), kind, extension: ext, codec, warnings })
}

fn read_head(p: &Path, n: usize) -> std::io::Result<Vec<u8>> {
    use std::io::Read;
    let mut f = std::fs::File::open(p)?;
    let mut buf = vec![0u8; n];
    let got = f.read(&mut buf)?;
    buf.truncate(got);
    Ok(buf)
}

/// Импорт всей папки медиа старой версии (только распознанные медиа-файлы).
pub fn import_dir(paths: &AppPaths, dir: &Path) -> (usize, Vec<String>) {
    let mut ok = 0;
    let mut errors = Vec::new();
    let Ok(rd) = std::fs::read_dir(dir) else { return (0, vec![format!("каталог {} недоступен", dir.display())]) };
    for e in rd.flatten() {
        let p = e.path();
        if !p.is_file() {
            continue;
        }
        let name = e.file_name().to_string_lossy().to_string();
        if paths.media_dir().join(&name).exists() {
            ok += 1; // уже есть — считаем перенесённым
            continue;
        }
        match copy_keeping_name(paths, &p) {
            Ok(()) => ok += 1,
            Err(err) => errors.push(format!("{name}: {err}")),
        }
    }
    (ok, errors)
}

/// Копия с сохранением имени (для миграции: имена уже в конфиге).
fn copy_keeping_name(paths: &AppPaths, source: &Path) -> Result<(), MediaError> {
    let name = source.file_name().and_then(|s| s.to_str()).and_then(safe_file_name).ok_or(MediaError::BadName)?;
    let meta = std::fs::metadata(source)?;
    if meta.len() > MAX_FILE_SIZE {
        return Err(MediaError::TooBig);
    }
    let t = infer::get_from_path(source)?.ok_or_else(|| MediaError::Unsupported("не распознан".into()))?;
    let top = t.mime_type().split('/').next().unwrap_or("");
    if !matches!(top, "image" | "video" | "audio") {
        return Err(MediaError::Unsupported(t.mime_type().into()));
    }
    std::fs::copy(source, paths.media_dir().join(name))?;
    Ok(())
}

pub fn media_path(paths: &AppPaths, name: &str) -> Option<PathBuf> {
    safe_file_name(name).map(|n| paths.media_dir().join(n))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn import_detects_type_and_rejects_html() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::new(dir.path().join("data"));
        paths.ensure_dirs().unwrap();
        // PNG magic
        let png = dir.path().join("evil.html");
        std::fs::write(&png, [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 0]).unwrap();
        let f = import(&paths, &png).unwrap();
        assert_eq!(f.name, "evil.png");
        assert_eq!(f.kind, "image");
        let f2 = import(&paths, &png).unwrap();
        assert_eq!(f2.name, "evil-2.png");
        // не медиа
        let txt = dir.path().join("a.txt");
        std::fs::write(&txt, "<html>").unwrap();
        assert!(matches!(import(&paths, &txt), Err(MediaError::Unsupported(_))));
        let cfg = Config::default();
        let l = list(&paths, &cfg).unwrap();
        assert_eq!(l.len(), 2);
        assert!(!l[0].used);
        delete(&paths, "evil.png").unwrap();
        assert!(matches!(delete(&paths, "../x"), Err(MediaError::BadName)));
    }

    #[test]
    fn probe_warns() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::new(dir.path());
        paths.ensure_dirs().unwrap();
        std::fs::write(paths.media_dir().join("v.mp4"), b"....ftypisom....hvc1....").unwrap();
        let r = probe(&paths, "v.mp4").unwrap();
        assert_eq!(r.codec.as_deref(), Some("H.265/HEVC"));
        assert_eq!(r.warnings.len(), 1);
    }
}
