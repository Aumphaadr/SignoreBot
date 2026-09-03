//! Где лежат данные приложения.
//!
//! По умолчанию — стандартные каталоги ОС (через Tauri `path()`), но для
//! разработки и тестов всё можно перенаправить переменной `SIGNOREBOT_DATA_DIR`.
//!
//! Пользователь может перенести данные на другой диск: тогда в стандартном
//! каталоге остаётся только файл-указатель `data-dir.txt` с путём.

use std::path::{Path, PathBuf};

/// Имя файла-указателя в стандартном каталоге.
pub const POINTER_FILE: &str = "data-dir.txt";

#[derive(Debug, Clone)]
pub struct AppPaths {
    pub root: PathBuf,
    /// Стандартный каталог ОС (там лежит указатель, если данные перенесены).
    pub default_root: PathBuf,
    /// Откуда взят `root`.
    pub source: PathSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "api.ts")]
pub enum PathSource {
    Default,
    Pointer,
    Env,
}

impl AppPaths {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        let root = root.into();
        Self { default_root: root.clone(), root, source: PathSource::Default }
    }

    /// Каталог из окружения (dev/tests) либо `None`.
    pub fn from_env() -> Option<Self> {
        std::env::var_os("SIGNOREBOT_DATA_DIR").map(|v| {
            let mut p = Self::new(PathBuf::from(v));
            p.source = PathSource::Env;
            p
        })
    }

    /// Стандартный каталог ОС с учётом файла-указателя: если в нём лежит
    /// `data-dir.txt` с существующим каталогом — данные там.
    pub fn from_default(default_root: PathBuf) -> Self {
        match read_pointer(&default_root) {
            Some(custom) if custom.is_dir() => Self { root: custom, default_root, source: PathSource::Pointer },
            Some(custom) => {
                tracing::warn!(target: "signorebot::core", "Каталог данных из указателя недоступен ({}); используется стандартный", custom.display());
                Self::new(default_root)
            }
            None => Self::new(default_root),
        }
    }

    /// Записать/снять указатель на пользовательский каталог.
    pub fn write_pointer(default_root: &Path, custom: Option<&Path>) -> std::io::Result<()> {
        std::fs::create_dir_all(default_root)?;
        let file = default_root.join(POINTER_FILE);
        match custom {
            Some(p) => std::fs::write(file, p.to_string_lossy().as_bytes()),
            None => match std::fs::remove_file(file) {
                Ok(()) => Ok(()),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(e) => Err(e),
            },
        }
    }

    /// Что переносится при смене каталога (относительно root).
    pub fn movable_entries() -> [&'static str; 4] {
        ["config.json", "secrets.json", "media", "config-backups"]
    }

    pub fn config_file(&self) -> PathBuf {
        self.root.join("config.json")
    }
    pub fn media_dir(&self) -> PathBuf {
        self.root.join("media")
    }
    pub fn logs_dir(&self) -> PathBuf {
        self.root.join("logs")
    }
    pub fn backups_dir(&self) -> PathBuf {
        self.root.join("config-backups")
    }
    /// Запасное хранилище токенов, если системный keyring недоступен.
    pub fn secrets_fallback_file(&self) -> PathBuf {
        self.root.join("secrets.json")
    }
    pub fn deleted_messages_log(&self) -> PathBuf {
        self.logs_dir().join("deleted_messages.log")
    }

    pub fn ensure_dirs(&self) -> std::io::Result<()> {
        for d in [&self.root, &self.media_dir(), &self.logs_dir(), &self.backups_dir()] {
            std::fs::create_dir_all(d)?;
        }
        Ok(())
    }
}

fn read_pointer(default_root: &Path) -> Option<PathBuf> {
    let text = std::fs::read_to_string(default_root.join(POINTER_FILE)).ok()?;
    let t = text.trim().trim_start_matches('\u{feff}');
    if t.is_empty() {
        None
    } else {
        Some(PathBuf::from(t))
    }
}

