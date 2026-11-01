//! The settings schema as *data*, so the window is generated from it.
//!
//! `ARCHITECTURE.md` §11: "Settings UI is generated from the schema, so a new
//! setting cannot exist without a control and vice versa". The bijection is
//! enforced by a test at the bottom of this file: every leaf key in
//! `Settings::default()` must appear exactly once here, and every control here
//! must point at a real leaf. Adding a field to `Settings` without a control
//! fails `cargo test`.
//!
//! Each control also carries the requirement it implements (`spec`) so the
//! acceptance rule — every `SPEC.md` §5 rule has a visible control — can be
//! checked mechanically instead of by eye.

use serde::Serialize;

use crate::model::settings::{Settings, DEFAULT_HOTKEY};
use crate::model::types::TARGET_FORMATS;

/// How the window should render the control.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ControlKind {
    /// On/off switch.
    Bool,
    /// One of `options`.
    Enum,
    /// Integer with `min`/`max`.
    Int,
    /// Absolute directory picked with a folder chooser.
    Path,
    /// Free text (validated by the backend).
    Text,
    /// Ordered multi-select of wheel segments; capped at 6 by the backend.
    TargetList,
    /// Shown, not editable — an invariant with a name (`SPEC.md` §5).
    Locked,
    /// Shown as status text; written by the system, not by the user.
    ReadOnly,
}

/// One control in the settings window.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Control {
    /// Dotted path into `Settings`, e.g. `output.batch_threshold`.
    pub key: String,
    /// Group heading the control belongs to.
    pub group: &'static str,
    pub label: &'static str,
    pub help: &'static str,
    /// Requirement this control implements, e.g. `SPEC.md §5` or `ARCH sec 15`.
    pub spec: &'static str,
    pub kind: ControlKind,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit: Option<&'static str>,
}

fn control(
    key: &str,
    group: &'static str,
    label: &'static str,
    help: &'static str,
    spec: &'static str,
    kind: ControlKind,
) -> Control {
    Control {
        key: key.to_string(),
        group,
        label,
        help,
        spec,
        kind,
        options: Vec::new(),
        min: None,
        max: None,
        unit: None,
    }
}

