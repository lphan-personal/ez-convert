//! Smart grab: the opt-in "take the current Finder/Explorer selection when the hotkey
//! fires with nothing dragging" path (`SPEC.md` §2).
//!
//! The rules that matter are about **consent**, not about reading a selection:
//!
//! 1. `smart_grab.enabled` defaults to `false` (`ARCHITECTURE.md` §15).
//! 2. The permission prompt is caused by the user turning the toggle **on** — never by
//!    launch, never by the hotkey, never by a settings reload.
//! 3. Turning it **off** touches no permission API at all. That is the one property
//!    that is easy to get wrong (an "un-probe on disable" would ask the system
//!    something on the way out), so it is modelled as a call count and tested.
//!
//! The OS half is behind two traits (`PermissionProbe`, `SelectionSource`) so both
//! properties are provable on a runner with no desktop session, and so the macOS
//! Accessibility prompt is a single named call site instead of a scattering of
//! `ATTRIBUTION` strings.

use std::path::PathBuf;

/// What the OS says about the permission smart grab needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionState {
    /// Never asked (fresh install, or the toggle has not been flipped yet).
    Unknown,
    /// This platform does not gate reading the file manager's selection. Windows
    /// reads the shell's selection in-process as the same user, so there is nothing
    /// to grant — reporting `NotRequired` keeps the UI honest instead of showing a
    /// fake prompt.
    NotRequired,
    Granted,
    Denied,
    /// The platform is not implemented (macOS in v1, see `ARCHITECTURE.md` §10.5).
    Unsupported,
}

impl PermissionState {
    pub fn as_str(self) -> &'static str {
        match self {
            PermissionState::Unknown => "unknown",
            PermissionState::NotRequired => "granted",
            PermissionState::Granted => "granted",
            PermissionState::Denied => "denied",
            PermissionState::Unsupported => "unsupported",
        }
    }

    /// Only these states allow a grab to be attempted.
    pub fn allows_grab(self) -> bool {
        matches!(
            self,
            PermissionState::NotRequired | PermissionState::Granted
        )
    }

    pub fn from_setting(value: &str) -> Self {
        match value {
            "granted" => PermissionState::Granted,
            "denied" => PermissionState::Denied,
            "unsupported" => PermissionState::Unsupported,
            _ => PermissionState::Unknown,
        }
    }
}

/// The OS permission call. `prompt` is `true` only for a user-initiated enable.
pub trait PermissionProbe {
    fn probe(&mut self, prompt: bool) -> PermissionState;
}

/// Reading the file manager's current selection.
pub trait SelectionSource {
    fn selection(&mut self) -> Result<Vec<PathBuf>, GrabError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GrabError {
    /// Smart grab is off: nothing was read and nothing was asked.
    Disabled,
    /// The OS needs consent first (macOS Accessibility).
    NeedsPermission(PermissionState),
    /// The platform has no reader yet.
    Unsupported(&'static str),
    /// Something went wrong while reading (no Explorer window, empty selection…).
    Unavailable(String),
}

impl std::fmt::Display for GrabError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GrabError::Disabled => write!(f, "smart grab is off"),
            GrabError::NeedsPermission(state) => {
                write!(f, "smart grab needs permission (state: {})", state.as_str())
            }
            GrabError::Unsupported(why) => write!(f, "smart grab is not available: {why}"),
            GrabError::Unavailable(why) => write!(f, "smart grab found nothing: {why}"),
        }
    }
}

/// What flipping the toggle produced, for the settings UI to render.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnableOutcome {
    pub enabled: bool,
    pub permission: PermissionState,
    /// One line for diagnostics: whether a prompt was shown, and why.
    pub message: String,
    /// True when the platform asked the user something. The macOS Accessibility
    /// prompt is the only thing in this app that can be true.
    pub prompted: bool,
}

/// The persisted half of the setting plus the in-flight permission state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SmartGrab {
    enabled: bool,
    permission: PermissionState,
}

impl Default for SmartGrab {
    fn default() -> Self {
        // `SPEC.md` §2 / §15: off until the user says otherwise.
        Self {
            enabled: false,
            permission: PermissionState::Unknown,
        }
    }
}

