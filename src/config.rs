// ============================================================================
// config.rs — Persistent User Configuration & Playlist State for GTKPlayer
// ============================================================================

use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::theme::ThemeMode;

fn default_volume() -> i32 {
    80
}

fn default_headroom() -> bool {
    true
}

fn default_eq() -> [f64; 10] {
    [0.0; 10]
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SavedTrack {
    pub path: PathBuf,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub artist: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UserConfig {
    #[serde(default)]
    pub theme: ThemeMode,
    #[serde(default = "default_volume")]
    pub volume_pct: i32,
    #[serde(default)]
    pub balance_pct: i32,
    #[serde(default = "default_headroom")]
    pub auto_headroom: bool,
    #[serde(default = "default_eq")]
    pub eq_bands: [f64; 10],
    #[serde(default)]
    pub playlist: Vec<SavedTrack>,
    #[serde(default)]
    pub current_idx: Option<usize>,
}

impl Default for UserConfig {
    fn default() -> Self {
        Self {
            theme: ThemeMode::FrostLight,
            volume_pct: 80,
            balance_pct: 0,
            auto_headroom: true,
            eq_bands: [0.0; 10],
            playlist: Vec::new(),
            current_idx: None,
        }
    }
}

impl UserConfig {
    pub fn config_path() -> Option<PathBuf> {
        dirs::config_dir().map(|d| d.join("gtkplayer").join("config.json"))
    }

    pub fn load() -> Self {
        if let Some(p) = Self::config_path() {
            if let Ok(raw) = fs::read_to_string(&p) {
                if let Ok(cfg) = serde_json::from_str::<Self>(&raw) {
                    return cfg;
                }
            }
        }
        // Fallback: import existing playlist & EQ state from audio_engine_v10 or fltk_player
        for app_dir in ["audio_engine_v10", "fltk_player"] {
            if let Some(legacy) = dirs::config_dir().map(|d| d.join(app_dir).join("config.json")) {
                if let Ok(raw) = fs::read_to_string(&legacy) {
                    if let Ok(mut cfg) = serde_json::from_str::<Self>(&raw) {
                        cfg.theme = ThemeMode::FrostLight;
                        return cfg;
                    }
                }
            }
        }
        Self::default()
    }

    pub fn save(&self) {
        let Some(path) = Self::config_path() else {
            return;
        };
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let tmp = path.with_extension("json.tmp");
            if fs::write(&tmp, json).is_ok() {
                let _ = fs::rename(&tmp, &path);
            }
        }
    }
}
