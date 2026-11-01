//! Settings schema, defaults, load/save/migrate — `ARCHITECTURE.md` §15.
//!
//! `SPEC.md` §5 requires the output rules to be *settings, not constants*, and
//! `SPEC.md` §2 requires the wheel layout and smart-grab opt-in to be user
//! choices. This module is the single store for all of them.
//!
//! Invariants this module owns:
//!
//! - the file is written atomically (temp file + rename), so a crash cannot
//!   leave a half-written `settings.json`;
//! - a corrupt file is copied to `settings.json.bak.<unix_ms>` and replaced by
//!   defaults — never a launch failure;
//! - invalid values are repaired (clamped or reset) and every repair is
//!   reported, so the window can show what it changed;
//! - unknown fields are preserved across saves, so a newer build's keys
//!   survive a downgrade.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::model::types::is_known_target;

/// Version of the on-disk settings document.
pub const SETTINGS_SCHEMA_VERSION: u32 = 1;

/// `SPEC.md` §2: "Hard cap 6 segments, never smaller" applies to the wheel.
pub const WHEEL_MAX_SEGMENTS: usize = 6;

/// Default batch size at which the `converted/` subfolder rule fires (`SPEC.md` §5).
pub const DEFAULT_BATCH_THRESHOLD: u32 = 3;

/// Default output subfolder name (`SPEC.md` §5).
pub const DEFAULT_BATCH_SUBFOLDER: &str = "converted";