/// Перенос данных в другой каталог (`None` — назад в стандартный).
/// При `copy` содержимое копируется (старое не удаляется); `config_json` —
/// актуальный конфиг из памяти. Пишет указатель; применяется после
/// перезапуска. Возвращает новый каталог и число скопированных файлов.
pub fn relocate(paths: &AppPaths, target: Option<PathBuf>, copy: bool, config_json: &[u8]) -> Result<(PathBuf, u64), String> {
    if paths.source == PathSource::Env {
        return Err("каталог задан переменной окружения SIGNOREBOT_DATA_DIR — из панели его не сменить".into());
    }
    let custom = target.is_some();
    let target = target.unwrap_or_else(|| paths.default_root.clone());
    if !target.is_absolute() {
        return Err("нужен полный путь к папке".into());
    }
    let same = |a: &Path, b: &Path| a.canonicalize().ok().zip(b.canonicalize().ok()).map(|(x, y)| x == y).unwrap_or(a == b);
    if same(&target, &paths.root) {
        return Err("это и есть текущий каталог данных".into());
    }
    if target.starts_with(&paths.root) || paths.root.starts_with(&target) {
        return Err("новая папка не должна быть внутри текущей (и наоборот)".into());
    }
    std::fs::create_dir_all(&target).map_err(|e| format!("не удалось создать папку: {e}"))?;
    let probe = target.join(".signorebot-write-test");
    std::fs::write(&probe, b"ok").map_err(|e| format!("в папку нельзя писать: {e}"))?;
    let _ = std::fs::remove_file(&probe);

    let mut copied = 0u64;
    if copy {
        if target.join("config.json").exists() {
            if custom {
                return Err("в выбранной папке уже есть данные SignoreBot (config.json). Выберите пустую папку или отключите копирование, чтобы использовать те данные".into());
            }
            // возврат в стандартную папку: старый конфиг там откладываем в резервные копии
            let bak_dir = target.join("config-backups");
            std::fs::create_dir_all(&bak_dir).map_err(|e| e.to_string())?;
            let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
            std::fs::rename(target.join("config.json"), bak_dir.join(format!("config.before-return.{stamp}.json"))).map_err(|e| e.to_string())?;
        }
        std::fs::write(target.join("config.json"), config_json).map_err(|e| e.to_string())?;
        copied += 1;
        for name in AppPaths::movable_entries().iter().filter(|n| **n != "config.json") {
            let from = paths.root.join(name);
            let to = target.join(name);
            if from.is_dir() {
                copied += copy_dir_all(&from, &to).map_err(|e| format!("{name}: {e}"))?;
            } else if from.is_file() {
                std::fs::copy(&from, &to).map_err(|e| format!("{name}: {e}"))?;
                copied += 1;
            }
        }
    }
    AppPaths::write_pointer(&paths.default_root, if custom { Some(&target) } else { None }).map_err(|e| format!("не удалось записать указатель: {e}"))?;
    Ok((target, copied))
}

/// Рекурсивное копирование каталога (файлы перезаписываются).
pub fn copy_dir_all(src: &Path, dst: &Path) -> std::io::Result<u64> {
    let mut n = 0;
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let to = dst.join(entry.file_name());
        if ty.is_dir() {
            n += copy_dir_all(&entry.path(), &to)?;
        } else if ty.is_file() {
            std::fs::copy(entry.path(), &to)?;
            n += 1;
        }
    }
    Ok(n)
}

