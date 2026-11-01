//! Global hotkey registration with conflict detection (`SPEC.md` §2, `ARCHITECTURE.md`
//! §10.3).
//!
//! The interesting part is not `RegisterHotKey` — it is what happens when the chord is
//! **already taken** by another application (or by a second copy of this one). That
//! path is modelled as data (`Acquisition`), tested against a stub registrar, and only
//! then wired to the OS, because the conflict case is the one a user hits and the one
//! that cannot be reproduced on demand on a test box.
//!
//! Platform notes:
//!
//! - **Windows**: a dedicated thread owns registration and the message queue, because
//!   `RegisterHotKey(NULL, …)` posts `WM_HOTKEY` to the *registering thread's* queue.
//!   The thread waits on an event so a rebind request can be serviced immediately
//!   (a plain `GetMessage` loop would strand it).
//! - **macOS**: PLANNED. `RegisterEventHotKey`/`CGEventTap` are unmeasured here
//!   (`ARCHITECTURE.md` §10.5); the adapter reports `Failed` with an explicit reason
//!   instead of pretending the chord was taken.
//!
//! Hand-rolled FFI for the same reason as `screen.rs`: the acceptance spike compiles
//! this exact file.

use crate::shell::accelerator::{self, Accelerator};

/// `ERROR_HOTKEY_ALREADY_REGISTERED` (1409). The only error code that means "another
/// application owns this chord"; every other code means our own call was wrong.
pub const CONFLICT_CODE: u32 = 1409;

/// A registration failure as the OS reported it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegError {
    pub code: u32,
    pub detail: String,
}

impl RegError {
    pub fn new(code: u32, detail: impl Into<String>) -> Self {
        Self {
            code,
            detail: detail.into(),
        }
    }

    /// True when the chord is owned by someone else (as opposed to a bad chord, a
    /// dead message queue, or an unsupported platform).
    pub fn is_conflict(&self) -> bool {
        self.code == CONFLICT_CODE
    }
}

/// One registration attempt, recorded so the outcome can explain itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attempt {
    pub accelerator: String,
    pub code: u32,
    pub detail: String,
}

/// The result of asking for a hotkey.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Acquisition {
    /// Registered, possibly after the wanted chord was found to be taken.
    Registered {
        accelerator: Accelerator,
        attempts: Vec<Attempt>,
    },
    /// Every chord we tried belongs to another application. The user has to choose
    /// another one; nothing was registered.
    AllTaken { attempts: Vec<Attempt> },
    /// Something other than a conflict went wrong (unsupported platform, a bad
    /// virtual key, a dead thread). Distinguished from `AllTaken` so the UI does not
    /// tell the user to pick a different chord when the platform is the problem.
    Failed { attempts: Vec<Attempt> },
}

impl Acquisition {
    pub fn accelerator(&self) -> Option<Accelerator> {
        match self {
            Acquisition::Registered { accelerator, .. } => Some(*accelerator),
            _ => None,
        }
    }