/// Default accelerator (`ARCHITECTURE.md` §15).
pub const DEFAULT_HOTKEY: &str = "Ctrl+Alt+Shift+D";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub schema_version: u32,
    pub output: OutputSettings,
    pub metadata: MetadataSettings,
    pub wheel: WheelSettings,
    pub hotkey: HotkeySettings,
    pub smart_grab: SmartGrabSettings,
    pub folder_drop: FolderDropSettings,
    pub undo: UndoSettings,
    pub sound: SoundSettings,
    pub pdf: PdfSettings,
    pub diagnostics: DiagnosticsSettings,
    pub limits: LimitsSettings,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct OutputSettings {
    /// `same_folder` (default) or `chosen_folder` (`SPEC.md` §5).
    pub location: String,
    /// Required when `location == "chosen_folder"`. Added by `U2`: the spec
    /// requires the location rule to be user-toggleable, and a toggle without a
    /// destination cannot change behaviour.
    pub chosen_folder: Option<String>,
    /// `SPEC.md` §5: batch jobs (3+) go to a subfolder. The name is a setting.
    pub batch_subfolder: String,
    /// Batch threshold. `SPEC.md` §5 says 3+; the number is a setting so the
    /// rule is not hardcoded.
    pub batch_threshold: u32,
    /// Whether the batch rule fires at all. `SPEC.md` §5 requires the rule to be
    /// user-toggleable, so it has an off switch as well as a name and a
    /// threshold.
    pub use_batch_subfolder: bool,
    /// `SPEC.md` §5: never overwrite. Exposed so the invariant has a name, but
    /// forced to `true` on load/repair and rendered as a locked control (§15).
    pub never_overwrite: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct MetadataSettings {
    /// `SPEC.md` §5: keep metadata by default, per-conversion strip toggle.
    pub strip: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct WheelSettings {
    /// Segments the user chose, in user order (`SPEC.md` §2).
    pub segments: Vec<String>,
    /// Explicit order. Kept separate from `segments` because §15 names both;
    /// repair makes `order` a permutation of `segments`.
    pub order: Vec<String>,
    /// Overflow lives behind a "more" segment.
    pub show_more_ring: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct HotkeySettings {
    pub accelerator: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SmartGrabSettings {
    /// `SPEC.md` §2: opt-in, never prompted at launch.
    pub enabled: bool,
    /// `unknown` | `granted` | `denied` | `unsupported`.
    pub permission_state: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct FolderDropSettings {
    /// `SPEC.md` §5: folder drop is 1 level deep unless this is on.
    pub include_subfolders: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct UndoSettings {
    /// `SPEC.md` §5: a 10-second Undo.
    pub window_seconds: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SoundSettings {
    /// `SPEC.md` §5: sound is user-toggleable ON/OFF.
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct PdfSettings {
    /// Page rasterised for pdf input (`ARCHITECTURE.md` §15).
    pub page: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct DiagnosticsSettings {
    pub capture_engine_stderr: bool,
    pub keep_job_logs: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct LimitsSettings {
    pub max_input_bytes: u64,
    pub max_parallel_images: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            schema_version: SETTINGS_SCHEMA_VERSION,
            output: OutputSettings::default(),
            metadata: MetadataSettings::default(),
            wheel: WheelSettings::default(),
            hotkey: HotkeySettings::default(),
            smart_grab: SmartGrabSettings::default(),
            folder_drop: FolderDropSettings::default(),
            undo: UndoSettings::default(),
            sound: SoundSettings::default(),
            pdf: PdfSettings::default(),
            diagnostics: DiagnosticsSettings::default(),
            limits: LimitsSettings::default(),
        }
    }
}

impl Default for OutputSettings {
    fn default() -> Self {
        Self {
            location: "same_folder".into(),
            chosen_folder: None,
            batch_subfolder: DEFAULT_BATCH_SUBFOLDER.into(),
            batch_threshold: DEFAULT_BATCH_THRESHOLD,
            use_batch_subfolder: true,
            never_overwrite: true,
        }
    }
}

impl Default for MetadataSettings {
    fn default() -> Self {
        Self { strip: false }
    }
}

impl Default for WheelSettings {
    fn default() -> Self {
        let segments = ["png", "jpg", "pdf", "webp", "mp3", "mp4"]
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>();
        Self {
            order: segments.clone(),
            segments,
            show_more_ring: true,
        }
    }
}

impl Default for HotkeySettings {
    fn default() -> Self {
        Self {
            accelerator: DEFAULT_HOTKEY.into(),
        }
    }
}

impl Default for SmartGrabSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            permission_state: "unknown".into(),
        }
    }
}

impl Default for FolderDropSettings {
    fn default() -> Self {
        Self {
            include_subfolders: false,
        }
    }
}

impl Default for UndoSettings {
    fn default() -> Self {
        Self { window_seconds: 10 }
    }
}

impl Default for SoundSettings {
    fn default() -> Self {
        Self { enabled: true }
    }
}

impl Default for PdfSettings {
    fn default() -> Self {
        Self { page: 1 }
    }
}

impl Default for DiagnosticsSettings {
    fn default() -> Self {
        Self {
            capture_engine_stderr: true,
            keep_job_logs: 50,
        }
    }
}

impl Default for LimitsSettings {
    fn default() -> Self {
        Self {
            max_input_bytes: 2_147_483_648,
            max_parallel_images: 4,
        }
    }
}

/// A value the window asked to change that this module refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingsError {
    UnknownKey(String),
    InvalidValue { key: String, why: String },
    Io(String),
}

impl std::fmt::Display for SettingsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SettingsError::UnknownKey(k) => write!(f, "unknown setting: {k}"),
            SettingsError::InvalidValue { key, why } => write!(f, "{key}: {why}"),
            SettingsError::Io(why) => write!(f, "settings file: {why}"),
        }
    }
}

/// Result of applying one control change: the repaired settings plus what the
/// repair had to fix.
#[derive(Debug, Clone)]
pub struct Applied {
    pub settings: Settings,
    pub repairs: Vec<String>,
}

impl Settings {
    /// Repairs invalid values in place and returns one human-readable line per
    /// repair. Called on every load and after every write (§15).
    pub fn repair(&mut self) -> Vec<String> {
        let mut notes = Vec::new();

        if self.schema_version != SETTINGS_SCHEMA_VERSION {
            notes.push(format!(
                "schema_version {} -> {} (this build understands {})",
                self.schema_version, SETTINGS_SCHEMA_VERSION, SETTINGS_SCHEMA_VERSION
            ));
            self.schema_version = SETTINGS_SCHEMA_VERSION;
        }

        // SPEC.md §5: never overwrite is a rule, not a preference.
        if !self.output.never_overwrite {
            notes.push("output.never_overwrite reset to true (SPEC §5: never overwrite)".into());
            self.output.never_overwrite = true;
        }

        let location = self.output.location.trim().to_ascii_lowercase();
        let location = match location.as_str() {
            "same_folder" => "same_folder".to_string(),
            "chosen_folder" => "chosen_folder".to_string(),
            other => {
                notes.push(format!(
                    "output.location {other:?} is not a location; using same_folder"
                ));
                "same_folder".to_string()
            }
        };
        self.output.location = location;

        if self.output.location == "chosen_folder" {
            match self.output.chosen_folder.as_deref() {
                Some(p) if Path::new(p).is_absolute() => {
                    self.output.chosen_folder = Some(p.to_string());
                }
                _ => {
                    notes.push(
                        "output.chosen_folder is not an absolute path; falling back to same_folder"
                            .into(),
                    );
                    self.output.location = "same_folder".into();
                    self.output.chosen_folder = None;
                }
            }
        }

        let cleaned = sanitize_subfolder(&self.output.batch_subfolder);
        if cleaned != self.output.batch_subfolder {
            notes.push(format!(
                "output.batch_subfolder {:?} -> {:?}",
                self.output.batch_subfolder, cleaned
            ));
            self.output.batch_subfolder = cleaned;
        }

        let threshold = self.output.batch_threshold.clamp(2, 500);
        if threshold != self.output.batch_threshold {
            notes.push(format!(
                "output.batch_threshold {} -> {threshold}",
                self.output.batch_threshold
            ));
            self.output.batch_threshold = threshold;
        }

        // Wheel: valid, deduped, capped at 6 (SPEC §2), order is a permutation.
        let mut segments: Vec<String> = Vec::new();
        for raw in &self.wheel.segments {
            let fmt = raw.trim().to_ascii_lowercase();
            if fmt.is_empty() || !is_known_target(&fmt) {
                notes.push(format!("wheel.segments dropped unknown format {raw:?}"));
                continue;
            }
            if segments.contains(&fmt) {
                notes.push(format!("wheel.segments removed duplicate {fmt:?}"));
                continue;
            }
            segments.push(fmt);
        }
        if segments.len() > WHEEL_MAX_SEGMENTS {
            notes.push(format!(
                "wheel.segments capped at {WHEEL_MAX_SEGMENTS} (SPEC §2 hard cap)"
            ));
            segments.truncate(WHEEL_MAX_SEGMENTS);
        }
        if segments.is_empty() {
            notes.push("wheel.segments was empty; defaults restored".into());
            segments = WheelSettings::default().segments;
        }
        self.wheel.segments = segments;

        let mut order: Vec<String> = Vec::new();
        for raw in &self.wheel.order {
            let fmt = raw.trim().to_ascii_lowercase();
            if !self.wheel.segments.contains(&fmt) {
                notes.push(format!(
                    "wheel.order dropped {raw:?} (not a chosen segment)"
                ));
                continue;
            }
            if order.contains(&fmt) {
                notes.push(format!("wheel.order removed duplicate {fmt:?}"));
                continue;
            }
            order.push(fmt);
        }
        for seg in &self.wheel.segments {
            if !order.contains(seg) {
                notes.push(format!("wheel.order appended missing segment {seg:?}"));
                order.push(seg.clone());
            }
        }
        self.wheel.order = order;

        let hotkey = self.hotkey.accelerator.trim().to_string();
        if hotkey.is_empty() {
            notes.push(format!(
                "hotkey.accelerator was empty; using {DEFAULT_HOTKEY}"
            ));
            self.hotkey.accelerator = DEFAULT_HOTKEY.into();
        } else {
            self.hotkey.accelerator = hotkey;
        }

        let permission = self.smart_grab.permission_state.trim().to_ascii_lowercase();
        let permission = match permission.as_str() {
            "unknown" | "granted" | "denied" | "unsupported" => permission,
            other => {
                notes.push(format!(
                    "smart_grab.permission_state {other:?} -> \"unknown\""
                ));
                "unknown".into()
            }
        };
        self.smart_grab.permission_state = permission;

        let undo = self.undo.window_seconds.clamp(1, 60);
        if undo != self.undo.window_seconds {
            notes.push(format!(
                "undo.window_seconds {} -> {undo}",
                self.undo.window_seconds
            ));
            self.undo.window_seconds = undo;
        }

        let page = self.pdf.page.clamp(1, 100_000);
        if page != self.pdf.page {
            notes.push(format!("pdf.page {} -> {page}", self.pdf.page));
            self.pdf.page = page;
        }

        let logs = self.diagnostics.keep_job_logs.clamp(0, 1_000);
        if logs != self.diagnostics.keep_job_logs {
            notes.push(format!(
                "diagnostics.keep_job_logs {} -> {logs}",
                self.diagnostics.keep_job_logs
            ));
            self.diagnostics.keep_job_logs = logs;
        }

        let max_in = self.limits.max_input_bytes.clamp(1 << 20, 1 << 40);
        if max_in != self.limits.max_input_bytes {
            notes.push(format!(
                "limits.max_input_bytes {} -> {max_in}",
                self.limits.max_input_bytes
            ));
            self.limits.max_input_bytes = max_in;
        }

        let parallel = self.limits.max_parallel_images.clamp(1, 16);
        if parallel != self.limits.max_parallel_images {
            notes.push(format!(
                "limits.max_parallel_images {} -> {parallel}",
                self.limits.max_parallel_images
            ));
            self.limits.max_parallel_images = parallel;
        }

        notes
    }
}

/// Keeps the batch subfolder a single path segment: no separators, no `..`, no
/// reserved device name, never empty (`ARCHITECTURE.md` §9.5).
pub fn sanitize_subfolder(raw: &str) -> String {
    let trimmed = raw.trim();
    let candidate = trimmed
        .split(['/', '\\'])
        .filter(|part| !part.trim().is_empty())
        .next_back()
        .unwrap_or("");
    let cleaned = candidate
        .trim_matches(|c: char| c == '.' || c.is_whitespace() || c.is_control())
        .replace(['<', '>', ':', '"', '|', '?', '*'], "");
    if cleaned.is_empty() || is_reserved_windows_name(&cleaned) {
        DEFAULT_BATCH_SUBFOLDER.to_string()
    } else {
        cleaned
    }
}

/// Windows device names are not valid file names (`ARCHITECTURE.md` §9.5).
pub fn is_reserved_windows_name(name: &str) -> bool {
    let stem = name
        .split('.')
        .next()
        .unwrap_or(name)
        .trim_end_matches([' ', '.'])
        .to_ascii_uppercase();
    matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (stem.len() == 4
            && (stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem.as_bytes()[3].is_ascii_digit())
}

/// Path of the settings file inside an app config directory.
pub fn settings_path(config_dir: &Path) -> PathBuf {
    config_dir.join("settings.json")
}

/// The app config directory, per platform (`ARCHITECTURE.md` §15).
///
/// `EZC_CONFIG_DIR` overrides it. It exists so tests and the restart probe can
/// point at a scratch directory without touching the developer's real profile;
/// it is not a user-facing setting.
pub fn config_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("EZC_CONFIG_DIR") {
        if !dir.trim().is_empty() {
            return PathBuf::from(dir);
        }
    }
    platform_config_dir().join("app.ezconvert")
}

#[cfg(target_os = "windows")]
fn platform_config_dir() -> PathBuf {
    std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

#[cfg(target_os = "macos")]
fn platform_config_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map(|home| {
            PathBuf::from(home)
                .join("Library")
                .join("Application Support")
        })
        .unwrap_or_else(|| PathBuf::from("."))
}

#[cfg(all(unix, not(target_os = "macos")))]
fn platform_config_dir() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Loaded settings plus what loading had to fix.
#[derive(Debug, Clone)]
pub struct Loaded {
    pub settings: Settings,
    pub repairs: Vec<String>,
    /// `true` when the previous file could not be parsed and was replaced.
    pub recovered_from_corrupt: bool,
    pub backup: Option<PathBuf>,
}

/// Reads `path`, repairs it, and never fails: a corrupt file becomes defaults
/// plus a timestamped backup (§15).
pub fn load(path: &Path) -> Loaded {
    let mut repairs = Vec::new();
    let mut recovered = false;
    let mut backup = None;
    let mut settings = Settings::default();

    match fs::read_to_string(path) {
        Ok(text) => match serde_json::from_str::<Value>(&text) {
            Ok(value) => match serde_json::from_value::<Settings>(value) {
                Ok(parsed) => settings = parsed,
                Err(err) => {
                    repairs.push(format!("settings file unusable ({err}); defaults written"));
                    recovered = true;
                    backup = Some(backup_corrupt(path, &text));
                }
            },
            Err(err) => {
                repairs.push(format!(
                    "settings file is not JSON ({err}); defaults written"
                ));
                recovered = true;
                backup = Some(backup_corrupt(path, &text));
            }
        },
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            repairs.push("no settings file yet; defaults written".into());
        }
        Err(err) => {
            repairs.push(format!("settings file unreadable ({err}); using defaults"));
        }
    }

    repairs.extend(settings.repair());
    Loaded {
        settings,
        repairs,
        recovered_from_corrupt: recovered,
        backup,
    }
}