/// Безопасное имя файла внутри каталога: только последний компонент, без
/// разделителей и `..`. Возвращает `None`, если имя недопустимо.
pub fn safe_file_name(name: &str) -> Option<String> {
    let trimmed = name.trim();
    if trimmed.is_empty() || trimmed == "." || trimmed == ".." {
        return None;
    }
    if trimmed.contains('/') || trimmed.contains('\\') || trimmed.contains('\0') {
        return None;
    }
    // Path::file_name отбрасывает всё лишнее; сверяем, что ничего не изменилось.
    let p = Path::new(trimmed);
    match p.file_name().and_then(|s| s.to_str()) {
        Some(f) if f == trimmed => Some(f.to_string()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_names() {
        assert_eq!(safe_file_name("a.mp3"), Some("a.mp3".into()));
        assert_eq!(safe_file_name(" a.mp3 "), Some("a.mp3".into()));
        assert_eq!(safe_file_name("../x"), None);
        assert_eq!(safe_file_name("..\\x"), None);
        assert_eq!(safe_file_name(".."), None);
        assert_eq!(safe_file_name(""), None);
        assert_eq!(safe_file_name("dir/x.mp4"), None);
    }

    #[test]
    fn pointer_roundtrip_and_fallback() {
        let dir = tempfile::tempdir().unwrap();
        let default_root = dir.path().join("default");
        let custom = dir.path().join("custom");
        // без указателя — стандартный
        let p = AppPaths::from_default(default_root.clone());
        assert_eq!(p.root, default_root);
        assert_eq!(p.source, PathSource::Default);
        // указатель на несуществующий каталог — стандартный с предупреждением
        AppPaths::write_pointer(&default_root, Some(&custom)).unwrap();
        assert_eq!(AppPaths::from_default(default_root.clone()).source, PathSource::Default);
        // каталог появился — берём его
        std::fs::create_dir_all(&custom).unwrap();
        let p = AppPaths::from_default(default_root.clone());
        assert_eq!(p.root, custom);
        assert_eq!(p.source, PathSource::Pointer);
        assert_eq!(p.default_root, default_root);
        // снятие указателя
        AppPaths::write_pointer(&default_root, None).unwrap();
        assert_eq!(AppPaths::from_default(default_root.clone()).source, PathSource::Default);
        AppPaths::write_pointer(&default_root, None).unwrap(); // повторно — не ошибка
    }

    #[test]
    fn relocate_copies_and_points_then_returns() {
        let dir = tempfile::tempdir().unwrap();
        let default_root = dir.path().join("default");
        let paths = AppPaths::new(default_root.clone());
        paths.ensure_dirs().unwrap();
        std::fs::write(paths.media_dir().join("a.mp3"), b"x").unwrap();
        std::fs::write(paths.secrets_fallback_file(), b"{}").unwrap();
        std::fs::write(paths.config_file(), b"stale").unwrap();
        let custom = dir.path().join("disk-d").join("SignoreBot");
        // ошибки: относительный путь, тот же каталог, вложенный
        assert!(relocate(&paths, Some(PathBuf::from("rel")), true, b"{}").is_err());
        assert!(relocate(&paths, Some(default_root.clone()), true, b"{}").is_err());
        assert!(relocate(&paths, Some(default_root.join("inner")), true, b"{}").is_err());
        // перенос с копированием: конфиг — из памяти, не с диска
        let (dir2, n) = relocate(&paths, Some(custom.clone()), true, b"{\"fresh\":1}").unwrap();
        assert_eq!(dir2, custom);
        assert_eq!(n, 3); // config.json + secrets.json + media/a.mp3
        assert_eq!(std::fs::read_to_string(custom.join("config.json")).unwrap(), "{\"fresh\":1}");
        assert!(custom.join("media/a.mp3").exists());
        assert!(paths.config_file().exists(), "старое не удаляется");
        // после перезапуска берётся новый каталог
        let reloaded = AppPaths::from_default(default_root.clone());
        assert_eq!(reloaded.root, custom);
        // повторное копирование в занятую папку — отказ; без копирования — можно
        assert!(relocate(&reloaded, Some(dir.path().join("disk-d/SignoreBot")), true, b"{}").is_err());
        let other = dir.path().join("other");
        std::fs::create_dir_all(&other).unwrap();
        std::fs::write(other.join("config.json"), b"{}").unwrap();
        assert!(relocate(&reloaded, Some(other), true, b"{}").is_err());
        // возврат к стандартному с копированием: старый конфиг там уходит в резервные копии
        let (back, n) = relocate(&reloaded, None, true, b"{\"back\":1}").unwrap();
        assert_eq!(back, default_root);
        assert!(n >= 1);
        assert_eq!(std::fs::read_to_string(default_root.join("config.json")).unwrap(), "{\"back\":1}");
        assert!(std::fs::read_dir(default_root.join("config-backups")).unwrap().flatten().any(|e| e.file_name().to_string_lossy().starts_with("config.before-return.")));
        assert_eq!(AppPaths::from_default(default_root.clone()).source, PathSource::Default);
        // env-режим — запрет
        let env = AppPaths { root: default_root.clone(), default_root: default_root.clone(), source: PathSource::Env };
        assert!(relocate(&env, Some(dir.path().join("z")), false, b"{}").is_err());
    }

    #[test]
    fn copy_dir_recursive() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src");
        std::fs::create_dir_all(src.join("a/b")).unwrap();
        std::fs::write(src.join("x.txt"), "1").unwrap();
        std::fs::write(src.join("a/b/y.txt"), "2").unwrap();
        let dst = dir.path().join("dst");
        assert_eq!(copy_dir_all(&src, &dst).unwrap(), 2);
        assert_eq!(std::fs::read_to_string(dst.join("a/b/y.txt")).unwrap(), "2");
    }
}