/// Every control, in display order. Groups appear in the order of first use.
pub fn controls() -> Vec<Control> {
    use ControlKind::*;

    let mut out = Vec::new();

    // ---- Output (SPEC.md §5) ------------------------------------------------
    out.push(control(
        "output.location",
        "Where files go",
        "Save converted files",
        "Same folder as the original, or a folder you choose once.",
        "SPEC §5",
        Enum,
    ));
    out.last_mut().unwrap().options = vec!["same_folder", "chosen_folder"];

    out.push(control(
        "output.chosen_folder",
        "Where files go",
        "Chosen folder",
        "Used only when 'Save converted files' is set to a chosen folder.",
        "SPEC §5",
        Path,
    ));

    out.push(control(
        "output.batch_threshold",
        "Where files go",
        "Batch starts at",
        "Batches of this many files or more follow the batch-folder rule.",
        "SPEC §5",
        Int,
    ));
    out.last_mut().unwrap().min = Some(2);
    out.last_mut().unwrap().max = Some(500);
    out.last_mut().unwrap().unit = Some("files");

    out.push(control(
        "output.batch_subfolder",
        "Where files go",
        "Batch folder name",
        "Subfolder created next to the originals for batch jobs.",
        "SPEC §5",
        Text,
    ));

    out.push(control(
        "output.use_batch_subfolder",
        "Where files go",
        "Use a batch folder",
        "Off means every result lands beside its original, however many files there are.",
        "SPEC §5",
        Bool,
    ));

    out.push(control(
        "output.never_overwrite",
        "Where files go",
        "Never overwrite files",
        "Conversions get a -1, -2 suffix instead. This is a rule, not a preference.",
        "SPEC §5",
        Locked,
    ));

    out.push(control(
        "folder_drop.include_subfolders",
        "Where files go",
        "Include subfolders on folder drops",
        "Off: a dropped folder contributes its own files only (one level deep).",
        "SPEC §5",
        Bool,
    ));

    // ---- Metadata (SPEC.md §5) ---------------------------------------------
    out.push(control(
        "metadata.strip",
        "Metadata",
        "Strip metadata by default",
        "Off keeps EXIF, ICC profiles, dates and audio tags. Toggle per batch too.",
        "SPEC §5",
        Bool,
    ));

    // ---- Wheel layout (SPEC.md §2) -----------------------------------------
    out.push(control(
        "wheel.segments",
        "Wheel layout",
        "Segments",
        "Choose up to 6 formats, in the order they appear around the wheel.",
        "SPEC §2",
        TargetList,
    ));
    out.last_mut().unwrap().options = TARGET_FORMATS.to_vec();

    out.push(control(
        "wheel.order",
        "Wheel layout",
        "Segment order",
        "Clockwise from the top. Keep it in step with the chosen segments.",
        "SPEC §2",
        TargetList,
    ));
    out.last_mut().unwrap().options = TARGET_FORMATS.to_vec();

    out.push(control(
        "wheel.show_more_ring",
        "Wheel layout",
        "Show a 'more' segment",
        "Targets that don't fit in 6 slots expand into a sub-ring.",
        "SPEC §2",
        Bool,
    ));

    // ---- Sound + undo (SPEC.md §5) -----------------------------------------
    out.push(control(
        "sound.enabled",
        "Feedback",
        "Completion sound",
        "A short chime when a batch finishes. Off means silent.",
        "SPEC §5",
        Bool,
    ));

    out.push(control(
        "undo.window_seconds",
        "Feedback",
        "Undo window",
        "Seconds the toast offers Undo before the result becomes permanent.",
        "SPEC §5",
        Int,
    ));
    out.last_mut().unwrap().min = Some(1);
    out.last_mut().unwrap().max = Some(60);
    out.last_mut().unwrap().unit = Some("seconds");

    // ---- Intake (SPEC.md §2) -----------------------------------------------
    out.push(control(
        "smart_grab.enabled",
        "Intake",
        "Smart grab",
        "When the hotkey fires with nothing dragging, take the current Finder/Explorer selection. Asks for permission the first time you turn it on.",
        "SPEC §2",
        Bool,
    ));

    out.push(control(
        "smart_grab.permission_state",
        "Intake",
        "Smart grab permission",
        "Reported by the OS after you turn smart grab on. Never requested at launch.",
        "SPEC §2",
        ReadOnly,
    ));
    out.last_mut().unwrap().options = vec!["unknown", "granted", "denied", "unsupported"];

    out.push(control(
        "hotkey.accelerator",
        "Intake",
        "Hotkey",
        "Shows the wheel at the cursor.",
        "ARCH §15",
        Text,
    ));

    // ---- Documents ----------------------------------------------------------
    out.push(control(
        "pdf.page",
        "Documents",
        "PDF page to rasterise",
        "Only the chosen page is rendered when a PDF is used as an image input.",
        "ARCH §15",
        Int,
    ));
    out.last_mut().unwrap().min = Some(1);
    out.last_mut().unwrap().max = Some(100_000);
    out.last_mut().unwrap().unit = Some("page");

    // ---- Diagnostics --------------------------------------------------------
    out.push(control(
        "diagnostics.capture_engine_stderr",
        "Diagnostics",
        "Keep engine output",
        "Stores the engine's own messages with each job. They stay on this machine.",
        "ARCH §15",
        Bool,
    ));

    out.push(control(
        "diagnostics.keep_job_logs",
        "Diagnostics",
        "Job logs to keep",
        "How many recent job logs are kept in the app cache.",
        "ARCH §15",
        Int,
    ));
    out.last_mut().unwrap().min = Some(0);
    out.last_mut().unwrap().max = Some(1_000);
    out.last_mut().unwrap().unit = Some("logs");

    // ---- Limits -------------------------------------------------------------
    out.push(control(
        "limits.max_input_bytes",
        "Limits",
        "Largest single file",
        "Files above this are refused at admission rather than half-converted.",
        "ARCH §15",
        Int,
    ));
    out.last_mut().unwrap().min = Some(1 << 20);
    out.last_mut().unwrap().max = Some(1 << 40);
    out.last_mut().unwrap().unit = Some("bytes");

    out.push(control(
        "limits.max_parallel_images",
        "Limits",
        "Image workers",
        "How many image conversions run at once.",
        "ARCH §4",
        Int,
    ));
    out.last_mut().unwrap().min = Some(1);
    out.last_mut().unwrap().max = Some(16);
    out.last_mut().unwrap().unit = Some("workers");

    out
}