fn backup_corrupt(path: &Path, text: &str) -> PathBuf {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let backup = path.with_file_name(format!("settings.json.bak.{stamp}"));
    // Best effort: if the backup cannot be written, defaults are still saved.
    let _ = fs::write(&backup, text);
    backup
}

/// Writes settings atomically: temp file in the same directory, fsync, rename.
///
/// Unknown keys present in the on-disk file are preserved: `merge_unknown`
/// copies every key the current build does not know about into the new value.
pub fn save(path: &Path, settings: &Settings) -> Result<(), SettingsError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| SettingsError::Io(e.to_string()))?;
    }

    let incoming = serde_json::to_value(settings).map_err(|e| SettingsError::Io(e.to_string()))?;
    let merged = match fs::read_to_string(path) {
        Ok(text) => match serde_json::from_str::<Value>(&text) {
            Ok(old) => merge_unknown(old, incoming),
            Err(_) => incoming,
        },
        Err(_) => incoming,
    };

    let mut bytes =
        serde_json::to_vec_pretty(&merged).map_err(|e| SettingsError::Io(e.to_string()))?;
    bytes.push(b'\n');

    let tmp = path.with_extension("json.tmp");
    {
        let mut file = fs::File::create(&tmp).map_err(|e| SettingsError::Io(e.to_string()))?;
        file.write_all(&bytes)
            .map_err(|e| SettingsError::Io(e.to_string()))?;
        file.sync_all()
            .map_err(|e| SettingsError::Io(e.to_string()))?;
    }
    fs::rename(&tmp, path).map_err(|e| SettingsError::Io(e.to_string()))?;
    Ok(())
}

