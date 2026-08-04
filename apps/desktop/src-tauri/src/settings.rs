use crate::model::AppSettings;
use anyhow::{Context, Result};
use std::fs;
use std::path::PathBuf;

pub fn config_path() -> Result<PathBuf> {
    let directory = dirs::config_dir()
        .context("the operating system did not provide a config directory")?
        .join("quill");
    Ok(directory.join("settings.json"))
}

pub fn load() -> AppSettings {
    let Ok(path) = config_path() else {
        return AppSettings::default();
    };
    let mut settings: AppSettings = fs::read_to_string(path)
        .ok()
        .and_then(|contents| serde_json::from_str(&contents).ok())
        .unwrap_or_default();
    let dismissed_count = settings.dismissed_suggestions.len();
    settings.cap_dismissed_suggestions();
    let backend_changed = settings.normalize_backend_for_platform();
    if settings.dismissed_suggestions.len() != dismissed_count || backend_changed {
        // Best-effort migration: the in-memory value is bounded even if the
        // existing settings file cannot be rewritten.
        let _ = save(&settings);
    }
    settings
}

pub fn save(settings: &AppSettings) -> Result<()> {
    let path = config_path()?;
    if let Some(directory) = path.parent() {
        fs::create_dir_all(directory).context("failed to create Quill config directory")?;
    }
    let pending = path.with_extension("json.pending");
    fs::write(&pending, serde_json::to_vec_pretty(settings)?)
        .context("failed to write pending Quill settings")?;
    fs::rename(&pending, &path).context("failed to commit Quill settings")?;
    Ok(())
}
