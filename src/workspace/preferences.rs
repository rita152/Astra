//! Versioned UI preferences and atomic local persistence.

use std::{
    collections::BTreeSet,
    fs::{self, File, OpenOptions},
    io::{ErrorKind, Write},
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

use serde::{Deserialize, Serialize};

use crate::agent::{ProjectId, ThreadSectionId};

const PREFERENCES_VERSION: u32 = 1;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UiPreferences {
    #[serde(default = "preferences_version")]
    pub version: u32,
    #[serde(default)]
    pub pinned_section_id: Option<ThreadSectionId>,
    #[serde(default)]
    pub collapsed_project_ids: BTreeSet<ProjectId>,
    #[serde(default)]
    pub pinned_collapsed: bool,
    #[serde(default)]
    pub projects_collapsed: bool,
    #[serde(default)]
    pub recent_collapsed: bool,
    #[serde(default)]
    pub review: ReviewPreferences,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ReviewPreferences {
    pub split: bool,
    pub wrap: bool,
    pub load_files: bool,
    pub rich: bool,
    pub words: bool,
    pub ignore_whitespace: bool,
    pub tree_open: bool,
}
impl Default for ReviewPreferences {
    fn default() -> Self {
        Self {
            split: false,
            wrap: false,
            load_files: true,
            rich: false,
            words: false,
            ignore_whitespace: false,
            tree_open: true,
        }
    }
}

fn preferences_version() -> u32 {
    PREFERENCES_VERSION
}

impl UiPreferences {
    pub(super) fn current() -> Self {
        Self {
            version: PREFERENCES_VERSION,
            ..Self::default()
        }
    }
}

#[derive(Debug)]
pub(super) struct PreferenceStore {
    path: PathBuf,
    write_serial: AtomicU64,
}

impl PreferenceStore {
    pub(super) fn new(path: PathBuf) -> Self {
        Self {
            path,
            write_serial: AtomicU64::new(1),
        }
    }

    pub(super) fn load(&self) -> Result<UiPreferences, String> {
        let bytes = match fs::read(&self.path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == ErrorKind::NotFound => {
                return Ok(UiPreferences::current());
            }
            Err(error) => return Err(format!("无法读取 UI 偏好：{error}")),
        };
        let preferences: UiPreferences =
            serde_json::from_slice(&bytes).map_err(|error| format!("UI 偏好格式无效：{error}"))?;
        if preferences.version != PREFERENCES_VERSION {
            return Ok(UiPreferences::current());
        }
        Ok(preferences)
    }

    pub(super) fn save(&self, preferences: &UiPreferences) -> Result<(), String> {
        let Some(parent) = self.path.parent() else {
            return Err("UI 偏好路径缺少父目录".to_owned());
        };
        fs::create_dir_all(parent).map_err(|error| format!("无法创建 UI 偏好目录：{error}"))?;
        let serial = self.write_serial.fetch_add(1, Ordering::Relaxed);
        let file_name = self
            .path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("ui-preferences.json");
        let temporary = parent.join(format!(
            ".{file_name}.{}.{}.tmp",
            std::process::id(),
            serial
        ));
        let write_result = (|| {
            let mut file = OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&temporary)
                .map_err(|error| format!("无法创建 UI 偏好临时文件：{error}"))?;
            let bytes = serde_json::to_vec_pretty(preferences)
                .map_err(|error| format!("无法序列化 UI 偏好：{error}"))?;
            file.write_all(&bytes)
                .map_err(|error| format!("无法写入 UI 偏好：{error}"))?;
            file.write_all(b"\n")
                .map_err(|error| format!("无法完成 UI 偏好写入：{error}"))?;
            file.sync_all()
                .map_err(|error| format!("无法同步 UI 偏好：{error}"))?;
            fs::rename(&temporary, &self.path)
                .map_err(|error| format!("无法原子替换 UI 偏好：{error}"))?;
            if let Ok(directory) = File::open(parent) {
                let _ = directory.sync_all();
            }
            Ok(())
        })();
        if write_result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        write_result
    }
}

pub(super) fn default_preferences_path() -> PathBuf {
    if let Some(path) = std::env::var_os("GPUI_UI_PREFERENCES_PATH") {
        return PathBuf::from(path);
    }
    #[cfg(target_os = "macos")]
    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home)
            .join("Library/Application Support/GPUI")
            .join("ui-preferences.json");
    }
    if let Some(config) = std::env::var_os("XDG_CONFIG_HOME") {
        return PathBuf::from(config)
            .join("gpui")
            .join("ui-preferences.json");
    }
    PathBuf::from(".gpui-ui-preferences.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_mismatch_does_not_reuse_old_preferences() {
        let directory = std::env::temp_dir().join(format!(
            "gpui-workspace-preferences-{}-{}",
            std::process::id(),
            PreferenceStore::new(PathBuf::new())
                .write_serial
                .fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join("preferences.json");
        fs::write(&path, br#"{"version":999,"pinned_section_id":"stale"}"#).unwrap();
        let loaded = PreferenceStore::new(path.clone()).load().unwrap();
        assert_eq!(loaded, UiPreferences::current());
        fs::remove_file(path).unwrap();
        fs::remove_dir(directory).unwrap();
    }

    #[test]
    fn preferences_are_written_as_complete_versioned_json() {
        static SERIAL: AtomicU64 = AtomicU64::new(1);
        let directory = std::env::temp_dir().join(format!(
            "gpui-workspace-atomic-{}-{}",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        let path = directory.join("preferences.json");
        let store = PreferenceStore::new(path.clone());
        let mut preferences = UiPreferences::current();
        preferences.pinned_section_id = Some("section-1".into());
        store.save(&preferences).unwrap();
        let bytes = fs::read(&path).unwrap();
        assert!(bytes.ends_with(b"\n"));
        assert_eq!(
            serde_json::from_slice::<UiPreferences>(&bytes).unwrap(),
            preferences
        );
        assert_eq!(fs::read_dir(&directory).unwrap().count(), 1);
        fs::remove_file(path).unwrap();
        fs::remove_dir(directory).unwrap();
    }
}