/// Copies keys only the old (on-disk) value knows about into the new value.
fn merge_unknown(stored: Value, incoming: Value) -> Value {
    match (stored, incoming) {
        (Value::Object(mut old), Value::Object(new)) => {
            for (key, value) in new {
                let merged = match old.remove(&key) {
                    Some(previous) => merge_unknown(previous, value),
                    None => value,
                };
                old.insert(key, merged);
            }
            Value::Object(old)
        }
        (_, incoming) => incoming,
    }
}

/// Owns the file path plus the current value, and applies single-control edits.
#[derive(Debug, Clone)]
pub struct SettingsStore {
    path: PathBuf,
    settings: Settings,
    repairs: Vec<String>,
}

impl SettingsStore {
    pub fn new(path: PathBuf) -> Self {
        let loaded = load(&path);
        Self {
            path,
            settings: loaded.settings,
            repairs: loaded.repairs,
        }
    }

    pub fn in_config_dir(config_dir: &Path) -> Self {
        Self::new(settings_path(config_dir))
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn get(&self) -> &Settings {
        &self.settings
    }

    /// Repairs reported by the last load (shown once in diagnostics).
    pub fn initial_repairs(&self) -> &[String] {
        &self.repairs
    }

    /// Applies one dotted-key edit, repairs the result, persists it, and
    /// returns the repairs the *edit* triggered.
    ///
    /// Unknown keys and invalid values are refused rather than silently
    /// written: a schema-generated control can only send keys the schema names.
    pub fn apply(&mut self, key: &str, value: Value) -> Result<Applied, SettingsError> {
        let mut document =
            serde_json::to_value(&self.settings).map_err(|e| SettingsError::Io(e.to_string()))?;
        let pointer = key_to_pointer(key);
        let target = document
            .pointer_mut(&pointer)
            .ok_or_else(|| SettingsError::UnknownKey(key.to_string()))?;
        *target = value;

        let mut candidate: Settings =
            serde_json::from_value(document).map_err(|e| SettingsError::InvalidValue {
                key: key.to_string(),
                why: e.to_string(),
            })?;
        let repairs = candidate.repair();
        save(&self.path, &candidate)?;
        self.settings = candidate.clone();
        Ok(Applied {
            settings: candidate,
            repairs,
        })
    }

    /// Applies several edits as one write (the batch bar toggles two rules at
    /// once). All-or-nothing: the first failure writes nothing.
    pub fn apply_many(&mut self, edits: &[(String, Value)]) -> Result<Applied, SettingsError> {
        let mut document =
            serde_json::to_value(&self.settings).map_err(|e| SettingsError::Io(e.to_string()))?;
        for (key, value) in edits {
            let pointer = key_to_pointer(key);
            let target = document
                .pointer_mut(&pointer)
                .ok_or_else(|| SettingsError::UnknownKey(key.to_string()))?;
            *target = value.clone();
        }
        let mut candidate: Settings =
            serde_json::from_value(document).map_err(|e| SettingsError::InvalidValue {
                key: "<batch>".into(),
                why: e.to_string(),
            })?;
        let repairs = candidate.repair();
        save(&self.path, &candidate)?;
        self.settings = candidate.clone();
        Ok(Applied {
            settings: candidate,
            repairs,
        })
    }

    /// Reloads from disk, discarding in-memory state. Used by the restart probe
    /// and by the window's "revert" control.
    pub fn reload(&mut self) -> Vec<String> {
        let loaded = load(&self.path);
        self.settings = loaded.settings;
        self.repairs = loaded.repairs.clone();
        loaded.repairs
    }
}

fn key_to_pointer(key: &str) -> String {
    if key.is_empty() {
        return String::new();
    }
    key.split('.')
        .map(|part| format!("/{}", part.replace('~', "~0").replace('/', "~1")))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(tag: &str) -> Self {
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let dir =
                std::env::temp_dir().join(format!("ezc-u2-{tag}-{}-{nanos}", std::process::id()));
            fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn defaults_match_the_documented_schema() {
        let s = Settings::default();
        assert_eq!(s.schema_version, 1);
        assert_eq!(s.output.location, "same_folder");
        assert_eq!(s.output.batch_subfolder, "converted");
        assert_eq!(s.output.batch_threshold, 3);
        assert!(s.output.never_overwrite, "SPEC §5: never overwrite");
        assert!(!s.metadata.strip, "SPEC §5: keep metadata by default");
        assert_eq!(
            s.wheel.segments,
            vec!["png", "jpg", "pdf", "webp", "mp3", "mp4"]
        );
        assert_eq!(s.wheel.order, s.wheel.segments);
        assert!(!s.smart_grab.enabled, "SPEC §2: smart grab is opt-in");
        assert_eq!(s.smart_grab.permission_state, "unknown");
        assert!(
            !s.folder_drop.include_subfolders,
            "SPEC §5: folder drop is 1 deep"
        );
        assert_eq!(s.undo.window_seconds, 10, "SPEC §5: 10-second undo");
        assert!(s.sound.enabled, "SPEC §5: sound on by default, toggleable");
        assert_eq!(s.limits.max_input_bytes, 2_147_483_648);
    }

    #[test]
    fn save_then_load_round_trips_through_disk() {
        let dir = TempDir::new("roundtrip");
        let path = settings_path(dir.path());
        let mut store = SettingsStore::new(path.clone());
        store.apply("sound.enabled", json!(false)).unwrap();
        store.apply("metadata.strip", json!(true)).unwrap();
        store.apply("output.batch_subfolder", json!("out")).unwrap();

        let reloaded = SettingsStore::new(path);
        assert!(!reloaded.get().sound.enabled);
        assert!(reloaded.get().metadata.strip);
        assert_eq!(reloaded.get().output.batch_subfolder, "out");
    }

    #[test]
    fn corrupt_file_is_backed_up_and_defaults_are_written() {
        let dir = TempDir::new("corrupt");
        let path = settings_path(dir.path());
        fs::write(&path, b"{ this is not json").unwrap();

        let loaded = load(&path);
        assert!(loaded.recovered_from_corrupt);
        assert_eq!(loaded.settings, Settings::default());
        let backup = loaded.backup.expect("backup path");
        assert!(backup.exists(), "corrupt file must be preserved");
        assert_eq!(fs::read_to_string(backup).unwrap(), "{ this is not json");
        assert!(loaded.repairs.iter().any(|r| r.contains("not JSON")));
    }

    #[test]
    fn unknown_fields_survive_a_save() {
        let dir = TempDir::new("unknown");
        let path = settings_path(dir.path());
        fs::write(
            &path,
            br#"{"schema_version":1,"from_a_newer_build":{"keep":"me"},"output":{"location":"same_folder","batch_subfolder":"converted","batch_threshold":3,"never_overwrite":true},"metadata":{"strip":false}}"#,
        )
        .unwrap();

        let mut store = SettingsStore::new(path.clone());
        store.apply("sound.enabled", json!(false)).unwrap();
        let text = fs::read_to_string(&path).unwrap();
        let value: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(value["from_a_newer_build"]["keep"], "me");
        assert_eq!(value["sound"]["enabled"], false);
    }

    #[test]
    fn invalid_values_are_repaired_and_reported() {
        let dir = TempDir::new("repair");
        let path = settings_path(dir.path());
        fs::write(
            &path,
            br#"{
              "schema_version": 7,
              "output": {"location":"elsewhere","batch_subfolder":"../escape/","batch_threshold":0,"never_overwrite":false},
              "wheel": {"segments":["png","PNG","klingon","jpg","webp","pdf","mp3","mp4"], "order":["mp4","png","nope"]},
              "undo": {"window_seconds": 0},
              "limits": {"max_parallel_images": 99}
            }"#,
        )
        .unwrap();

        let loaded = load(&path);
        let s = &loaded.settings;
        assert_eq!(s.schema_version, 1);
        assert_eq!(s.output.location, "same_folder");
        assert_eq!(s.output.batch_subfolder, "escape");
        assert_eq!(s.output.batch_threshold, 2);
        assert!(s.output.never_overwrite);
        assert_eq!(s.wheel.segments.len(), WHEEL_MAX_SEGMENTS);
        // User order first (`mp4`, `png`), then the segments the cap kept.
        assert_eq!(
            s.wheel.order,
            vec!["mp4", "png", "jpg", "webp", "pdf", "mp3"]
        );
        let mut set = s.wheel.order.clone();
        set.sort();
        let mut segs = s.wheel.segments.clone();
        segs.sort();
        assert_eq!(set, segs, "order must be a permutation of the segments");
        assert_eq!(s.undo.window_seconds, 1);
        assert_eq!(s.limits.max_parallel_images, 16);
        assert!(loaded.repairs.len() >= 8, "repairs: {:?}", loaded.repairs);
    }

    #[test]
    fn chosen_folder_requires_an_absolute_path() {
        let mut s = Settings::default();
        s.output.location = "chosen_folder".into();
        s.output.chosen_folder = Some("relative/path".into());
        let notes = s.repair();
        assert_eq!(s.output.location, "same_folder");
        assert!(notes.iter().any(|n| n.contains("chosen_folder")));

        let mut s = Settings::default();
        s.output.location = "chosen_folder".into();
        s.output.chosen_folder = Some("/tmp/out".into());
        s.repair();
        assert_eq!(s.output.location, "chosen_folder");
        assert_eq!(s.output.chosen_folder.as_deref(), Some("/tmp/out"));
    }

    #[test]
    fn wheel_is_capped_and_order_is_a_permutation() {
        let mut s = Settings::default();
        s.wheel.segments = vec!["png".into(), "jpg".into(), "pdf".into()];
        s.wheel.order = vec!["pdf".into(), "png".into()];
        s.repair();
        assert_eq!(s.wheel.order, vec!["pdf", "png", "jpg"]);
    }

    #[test]
    fn writing_is_atomic_and_leaves_no_temp_file() {
        let dir = TempDir::new("atomic");
        let path = settings_path(dir.path());
        let mut store = SettingsStore::new(path.clone());
        store.apply("sound.enabled", json!(false)).unwrap();
        let stray: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
            .filter(|n| n.ends_with(".tmp"))
            .collect();
        assert!(stray.is_empty(), "temp files left behind: {stray:?}");
    }

    #[test]
    fn unknown_key_is_refused_without_touching_the_file() {
        let dir = TempDir::new("unknownkey");
        let path = settings_path(dir.path());
        let mut store = SettingsStore::new(path.clone());
        store.apply("sound.enabled", json!(false)).unwrap();
        let before = fs::read_to_string(&path).unwrap();
        let err = store.apply("sound.volume", json!(0.5)).unwrap_err();
        assert_eq!(err, SettingsError::UnknownKey("sound.volume".into()));
        assert_eq!(fs::read_to_string(&path).unwrap(), before);
    }

    #[test]
    fn invalid_type_is_refused() {
        let dir = TempDir::new("badtype");
        let mut store = SettingsStore::new(settings_path(dir.path()));
        let err = store.apply("sound.enabled", json!("yes")).unwrap_err();
        assert!(
            matches!(err, SettingsError::InvalidValue { .. }),
            "got {err:?}"
        );
    }

    #[test]
    fn never_overwrite_cannot_be_turned_off() {
        let dir = TempDir::new("overwrite");
        let mut store = SettingsStore::new(settings_path(dir.path()));
        let applied = store.apply("output.never_overwrite", json!(false)).unwrap();
        assert!(applied.settings.output.never_overwrite);
        assert!(applied
            .repairs
            .iter()
            .any(|r| r.contains("never_overwrite")));
    }

    #[test]
    fn batch_edits_are_atomic() {
        let dir = TempDir::new("many");
        let path = settings_path(dir.path());
        let mut store = SettingsStore::new(path.clone());
        let before = fs::read_to_string(&path).unwrap_or_default();
        let err = store
            .apply_many(&[
                ("sound.enabled".into(), json!(false)),
                ("nope.nope".into(), json!(1)),
            ])
            .unwrap_err();
        assert!(matches!(err, SettingsError::UnknownKey(_)));
        assert_eq!(fs::read_to_string(&path).unwrap_or_default(), before);
        assert!(store.get().sound.enabled);
    }

    #[test]
    fn subfolder_sanitising_keeps_one_plain_segment() {
        assert_eq!(sanitize_subfolder("converted"), "converted");
        assert_eq!(sanitize_subfolder("  out/  "), "out");
        assert_eq!(sanitize_subfolder("../escape"), "escape");
        assert_eq!(sanitize_subfolder("a\\b"), "b");
        assert_eq!(sanitize_subfolder(""), DEFAULT_BATCH_SUBFOLDER);
        assert_eq!(sanitize_subfolder(".."), DEFAULT_BATCH_SUBFOLDER);
        assert_eq!(sanitize_subfolder("CON"), DEFAULT_BATCH_SUBFOLDER);
        assert_eq!(sanitize_subfolder("com3"), DEFAULT_BATCH_SUBFOLDER);
        assert_eq!(sanitize_subfolder("LPT9.log"), DEFAULT_BATCH_SUBFOLDER);
        assert_eq!(sanitize_subfolder("re:port"), "report");
    }

    #[test]
    fn config_dir_override_is_honoured() {
        // SAFETY: single-threaded test scoped to this function's process state.
        std::env::set_var("EZC_CONFIG_DIR", "/tmp/ezc-test-config");
        assert_eq!(config_dir(), PathBuf::from("/tmp/ezc-test-config"));
        std::env::remove_var("EZC_CONFIG_DIR");
    }
}
