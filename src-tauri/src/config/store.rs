//! Загрузка/сохранение конфига: атомарная запись, резервные копии, миграция.

use super::migrate::{parse_any, MigrationReport};
use super::schema::Config;
use crate::paths::AppPaths;
use std::path::Path;

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("ошибка ввода-вывода: {0}")]
    Io(#[from] std::io::Error),
    #[error("ошибка JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("{0}")]
    Migrate(#[from] super::migrate::MigrateError),
}

/// Результат загрузки.
pub struct Loaded {
    pub config: Config,
    pub report: Option<MigrationReport>,
    pub created: bool,
}

/// Прочитать конфиг из каталога приложения. Если файла нет — создать
/// дефолтный. Если файл старой версии — мигрировать, сохранив резервную копию.
pub fn load_or_create(paths: &AppPaths) -> Result<Loaded, StoreError> {
    paths.ensure_dirs()?;
    let file = paths.config_file();
    if !file.exists() {
        let mut cfg = Config::default();
        cfg.normalize();
        save(paths, &cfg)?;
        return Ok(Loaded { config: cfg, report: None, created: true });
    }
    let text = std::fs::read_to_string(&file)?;
    let text = text.trim_start_matches('\u{feff}');
    let parsed = serde_json::from_str::<serde_json::Value>(text).map_err(StoreError::from).and_then(|doc| parse_any(&doc).map_err(StoreError::from));
    let (cfg, report) = match parsed {
        Ok(v) => v,
        Err(e) => {
            // Файл повреждён: откладываем его в резервные копии и стартуем с
            // пустыми настройками — приложение должно открыться и рассказать об этом.
            backup_file(paths, &file, "broken")?;
            let mut cfg = Config::default();
            cfg.normalize();
            save(paths, &cfg)?;
            let report = MigrationReport {
                from_version: 0,
                notes: vec![format!(
                    "Файл настроек не удалось прочитать ({e}). Он сохранён в config-backups как *.broken.*, а бот запущен с пустыми настройками. Импортируйте резервную копию или старый config.json на вкладке «Состояние»."
                )],
            };
            return Ok(Loaded { config: cfg, report: Some(report), created: true });
        }
    };
    let migrated = report.from_version != super::schema::CONFIG_VERSION;
    if migrated {
        backup_file(paths, &file, &format!("v{}", report.from_version))?;
        save(paths, &cfg)?;
    }
    Ok(Loaded { config: cfg, report: if migrated { Some(report) } else { None }, created: false })
}

/// Разобрать произвольный JSON-документ (импорт файла старой версии).
pub fn parse_document(text: &str) -> Result<(Config, MigrationReport), StoreError> {
    let doc: serde_json::Value = serde_json::from_str(text.trim_start_matches('\u{feff}'))?;
    Ok(parse_any(&doc)?)
}

/// Атомарная запись: во временный файл рядом, затем `rename`.
pub fn save(paths: &AppPaths, cfg: &Config) -> Result<(), StoreError> {
    let file = paths.config_file();
    write_atomic(&file, &serde_json::to_vec_pretty(cfg)?)
}

pub fn write_atomic(file: &Path, bytes: &[u8]) -> Result<(), StoreError> {
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = file.with_extension(format!("tmp-{}", std::process::id()));
    std::fs::write(&tmp, bytes)?;
    if let Err(e) = std::fs::rename(&tmp, file) {
        let _ = std::fs::remove_file(&tmp);
        return Err(e.into());
    }
    Ok(())
}

/// Копия файла в `config-backups/<имя>.<метка>.<время>.json`.
pub fn backup_file(paths: &AppPaths, file: &Path, label: &str) -> Result<(), StoreError> {
    if !file.exists() {
        return Ok(());
    }
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let name = file.file_stem().and_then(|s| s.to_str()).unwrap_or("config");
    let dest = paths.backups_dir().join(format!("{name}.{label}.{stamp}.json"));
    std::fs::create_dir_all(paths.backups_dir())?;
    std::fs::copy(file, dest)?;
    Ok(())
}

/// Экспорт для пользователя: тот же конфиг, но без сведений об аккаунтах
/// (ключ оверлеев тоже не отдаём — он локальный секрет).
pub fn export_document(cfg: &Config) -> serde_json::Value {
    let mut c = cfg.clone();
    c.accounts = Default::default();
    c.network.overlay_key = String::new();
    c.obs.password = String::new();
    serde_json::to_value(c).unwrap_or(serde_json::Value::Null)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_loads_and_migrates() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::new(dir.path());
        let l = load_or_create(&paths).unwrap();
        assert!(l.created);
        assert!(paths.config_file().exists());

        // подсунем старый формат
        std::fs::write(paths.config_file(), r#"{"commands":{"!a":{"name":"a"}},"overlays":[]}"#).unwrap();
        let l = load_or_create(&paths).unwrap();
        assert!(!l.created);
        assert_eq!(l.report.unwrap().from_version, 1);
        assert_eq!(l.config.commands[0].name, "a");
        let backups: Vec<_> = std::fs::read_dir(paths.backups_dir()).unwrap().collect();
        assert_eq!(backups.len(), 1);

        // теперь на диске v2 — повторная загрузка без миграции
        let l = load_or_create(&paths).unwrap();
        assert!(l.report.is_none());
        assert_eq!(l.config.version, 2);
    }

    #[test]
    fn export_strips_private() {
        let mut c = Config::default();
        c.normalize();
        c.accounts.bot = Some(super::super::schema::AccountInfo { login: "b".into(), ..Default::default() });
        let v = export_document(&c);
        assert!(v["accounts"]["bot"].is_null());
        assert_eq!(v["network"]["overlayKey"], "");
    }
}