impl SmartGrab {
    pub fn new(enabled: bool, permission: PermissionState) -> Self {
        Self {
            enabled,
            permission,
        }
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    pub fn permission(&self) -> PermissionState {
        self.permission
    }

    /// Turns smart grab on or off.
    ///
    /// Enabling probes the OS exactly once, with `prompt = true`; disabling never
    /// calls the probe (`ARCHITECTURE.md` §15: "disabling it requires no permission at
    /// all"). Disabling also forgets a `Denied` state so the next enable asks again
    /// instead of silently staying refused.
    pub fn set_enabled(&mut self, enable: bool, probe: &mut dyn PermissionProbe) -> EnableOutcome {
        if !enable {
            self.enabled = false;
            if self.permission == PermissionState::Denied {
                self.permission = PermissionState::Unknown;
            }
            return EnableOutcome {
                enabled: false,
                permission: self.permission,
                message: "smart grab off; no permission was requested".into(),
                prompted: false,
            };
        }

        let permission = probe.probe(true);
        self.permission = permission;
        self.enabled = permission.allows_grab();
        let prompted = permission == PermissionState::Denied;
        let message = match permission {
            PermissionState::NotRequired => {
                "smart grab on; this platform needs no extra permission".to_string()
            }
            PermissionState::Granted => "smart grab on; permission granted".to_string(),
            PermissionState::Denied => {
                "smart grab needs permission. Grant it in System Settings › Privacy & \
                 Security › Accessibility, then turn smart grab on again."
                    .to_string()
            }
            PermissionState::Unsupported => {
                "smart grab is not available on this platform yet".to_string()
            }
            PermissionState::Unknown => "smart grab could not be enabled".to_string(),
        };
        EnableOutcome {
            enabled: self.enabled,
            permission,
            message,
            prompted,
        }
    }

    /// Reads the current selection — or refuses without touching the source.
    pub fn grab(&self, source: &mut dyn SelectionSource) -> Result<Vec<PathBuf>, GrabError> {
        if !self.enabled {
            return Err(GrabError::Disabled);
        }
        if !self.permission.allows_grab() {
            return Err(GrabError::NeedsPermission(self.permission));
        }
        let paths = source.selection()?;
        if paths.is_empty() {
            return Err(GrabError::Unavailable("nothing is selected".into()));
        }
        Ok(paths)
    }
}

/// The production probe.
///
/// Windows: `NotRequired` — reading Explorer's selection is an in-process shell call
/// as the signed-in user, so there is no consent to obtain and a prompt would be
/// fiction. macOS: PLANNED — `AXIsProcessTrustedWithOptions(prompt)` is the call, but
/// this project has never measured macOS (`ARCHITECTURE.md` §10.5), so the adapter
/// reports `Unsupported` rather than claiming a flow it cannot run.
#[cfg(windows)]
pub struct SystemPermission;

#[cfg(windows)]
impl PermissionProbe for SystemPermission {
    fn probe(&mut self, _prompt: bool) -> PermissionState {
        PermissionState::NotRequired
    }
}

#[cfg(not(windows))]
pub struct SystemPermission;

#[cfg(not(windows))]
impl PermissionProbe for SystemPermission {
    fn probe(&mut self, _prompt: bool) -> PermissionState {
        PermissionState::Unsupported
    }
}

/// The production selection source.
///
/// Windows: **not implemented in this build.** The reader has to go through the shell's
/// COM surface (`IShellWindows` → `IShellBrowser` → `IFolderView` → `SHGetNameFromIDList`),
/// which is a separate, testable unit of unsafe code; shipping a stub that silently
/// returns "nothing selected" would make the toggle look broken in a way the user
/// cannot diagnose. The honest refusal below is what the UI shows, and the reader has
/// its own card (see the `U3` handoff). The *consent* half of smart grab — off by
/// default, probe only on enable, nothing on disable — is implemented and tested
/// above, because that is what `SPEC.md` §2 makes a promise about.
#[cfg(windows)]
struct WindowsExplorerSelection;

#[cfg(windows)]
impl SelectionSource for WindowsExplorerSelection {
    fn selection(&mut self) -> Result<Vec<PathBuf>, GrabError> {
        Err(GrabError::Unsupported(
            "reading the Explorer selection is not implemented yet",
        ))
    }
}

#[cfg(not(windows))]
struct MacFinderSelection;

#[cfg(not(windows))]
impl SelectionSource for MacFinderSelection {
    fn selection(&mut self) -> Result<Vec<PathBuf>, GrabError> {
        Err(GrabError::Unsupported(
            "reading the Finder selection is not implemented on this platform",
        ))
    }
}

/// The source the app uses. Boxed so the caller never branches on the platform.
pub fn platform_selection_source() -> Box<dyn SelectionSource + Send> {
    #[cfg(windows)]
    {
        Box::new(WindowsExplorerSelection)
    }
    #[cfg(not(windows))]
    {
        Box::new(MacFinderSelection)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct CountingProbe {
        calls: usize,
        prompts: usize,
        answer: Option<PermissionState>,
    }

    impl PermissionProbe for CountingProbe {
        fn probe(&mut self, prompt: bool) -> PermissionState {
            self.calls += 1;
            if prompt {
                self.prompts += 1;
            }
            self.answer.unwrap_or(PermissionState::NotRequired)
        }
    }

    #[derive(Default)]
    struct CountingSource {
        calls: usize,
        paths: Vec<PathBuf>,
    }

    impl SelectionSource for CountingSource {
        fn selection(&mut self) -> Result<Vec<PathBuf>, GrabError> {
            self.calls += 1;
            Ok(self.paths.clone())
        }
    }

    #[test]
    fn smart_grab_is_off_until_the_user_turns_it_on() {
        let grab = SmartGrab::default();
        assert!(!grab.enabled());
        assert_eq!(grab.permission(), PermissionState::Unknown);
    }

    #[test]
    fn disabling_asks_the_os_nothing_at_all() {
        let mut probe = CountingProbe::default();
        let mut grab = SmartGrab::default();
        let _ = grab.set_enabled(true, &mut probe);
        assert_eq!(probe.calls, 1, "enabling probes once");

        let outcome = grab.set_enabled(false, &mut probe);
        assert!(!outcome.enabled);
        assert!(!outcome.prompted);
        assert_eq!(probe.calls, 1, "disabling must not call the permission API");
        assert_eq!(probe.prompts, 1);
        assert!(outcome.message.contains("no permission"));
    }

    #[test]
    fn enabling_probes_with_prompt_and_only_once() {
        let mut probe = CountingProbe::default();
        let mut grab = SmartGrab::default();
        let outcome = grab.set_enabled(true, &mut probe);
        assert!(outcome.enabled);
        assert_eq!(probe.calls, 1);
        assert_eq!(probe.prompts, 1);
        assert_eq!(outcome.permission, PermissionState::NotRequired);
    }

    #[test]
    fn a_denied_permission_leaves_the_feature_off_and_says_where_to_grant_it() {
        let mut probe = CountingProbe {
            answer: Some(PermissionState::Denied),
            ..Default::default()
        };
        let mut grab = SmartGrab::default();
        let outcome = grab.set_enabled(true, &mut probe);
        assert!(!outcome.enabled, "a denial must not leave the feature on");
        assert!(outcome.prompted);
        assert!(outcome.message.contains("Accessibility"));
        assert!(!grab.enabled());
        // Disabling afterwards resets the refusal so the next enable can ask again.
        let _ = grab.set_enabled(false, &mut probe);
        assert_eq!(grab.permission(), PermissionState::Unknown);
    }

    #[test]
    fn an_unsupported_platform_cannot_be_enabled() {
        let mut probe = CountingProbe {
            answer: Some(PermissionState::Unsupported),
            ..Default::default()
        };
        let mut grab = SmartGrab::default();
        let outcome = grab.set_enabled(true, &mut probe);
        assert!(!outcome.enabled);
        assert!(!outcome.prompted);
        assert!(outcome.message.contains("not available"));
    }

    #[test]
    fn grabbing_while_disabled_reads_no_selection() {
        let mut source = CountingSource {
            paths: vec![PathBuf::from("/tmp/a.png")],
            ..Default::default()
        };
        let grab = SmartGrab::default();
        assert_eq!(grab.grab(&mut source), Err(GrabError::Disabled));
        assert_eq!(source.calls, 0, "a disabled grab must not touch the shell");
    }

    #[test]
    fn an_enabled_grab_returns_the_selection_and_an_empty_one_is_an_error() {
        let mut probe = CountingProbe::default();
        let mut grab = SmartGrab::default();
        grab.set_enabled(true, &mut probe);

        let mut source = CountingSource {
            paths: vec![PathBuf::from("C:/Users/ryo/Desktop/a.png")],
            ..Default::default()
        };
        assert_eq!(
            grab.grab(&mut source).unwrap(),
            vec![PathBuf::from("C:/Users/ryo/Desktop/a.png")]
        );

        let mut empty = CountingSource::default();
        assert!(matches!(
            grab.grab(&mut empty),
            Err(GrabError::Unavailable(_))
        ));
        assert_eq!(empty.calls, 1);
    }

    #[test]
    fn a_permission_state_round_trips_through_the_settings_string() {
        for state in [
            PermissionState::Unknown,
            PermissionState::NotRequired,
            PermissionState::Granted,
            PermissionState::Denied,
            PermissionState::Unsupported,
        ] {
            assert_eq!(
                PermissionState::from_setting(state.as_str()).allows_grab(),
                state.allows_grab(),
                "{state:?}"
            );
        }
        // `NotRequired` persists as "granted": that is what `settings.json` allows, and
        // the distinction is presentational.
        assert_eq!(PermissionState::NotRequired.as_str(), "granted");
    }
}