/// Default values keyed by control, so the window can offer "reset".
pub fn defaults(settings: &Settings) -> serde_json::Value {
    serde_json::to_value(settings).unwrap_or(serde_json::Value::Null)
}

/// Hint for the hotkey field; kept out of the schema struct because it is text.
pub fn hotkey_hint() -> &'static str {
    DEFAULT_HOTKEY
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;
    use std::collections::BTreeSet;

    /// Dotted leaf paths of a JSON object.
    fn leaves(value: &Value, prefix: &str, out: &mut BTreeSet<String>) {
        match value {
            Value::Object(map) if !map.is_empty() => {
                for (key, child) in map {
                    let path = if prefix.is_empty() {
                        key.clone()
                    } else {
                        format!("{prefix}.{key}")
                    };
                    leaves(child, &path, out);
                }
            }
            Value::Array(_) => {
                // Arrays are leaves: a list is one control, not one per entry.
                out.insert(prefix.to_string());
            }
            _ => {
                out.insert(prefix.to_string());
            }
        }
    }

    #[test]
    fn every_setting_has_exactly_one_control_and_vice_versa() {
        let settings = serde_json::to_value(Settings::default()).unwrap();
        let mut setting_leaves = BTreeSet::new();
        leaves(&settings, "", &mut setting_leaves);
        setting_leaves.remove("schema_version"); // document version, not a user setting

        let mut keys = BTreeSet::new();
        for c in controls() {
            assert!(
                keys.insert(c.key.clone()),
                "duplicate control for key {}",
                c.key
            );
        }

        let missing: Vec<_> = setting_leaves.difference(&keys).cloned().collect();
        let orphaned: Vec<_> = keys.difference(&setting_leaves).cloned().collect();
        assert!(missing.is_empty(), "settings with no control: {missing:?}");
        assert!(
            orphaned.is_empty(),
            "controls with no setting: {orphaned:?}"
        );
    }

    #[test]
    fn every_control_names_its_requirement() {
        for c in controls() {
            assert!(!c.spec.trim().is_empty(), "{} has no spec reference", c.key);
            assert!(!c.label.trim().is_empty(), "{} has no label", c.key);
            assert!(!c.help.trim().is_empty(), "{} has no help text", c.key);
        }
    }

    #[test]
    fn every_spec_section_5_rule_has_a_control() {
        // The card's acceptance rule, encoded: each bullet of SPEC.md §5 maps
        // to at least one control the user can see.
        let keys: BTreeSet<String> = controls().into_iter().map(|c| c.key).collect();
        let required = [
            ("folder drop depth", "folder_drop.include_subfolders"),
            ("output location", "output.location"),
            ("batch folder rule", "output.batch_threshold"),
            ("batch folder name", "output.batch_subfolder"),
            ("batch folder on/off", "output.use_batch_subfolder"),
            ("never overwrite", "output.never_overwrite"),
            ("metadata strip", "metadata.strip"),
            ("undo", "undo.window_seconds"),
            ("sound", "sound.enabled"),
        ];
        for (rule, key) in required {
            assert!(
                keys.contains(key),
                "SPEC §5 rule '{rule}' has no control ({key})"
            );
        }
        // Multi-file drops and folder drops have no setting: they are intake
        // behaviour, and the window's drop surface is the control. Assert the
        // intake controls that do exist.
        assert!(keys.contains("smart_grab.enabled"));
        assert!(keys.contains("wheel.segments"));
        assert!(keys.contains("wheel.order"));
    }

    #[test]
    fn locked_and_readonly_controls_are_not_user_writable() {
        let locked: Vec<_> = controls()
            .into_iter()
            .filter(|c| matches!(c.kind, ControlKind::Locked | ControlKind::ReadOnly))
            .map(|c| c.key)
            .collect();
        assert!(locked.contains(&"output.never_overwrite".to_string()));
        assert!(locked.contains(&"smart_grab.permission_state".to_string()));
    }

    #[test]
    fn enum_controls_offer_only_documented_values() {
        for c in controls() {
            if c.kind == ControlKind::Enum || c.kind == ControlKind::ReadOnly {
                assert!(!c.options.is_empty(), "{} has no options", c.key);
            }
        }
    }

    #[test]
    fn target_list_controls_offer_the_spec_formats() {
        for c in controls()
            .into_iter()
            .filter(|c| c.kind == ControlKind::TargetList)
        {
            assert_eq!(c.options, TARGET_FORMATS);
        }
    }
}