    /// One line for the diagnostics pane / log.
    pub fn describe(&self) -> String {
        match self {
            Acquisition::Registered {
                accelerator,
                attempts,
            } => {
                if attempts.is_empty() {
                    format!("{accelerator} registered")
                } else {
                    format!(
                        "{accelerator} registered after {} conflict(s): {}",
                        attempts.len(),
                        attempts
                            .iter()
                            .map(|a| format!("{} (code {})", a.accelerator, a.code))
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                }
            }
            Acquisition::AllTaken { attempts } => format!(
                "no hotkey available: {}",
                attempts
                    .iter()
                    .map(|a| format!("{} taken (code {})", a.accelerator, a.code))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Acquisition::Failed { attempts } => format!(
                "hotkey could not be registered: {}",
                attempts
                    .iter()
                    .map(|a| format!("{} -> {}", a.accelerator, a.detail))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        }
    }
}

/// The OS-facing half, reduced to two calls so the conflict logic is testable without
/// a desktop session.
pub trait Registrar {
    fn register(&mut self, accelerator: Accelerator) -> Result<(), RegError>;
    fn unregister(&mut self, accelerator: Accelerator);
}

/// Tries `wanted`, then each fallback in order, and reports why each attempt failed.
///
/// A conflict (`code 1409`) moves to the next candidate; any other error stops the
/// ladder, because retrying a chord the platform rejected for a different reason
/// would just fail again.
pub fn acquire(
    registrar: &mut dyn Registrar,
    wanted: Accelerator,
    fallbacks: &[Accelerator],
) -> Acquisition {
    let mut attempts: Vec<Attempt> = Vec::new();
    let mut candidates = std::iter::once(wanted).chain(fallbacks.iter().copied());

    while let Some(candidate) = candidates.next() {
        match registrar.register(candidate) {
            Ok(()) => {
                if candidate != wanted {
                    // The wanted chord is taken; keep its attempt record so the UI can
                    // say *why* it is not what the user asked for.
                    attempts.retain(|a| a.code == CONFLICT_CODE);
                }
                return Acquisition::Registered {
                    accelerator: candidate,
                    attempts,
                };
            }
            Err(error) => {
                let conflict = error.is_conflict();
                attempts.push(Attempt {
                    accelerator: candidate.canonical(),
                    code: error.code,
                    detail: error.detail,
                });
                if !conflict {
                    return Acquisition::Failed { attempts };
                }
            }
        }
    }

    Acquisition::AllTaken { attempts }
}

/// Registers the accelerator from settings, with the ladder from `accelerator.rs`.
pub fn acquire_from_settings(registrar: &mut dyn Registrar, configured: &str) -> Acquisition {
    match accelerator::parse(configured) {
        Ok(wanted) => {
            let ladder = accelerator::conflict_ladder(wanted);
            acquire(registrar, wanted, &ladder)
        }
        Err(error) => Acquisition::Failed {
            attempts: vec![Attempt {
                accelerator: configured.to_string(),
                code: 0,
                detail: error.to_string(),
            }],
        },
    }
}

#[cfg(windows)]
mod win32 {
    use super::*;
    use std::collections::VecDeque;
    use std::sync::mpsc::{channel, Receiver, Sender};
    use std::sync::{Arc, Mutex};
    use std::thread::JoinHandle;

    type Bool = i32;
    type Handle = isize;
    type Uint = u32;

    const WM_HOTKEY: Uint = 0x0312;
    const INFINITE: Uint = 0xFFFF_FFFF;
    const WAIT_OBJECT_0: Uint = 0;
    const QS_ALLINPUT: Uint = 0x04FF;
    const PM_REMOVE: Uint = 0x0001;

    #[repr(C)]
    #[derive(Debug, Clone, Copy, Default)]
    struct Msg {
        hwnd: Handle,
        message: Uint,
        wparam: usize,
        lparam: isize,
        time: Uint,
        pt: [i32; 2],
    }

    #[link(name = "user32")]
    extern "system" {
        fn RegisterHotKey(hwnd: Handle, id: i32, modifiers: Uint, vk: Uint) -> Bool;
        fn UnregisterHotKey(hwnd: Handle, id: i32) -> Bool;
        fn PeekMessageW(msg: *mut Msg, hwnd: Handle, min: Uint, max: Uint, remove: Uint) -> Bool;
        fn MsgWaitForMultipleObjectsEx(
            count: Uint,
            handles: *const Handle,
            millis: Uint,
            wake_mask: Uint,
            flags: Uint,
        ) -> Uint;
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn CreateEventW(
            attributes: *mut u8,
            manual_reset: Bool,
            initial_state: Bool,
            name: *const u16,
        ) -> Handle;
        fn SetEvent(event: Handle) -> Bool;
        fn CloseHandle(handle: Handle) -> Bool;
        fn GetLastError() -> Uint;
    }

    /// The hotkey id we own. One app, one hotkey (`SPEC.md` §2).
    const HOTKEY_ID: i32 = 0xE2C0;

    /// A registrar bound to the thread that owns the message queue. `RegisterHotKey`
    /// with a null window is thread-scoped, so this must never be moved to another
    /// thread.
    struct ThreadRegistrar {
        active: Option<Accelerator>,
    }

    impl Registrar for ThreadRegistrar {
        fn register(&mut self, accelerator: Accelerator) -> Result<(), RegError> {
            if let Some(previous) = self.active.take() {
                self.unregister(previous);
            }
            // SAFETY: null HWND + a valid modifier mask and virtual-key code.
            let ok = unsafe {
                RegisterHotKey(0, HOTKEY_ID, accelerator.register_mask(), accelerator.vk())
            };
            if ok != 0 {
                self.active = Some(accelerator);
                return Ok(());
            }
            // SAFETY: reads the thread's last error, set by the call above.
            let code = unsafe { GetLastError() };
            Err(RegError::new(
                code,
                match code {
                    CONFLICT_CODE => "another application already owns this chord".to_string(),
                    other => format!("RegisterHotKey failed with code {other}"),
                },
            ))
        }

        fn unregister(&mut self, accelerator: Accelerator) {
            if self.active == Some(accelerator) || self.active.is_some() {
                // SAFETY: pairs with the successful register on the same thread.
                unsafe {
                    UnregisterHotKey(0, HOTKEY_ID);
                }
                self.active = None;
            }
        }
    }

    enum Command {
        Acquire {
            wanted: Accelerator,
            fallbacks: Vec<Accelerator>,
            reply: Sender<Acquisition>,
        },
        Clear {
            reply: Sender<()>,
        },
        Shutdown {
            reply: Sender<()>,
        },
    }

    /// Handle to the hotkey thread. Dropping it unregisters and stops the thread.
    pub struct Hotkeys {
        commands: Arc<Mutex<VecDeque<Command>>>,
        wake: Handle,
        thread: Option<JoinHandle<()>>,
    }

    // SAFETY: `wake` is an auto-reset event handle owned by this struct and only used
    // through `SetEvent`/`CloseHandle`, both thread-safe; `commands` is behind a mutex.
    unsafe impl Send for Hotkeys {}

    impl Hotkeys {
        /// Starts the registration thread. `on_fire` runs on that thread — it must be
        /// cheap (the production callback posts to the Tauri event loop).
        pub fn start<F>(on_fire: F) -> std::io::Result<Self>
        where
            F: FnMut(Accelerator) + Send + 'static,
        {
            let commands: Arc<Mutex<VecDeque<Command>>> = Arc::new(Mutex::new(VecDeque::new()));
            let thread_commands = Arc::clone(&commands);
            // SAFETY: standard auto-reset event; null attributes/name.
            let wake = unsafe { CreateEventW(std::ptr::null_mut(), 0, 0, std::ptr::null()) };
            if wake == 0 {
                return Err(std::io::Error::other("CreateEventW failed"));
            }
            let thread_wake = wake;
            let thread = std::thread::Builder::new()
                .name("ezc-hotkeys".into())
                .spawn(move || run(thread_commands, thread_wake, on_fire))?;
            Ok(Self {
                commands,
                wake,
                thread: Some(thread),
            })
        }

        /// Registers `wanted`, falling through `fallbacks` on conflict.
        pub fn acquire(&self, wanted: Accelerator, fallbacks: Vec<Accelerator>) -> Acquisition {
            let (reply, response) = channel();
            self.push(Command::Acquire {
                wanted,
                fallbacks,
                reply,
            });
            response.recv().unwrap_or(Acquisition::Failed {
                attempts: vec![Attempt {
                    accelerator: wanted.canonical(),
                    code: 0,
                    detail: "the hotkey thread stopped".into(),
                }],
            })
        }

        /// Registers whatever `settings.json` says, with the standard ladder.
        pub fn acquire_from_settings(&self, configured: &str) -> Acquisition {
            match accelerator::parse(configured) {
                Ok(wanted) => self.acquire(wanted, accelerator::conflict_ladder(wanted)),
                Err(error) => Acquisition::Failed {
                    attempts: vec![Attempt {
                        accelerator: configured.to_string(),
                        code: 0,
                        detail: error.to_string(),
                    }],
                },
            }
        }

        /// Unregisters the active chord (used when the user turns the hotkey off).
        pub fn clear(&self) {
            let (reply, response) = channel();
            self.push(Command::Clear { reply });
            let _ = response.recv();
        }

        fn push(&self, command: Command) {
            if let Ok(mut queue) = self.commands.lock() {
                queue.push_back(command);
            }
            // SAFETY: `wake` is a live event handle for the lifetime of `self`.
            unsafe {
                SetEvent(self.wake);
            }
        }
    }

    impl Drop for Hotkeys {
        fn drop(&mut self) {
            let (reply, response) = channel();
            self.push(Command::Shutdown { reply });
            let _ = response.recv();
            if let Some(thread) = self.thread.take() {
                let _ = thread.join();
            }
            // SAFETY: the thread has stopped and no longer waits on this handle.
            unsafe {
                CloseHandle(self.wake);
            }
        }
    }

    fn run<F>(commands: Arc<Mutex<VecDeque<Command>>>, wake: Handle, mut on_fire: F)
    where
        F: FnMut(Accelerator),
    {
        let mut registrar = ThreadRegistrar { active: None };
        let mut running = true;
        while running {
            loop {
                let command = commands.lock().ok().and_then(|mut q| q.pop_front());
                let Some(command) = command else { break };
                match command {
                    Command::Acquire {
                        wanted,
                        fallbacks,
                        reply,
                    } => {
                        let outcome = acquire(&mut registrar, wanted, &fallbacks);
                        let _ = reply.send(outcome);
                    }
                    Command::Clear { reply } => {
                        if let Some(active) = registrar.active {
                            registrar.unregister(active);
                        }
                        let _ = reply.send(());
                    }
                    Command::Shutdown { reply } => {
                        if let Some(active) = registrar.active {
                            registrar.unregister(active);
                        }
                        running = false;
                        let _ = reply.send(());
                    }
                }
            }
            if !running {
                break;
            }
            // Wait for either a command (event) or a message (hotkey press).
            // SAFETY: one valid handle; the queue's wake mask is QS_ALLINPUT.
            unsafe {
                MsgWaitForMultipleObjectsEx(1, &wake, INFINITE, QS_ALLINPUT, 0);
            }
            let mut msg = Msg::default();
            // SAFETY: `msg` is writable and the loop only reads `msg.message`.
            while unsafe { PeekMessageW(&mut msg, 0, 0, 0, PM_REMOVE) } != 0 {
                if msg.message == WM_HOTKEY {
                    if let Some(active) = registrar.active {
                        on_fire(active);
                    }
                }
            }
            let _ = WAIT_OBJECT_0;
        }
    }

    /// The registrar handle type the app stores.
    pub type PlatformHotkeys = Hotkeys;
}

#[cfg(windows)]
pub use win32::{Hotkeys as PlatformHotkeys};

#[cfg(not(windows))]
mod other {
    use super::*;

    /// PLANNED on macOS: `RegisterEventHotKey` needs Carbon, or `CGEventTap` needs
    /// Accessibility consent — and `SPEC.md` §2 says consent is asked **only** when
    /// the user turns smart grab on. So the hotkey itself must not depend on a
    /// permission prompt, which is one more reason this is unmeasured rather than
    /// guessed. Reports `Failed` with a reason the UI can show verbatim.
    pub struct PlatformHotkeys;

    impl PlatformHotkeys {
        pub fn start<F>(_on_fire: F) -> std::io::Result<Self>
        where
            F: FnMut(Accelerator) + Send + 'static,
        {
            Ok(Self)
        }

        pub fn acquire(&self, wanted: Accelerator, _fallbacks: Vec<Accelerator>) -> Acquisition {
            Acquisition::Failed {
                attempts: vec![Attempt {
                    accelerator: wanted.canonical(),
                    code: 0,
                    detail: "global hotkeys are not implemented on this platform (macOS unmeasured)"
                        .into(),
                }],
            }
        }

        pub fn acquire_from_settings(&self, configured: &str) -> Acquisition {
            match accelerator::parse(configured) {
                Ok(wanted) => self.acquire(wanted, Vec::new()),
                Err(error) => Acquisition::Failed {
                    attempts: vec![Attempt {
                        accelerator: configured.to_string(),
                        code: 0,
                        detail: error.to_string(),
                    }],
                },
            }
        }

        pub fn clear(&self) {}
    }
}

#[cfg(not(windows))]
pub use other::PlatformHotkeys;

#[cfg(test)]
mod tests {
    use super::*;

    /// A registrar that fails the first `taken` chords with 1409 and then succeeds,
    /// recording the order it was asked in.
    #[derive(Default)]
    struct StubRegistrar {
        taken: Vec<String>,
        registered: Vec<String>,
        fail_with: Option<u32>,
        asked: Vec<String>,
    }

    impl Registrar for StubRegistrar {
        fn register(&mut self, accelerator: Accelerator) -> Result<(), RegError> {
            let name = accelerator.canonical();
            self.asked.push(name.clone());
            if let Some(code) = self.fail_with {
                return Err(RegError::new(code, "stub failure"));
            }
            if self.taken.contains(&name) {
                return Err(RegError::new(CONFLICT_CODE, "stub conflict"));
            }
            self.registered.push(name);
            Ok(())
        }

        fn unregister(&mut self, accelerator: Accelerator) {
            self.registered.retain(|n| *n != accelerator.canonical());
        }
    }

    fn default_hotkey() -> Accelerator {
        accelerator::parse("Ctrl+Alt+Shift+D").unwrap()
    }

    #[test]
    fn a_free_chord_registers_on_the_first_attempt() {
        let mut registrar = StubRegistrar::default();
        let wanted = default_hotkey();
        let outcome = acquire(&mut registrar, wanted, &accelerator::conflict_ladder(wanted));
        assert_eq!(
            outcome,
            Acquisition::Registered {
                accelerator: wanted,
                attempts: vec![]
            }
        );
        assert_eq!(registrar.asked, vec!["Ctrl+Alt+Shift+D".to_string()]);
    }

    #[test]
    fn a_taken_chord_falls_through_to_the_next_and_says_why() {
        let mut registrar = StubRegistrar {
            taken: vec!["Ctrl+Alt+Shift+D".into()],
            ..Default::default()
        };
        let wanted = default_hotkey();
        let outcome = acquire(&mut registrar, wanted, &accelerator::conflict_ladder(wanted));

        let Acquisition::Registered {
            accelerator,
            attempts,
        } = outcome
        else {
            panic!("expected a fallback registration, got {outcome:?}");
        };
        assert_ne!(accelerator, wanted, "must not claim the taken chord");
        assert_eq!(attempts.len(), 1, "exactly one conflict recorded");
        assert_eq!(attempts[0].code, CONFLICT_CODE);
        assert_eq!(attempts[0].accelerator, "Ctrl+Alt+Shift+D");
        assert_eq!(
            registrar.asked.len(),
            2,
            "asked for the wanted chord then one alternative: {:?}",
            registrar.asked
        );
        assert!(outcome_describe(&accelerator, &attempts).contains("conflict"));
    }

    fn outcome_describe(accelerator: &Accelerator, attempts: &[Attempt]) -> String {
        Acquisition::Registered {
            accelerator: *accelerator,
            attempts: attempts.to_vec(),
        }
        .describe()
    }

    #[test]
    fn when_every_candidate_is_taken_nothing_is_registered_and_the_reason_is_a_conflict() {
        let wanted = default_hotkey();
        let ladder = accelerator::conflict_ladder(wanted);
        let mut registrar = StubRegistrar {
            taken: std::iter::once(wanted.canonical())
                .chain(ladder.iter().map(|a| a.canonical()))
                .collect(),
            ..Default::default()
        };
        let outcome = acquire(&mut registrar, wanted, &ladder);
        let Acquisition::AllTaken { attempts } = outcome else {
            panic!("expected AllTaken, got {outcome:?}");
        };
        assert_eq!(attempts.len(), 1 + ladder.len());
        assert!(attempts.iter().all(|a| a.code == CONFLICT_CODE));
        assert!(registrar.registered.is_empty());
    }

    #[test]
    fn a_non_conflict_error_stops_the_ladder_instead_of_retrying() {
        // A bad virtual key would fail identically on every candidate; asking 8 times
        // would just be noise in the log.
        let mut registrar = StubRegistrar {
            fail_with: Some(87), // ERROR_INVALID_PARAMETER
            ..Default::default()
        };
        let wanted = default_hotkey();
        let outcome = acquire(&mut registrar, wanted, &accelerator::conflict_ladder(wanted));
        assert!(matches!(outcome, Acquisition::Failed { .. }));
        assert_eq!(registrar.asked.len(), 1, "stopped after the first failure");
    }

    #[test]
    fn an_unparseable_setting_is_reported_without_touching_the_os() {
        let mut registrar = StubRegistrar::default();
        let outcome = acquire_from_settings(&mut registrar, "Ctrl+Banana");
        assert!(matches!(outcome, Acquisition::Failed { .. }));
        assert!(outcome.describe().contains("Banana"));
        assert!(registrar.asked.is_empty());
    }
}
