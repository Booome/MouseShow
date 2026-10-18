//! Settings: a TOML file for long-lived preferences, CLI flags for one-off runs.

use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct Config {
    pub general: General,
    pub colors: Colors,
    pub ripple: Ripple,
    pub trail: Trail,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct General {
    /// Render rate. 60 needs no explanation; 30 halves the cost on huge screens.
    pub fps: u32,
    /// Multiply every size by this. Handy on 4K where defaults look small.
    pub scale: f32,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct Colors {
    /// `#RRGGBB` or `#RRGGBBAA`.
    pub left: String,
    pub right: String,
    pub middle: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct Ripple {
    pub enabled: bool,
    pub from_radius: f32,
    pub to_radius: f32,
    pub duration_ms: f32,
    pub width: f32,
    /// Soft fill inside the ring, 0.0 = ring only.
    pub fill_alpha: f32,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct Trail {
    pub enabled: bool,
    pub width: f32,
    /// How long the trail lingers after the button is released.
    pub tail_ms: f32,
    /// Ignore movements shorter than this (in px) to avoid jitter.
    pub min_step: f32,
}

impl Default for General {
    fn default() -> Self {
        Self {
            fps: 60,
            scale: 1.0,
        }
    }
}

impl Default for Colors {
    fn default() -> Self {
        Self {
            left: "#FFD600".into(),
            right: "#00C8FF".into(),
            middle: "#FF5C8A".into(),
        }
    }
}

impl Default for Ripple {
    fn default() -> Self {
        Self {
            enabled: true,
            from_radius: 4.5,
            to_radius: 21.0,
            duration_ms: 460.0,
            width: 3.0,
            fill_alpha: 0.10,
        }
    }
}

impl Default for Trail {
    fn default() -> Self {
        Self {
            enabled: true,
            width: 3.2,
            tail_ms: 320.0,
            min_step: 2.0,
        }
    }
}

impl Config {
    pub fn path() -> PathBuf {
        let base = std::env::var("APPDATA").unwrap_or_else(|_| ".".into());
        PathBuf::from(base).join("MouseShow").join("config.toml")
    }

    /// Load from `path`. A missing file is not an error: defaults apply.
    pub fn load(path: &PathBuf) -> Result<Self, String> {
        match fs::read_to_string(path) {
            Ok(text) => toml::from_str(&text).map_err(|e| format!("{}: {e}", path.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(format!("{}: {e}", path.display())),
        }
    }

    pub fn to_toml(&self) -> String {
        toml::to_string_pretty(self).unwrap_or_else(|_| String::new())
    }

    /// Clamp values that would otherwise produce a broken render.
    pub fn sanitize(&mut self) {
        self.general.fps = self.general.fps.clamp(10, 240);
        self.general.scale = self.general.scale.clamp(0.2, 4.0);
        self.ripple.duration_ms = self.ripple.duration_ms.clamp(50.0, 5000.0);
        self.ripple.from_radius = self.ripple.from_radius.max(1.0);
        self.ripple.to_radius = self.ripple.to_radius.max(self.ripple.from_radius + 1.0);
        self.ripple.width = self.ripple.width.clamp(0.5, 40.0);
        self.ripple.fill_alpha = self.ripple.fill_alpha.clamp(0.0, 1.0);
        self.trail.width = self.trail.width.clamp(0.5, 40.0);
        self.trail.tail_ms = self.trail.tail_ms.clamp(0.0, 5000.0);
        self.trail.min_step = self.trail.min_step.clamp(0.0, 50.0);
    }
}
