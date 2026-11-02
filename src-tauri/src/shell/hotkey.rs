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
//! - **macOS**: Carbon `RegisterEventHotKey` on the application event target, with an
//!   `InstallEventHandler` handler for `kEventHotKeyPressed`. Carbon rather than
//!   `CGEventTap` because a hot key registered this way needs **no** Accessibility
//!   (TCC) grant, and `SPEC.md` §2 only asks for consent when the user turns smart
//!   grab on. There is no hotkey thread here: `RegisterHotKey(NULL, …)` is
//!   thread-scoped, but a Carbon hot key belongs to the *app's* event target, so it
//!   is registered on the thread that owns the run loop (`.setup()` and the IPC
//!   command both run there).
//!
//! Hand-rolled FFI for the same reason as `screen.rs`: the acceptance spike compiles
//! this exact file.

use crate::shell::accelerator::{self, Accelerator, Key};

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

// ---- macOS: the portable half of the Carbon registrar --------------------
//
// Everything below is platform-free (`u32` masks and key codes, `i32` OSStatus —
// no Carbon types), so the mapping and the conflict classification are unit-tested
// on Linux, where the registration itself cannot run. `mod macos` is then a thin
// wrapper around `RegisterEventHotKey`.

/// Carbon `cmdKey` (`HIToolbox/Events.h`).
pub const CARBON_CMD: u32 = 0x0100;
/// Carbon `shiftKey`.
pub const CARBON_SHIFT: u32 = 0x0200;
/// Carbon `optionKey`.
pub const CARBON_OPTION: u32 = 0x0800;
/// Carbon `controlKey`.
pub const CARBON_CONTROL: u32 = 0x1000;

/// `eventHotKeyExistsErr` (-9878): the chord is already registered, by another
/// application or by an earlier registration in this one. macOS's equivalent of
/// `ERROR_HOTKEY_ALREADY_REGISTERED`, which is what keeps the conflict ladder and
/// the `AllTaken` case working unchanged.
pub const EVENT_HOTKEY_EXISTS_ERR: i32 = -9878;
/// `paramErr` (-50).
pub const PARAM_ERR: i32 = -50;
/// `eventNotHandledErr` (-9874): the event was not ours, so Carbon may offer it to
/// the next handler.
pub const EVENT_NOT_HANDLED_ERR: i32 = -9874;

/// The accelerator's modifiers as Carbon's mask.
///
/// `Ctrl` is Control, `Alt` is Option, `Meta`/`Win` is Command: the same meanings
/// `accelerator.rs` gives them, so the chord a user configured is the chord they
/// get. A chord spelled `Ctrl+Alt+Shift+D` therefore presses **Control+Option+
/// Shift+D** on a Mac — the string in `settings.json` is not rewritten per platform.
pub fn carbon_modifiers(accelerator: Accelerator) -> u32 {
    let mut mask = 0;
    if accelerator.ctrl {
        mask |= CARBON_CONTROL;
    }
    if accelerator.alt {
        mask |= CARBON_OPTION;
    }
    if accelerator.shift {
        mask |= CARBON_SHIFT;
    }
    if accelerator.meta {
        mask |= CARBON_CMD;
    }
    mask
}

/// The macOS virtual key code (`kVK_*`) for a key, or `None` when the key does not
/// exist on a Mac keyboard.
///
/// `None` is a real answer, not a gap: `Insert` has no `kVK_` constant (the Help key
/// sits in that position on Apple keyboards and cannot be told apart reliably), and
/// the function keys stop at `F20`. Registering *something else* would give the user
/// a chord they cannot press, so the registrar reports the chord as unusable.
pub fn carbon_key_code(key: Key) -> Option<u32> {
    let code = match key {
        // Letters, ANSI layout (`kVK_ANSI_*`), in code order.
        Key::Letter('A') => 0x00,
        Key::Letter('S') => 0x01,
        Key::Letter('D') => 0x02,
        Key::Letter('F') => 0x03,
        Key::Letter('H') => 0x04,
        Key::Letter('G') => 0x05,
        Key::Letter('Z') => 0x06,
        Key::Letter('X') => 0x07,
        Key::Letter('C') => 0x08,
        Key::Letter('V') => 0x09,
        Key::Letter('B') => 0x0B,
        Key::Letter('Q') => 0x0C,
        Key::Letter('W') => 0x0D,
        Key::Letter('E') => 0x0E,
        Key::Letter('R') => 0x0F,
        Key::Letter('Y') => 0x10,
        Key::Letter('T') => 0x11,
        Key::Letter('O') => 0x1F,
        Key::Letter('U') => 0x20,
        Key::Letter('I') => 0x22,
        Key::Letter('P') => 0x23,
        Key::Letter('L') => 0x25,
        Key::Letter('J') => 0x26,
        Key::Letter('K') => 0x28,
        Key::Letter('N') => 0x2D,
        Key::Letter('M') => 0x2E,
        Key::Letter(_) => return None,
        // Digits are not in numeric order: that is the layout, not a typo.
        Key::Digit(1) => 0x12,
        Key::Digit(2) => 0x13,
        Key::Digit(3) => 0x14,
        Key::Digit(4) => 0x15,
        Key::Digit(6) => 0x16,
        Key::Digit(5) => 0x17,
        Key::Digit(9) => 0x19,
        Key::Digit(7) => 0x1A,
        Key::Digit(8) => 0x1C,
        Key::Digit(0) => 0x1D,
        Key::Digit(_) => return None,
        Key::Function(1) => 0x7A,
        Key::Function(2) => 0x78,
        Key::Function(3) => 0x63,
        Key::Function(4) => 0x76,
        Key::Function(5) => 0x60,
        Key::Function(6) => 0x61,
        Key::Function(7) => 0x62,
        Key::Function(8) => 0x64,
        Key::Function(9) => 0x65,
        Key::Function(10) => 0x6D,
        Key::Function(11) => 0x67,
        Key::Function(12) => 0x6F,
        Key::Function(13) => 0x69,
        Key::Function(14) => 0x6B,
        Key::Function(15) => 0x71,
        Key::Function(16) => 0x6A,
        Key::Function(17) => 0x40,
        Key::Function(18) => 0x4F,
        Key::Function(19) => 0x50,
        Key::Function(20) => 0x5A,
        Key::Function(_) => return None,
        Key::Enter => 0x24,
        Key::Tab => 0x30,
        Key::Space => 0x31,
        Key::Backspace => 0x33,
        Key::Escape => 0x35,
        Key::Delete => 0x75,
        Key::Home => 0x73,
        Key::End => 0x77,
        Key::PageUp => 0x74,
        Key::PageDown => 0x79,
        Key::Left => 0x7B,
        Key::Right => 0x7C,
        Key::Down => 0x7D,
        Key::Up => 0x7E,
        // No `kVK_` constant on macOS.
        Key::Insert => return None,
    };
    Some(code)
}

/// Turns a Carbon `OSStatus` into the one thing callers branch on: *was this a
/// conflict?*
///
/// `eventHotKeyExistsErr` becomes `RegError::CONFLICT_CODE` so the existing ladder
/// walks to the next rung and `AllTaken` still means what it says; every other
/// status keeps its own (absolute) code and names the call, because a bad key code
/// must stop the ladder rather than retry it.
pub fn classify_carbon_status(status: i32, call: &str) -> RegError {
    if status == EVENT_HOTKEY_EXISTS_ERR {
        return RegError::new(
            CONFLICT_CODE,
            format!("another application already owns this chord ({call}: eventHotKeyExistsErr {status})"),
        );
    }
    let meaning = match status {
        PARAM_ERR => " (paramErr: the key code or modifier mask was rejected)",
        EVENT_NOT_HANDLED_ERR => " (eventNotHandledErr)",
        _ => "",
    };
    RegError::new(
        status.unsigned_abs(),
        format!("{call} failed with OSStatus {status}{meaning}"),
    )
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

#[cfg(target_os = "macos")]
mod macos {
    use super::*;
    use std::ffi::c_void;
    use std::sync::Mutex;

    type OSStatus = i32;
    type EventTargetRef = *mut c_void;
    type EventHotKeyRef = *mut c_void;
    type EventHandlerRef = *mut c_void;
    type EventRef = *mut c_void;
    type EventHandlerCallRef = *mut c_void;

    /// `EventHotKeyID` — our signature plus one id.
    #[repr(C)]
    #[derive(Debug, Clone, Copy, Default)]
    struct EventHotKeyId {
        signature: u32,
        id: u32,
    }

    #[repr(C)]
    #[derive(Debug, Clone, Copy)]
    struct EventTypeSpec {
        event_class: u32,
        event_kind: u32,
    }

    type EventHandlerProc = extern "C" fn(EventHandlerCallRef, EventRef, *mut c_void) -> OSStatus;

    #[link(name = "Carbon", kind = "framework")]
    extern "C" {
        fn GetApplicationEventTarget() -> EventTargetRef;
        fn InstallEventHandler(
            target: EventTargetRef,
            handler: EventHandlerProc,
            num_types: usize,
            types: *const EventTypeSpec,
            user_data: *mut c_void,
            out: *mut EventHandlerRef,
        ) -> OSStatus;
        fn RegisterEventHotKey(
            key_code: u32,
            modifiers: u32,
            hotkey_id: EventHotKeyId,
            target: EventTargetRef,
            options: u32,
            out: *mut EventHotKeyRef,
        ) -> OSStatus;
        fn UnregisterEventHotKey(hotkey: EventHotKeyRef) -> OSStatus;
        fn GetEventParameter(
            event: EventRef,
            name: u32,
            desired_type: u32,
            out_type: *mut u32,
            buffer_size: usize,
            out_size: *mut usize,
            out_data: *mut c_void,
        ) -> OSStatus;
    }

    /// `kEventClassKeyboard` (`'keyb'`).
    const EVENT_CLASS_KEYBOARD: u32 = 0x6B65_7962;
    /// `kEventHotKeyPressed`.
    const EVENT_HOTKEY_PRESSED: u32 = 5;
    /// `kEventParamDirectObject` (`'----'`) carrying a `typeEventHotKeyID` (`'hkid'`).
    const EVENT_PARAM_DIRECT_OBJECT: u32 = 0x2D2D_2D2D;
    const TYPE_EVENT_HOTKEY_ID: u32 = 0x686B_6964;
    /// Ours (`'EZC1'`), so a hot key belonging to some other app can never fire our
    /// callback even if the handler is offered one.
    const HOTKEY_SIGNATURE: u32 = 0x455A_4331;
    const HOTKEY_ID: u32 = 1;

    type Fire = Box<dyn FnMut(Accelerator) + Send>;

    /// The callback the app installed. A Carbon handler is a plain function pointer,
    /// so the closure has to live somewhere static: one app, one hotkey
    /// (`SPEC.md` §2).
    static ON_FIRE: Mutex<Option<Fire>> = Mutex::new(None);
    /// The accelerator actually registered, so a press reports the chord that fired.
    static ACTIVE: Mutex<Option<Accelerator>> = Mutex::new(None);
    /// The installed handler as an `EventHandlerRef`; 0 until installed.
    static HANDLER: Mutex<isize> = Mutex::new(0);

    /// Carbon's hot-key handler. Runs on the thread that dispatched the event — the
    /// main thread, where the app's run loop lives.
    extern "C" fn on_hotkey_pressed(
        _call: EventHandlerCallRef,
        event: EventRef,
        _user: *mut c_void,
    ) -> OSStatus {
        let mut id = EventHotKeyId::default();
        // SAFETY: `event` is the event Carbon is delivering to us and the buffer is
        // exactly `EventHotKeyId`, which is what the requested type says it is.
        let status = unsafe {
            GetEventParameter(
                event,
                EVENT_PARAM_DIRECT_OBJECT,
                TYPE_EVENT_HOTKEY_ID,
                std::ptr::null_mut(),
                std::mem::size_of::<EventHotKeyId>(),
                std::ptr::null_mut(),
                &mut id as *mut EventHotKeyId as *mut c_void,
            )
        };
        if status != 0 || id.signature != HOTKEY_SIGNATURE {
            return EVENT_NOT_HANDLED_ERR;
        }
        let active = *ACTIVE.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(accelerator) = active {
            // The callback runs with the slot held: a rebind cannot land in the
            // middle of a press, and the callback must not re-enter `acquire`.
            let mut slot = ON_FIRE.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(fire) = slot.as_mut() {
                fire(accelerator);
            }
        }
        0
    }

    /// Installs the handler once, on the app's event target.
    fn install_handler() -> Result<(), RegError> {
        let mut installed = HANDLER.lock().unwrap_or_else(|e| e.into_inner());
        if *installed != 0 {
            return Ok(());
        }
        let spec = EventTypeSpec {
            event_class: EVENT_CLASS_KEYBOARD,
            event_kind: EVENT_HOTKEY_PRESSED,
        };
        let mut handler: EventHandlerRef = std::ptr::null_mut();
        // SAFETY: the target is the app's own event target; `spec` outlives the call
        // and `handler` is a valid out-pointer.
        let status = unsafe {
            InstallEventHandler(
                GetApplicationEventTarget(),
                on_hotkey_pressed,
                1,
                &spec,
                std::ptr::null_mut(),
                &mut handler,
            )
        };
        if status != 0 {
            return Err(classify_carbon_status(status, "InstallEventHandler"));
        }
        *installed = handler as isize;
        Ok(())
    }

    /// One live registration. The `EventHotKeyRef` is kept as `isize` so the handle
    /// type stays `Send + Sync` without an `unsafe impl` — it is only ever handed
    /// back to `UnregisterEventHotKey`.
    #[derive(Debug, Clone, Copy)]
    struct Registration {
        reference: isize,
        accelerator: Accelerator,
    }

    /// The OS-facing half for one `acquire` run, holding the registration it owns so
    /// the shared ladder can try rung after rung.
    struct CarbonRegistrar {
        registration: Option<Registration>,
    }

    impl CarbonRegistrar {
        fn release(&mut self) {
            if let Some(registration) = self.registration.take() {
                // SAFETY: the reference came from a successful
                // `RegisterEventHotKey` and is unregistered exactly once.
                unsafe {
                    UnregisterEventHotKey(registration.reference as EventHotKeyRef);
                }
            }
            *ACTIVE.lock().unwrap_or_else(|e| e.into_inner()) = None;
        }
    }

    impl Registrar for CarbonRegistrar {
        fn register(&mut self, accelerator: Accelerator) -> Result<(), RegError> {
            self.release();
            let key_code = carbon_key_code(accelerator.key).ok_or_else(|| {
                RegError::new(
                    PARAM_ERR.unsigned_abs(),
                    format!(
                        "{} has no macOS key code (the key does not exist on a Mac keyboard)",
                        accelerator.canonical()
                    ),
                )
            })?;
            let mut reference: EventHotKeyRef = std::ptr::null_mut();
            // SAFETY: the app's event target, a key code and mask from the tables in
            // this file, and a valid out-pointer.
            let status = unsafe {
                RegisterEventHotKey(
                    key_code,
                    carbon_modifiers(accelerator),
                    EventHotKeyId {
                        signature: HOTKEY_SIGNATURE,
                        id: HOTKEY_ID,
                    },
                    GetApplicationEventTarget(),
                    0,
                    &mut reference,
                )
            };
            if status != 0 || reference.is_null() {
                return Err(classify_carbon_status(status, "RegisterEventHotKey"));
            }
            self.registration = Some(Registration {
                reference: reference as isize,
                accelerator,
            });
            *ACTIVE.lock().unwrap_or_else(|e| e.into_inner()) = Some(accelerator);
            Ok(())
        }

        fn unregister(&mut self, _accelerator: Accelerator) {
            self.release();
        }
    }

    /// Handle to macOS hot-key registration. Dropping it unregisters the chord.
    pub struct Hotkeys {
        active: Mutex<Option<Registration>>,
    }

    impl Hotkeys {
        /// Installs the event handler and remembers `on_fire`.
        ///
        /// Call this on the thread that owns the app's run loop (`.setup()` does),
        /// because the handler hangs off the *application* event target rather than
        /// off a thread's message queue.
        pub fn start<F>(on_fire: F) -> std::io::Result<Self>
        where
            F: FnMut(Accelerator) + Send + 'static,
        {
            install_handler()
                .map_err(|error| std::io::Error::other(error.detail.clone()))?;
            *ON_FIRE.lock().unwrap_or_else(|e| e.into_inner()) = Some(Box::new(on_fire));
            Ok(Self {
                active: Mutex::new(None),
            })
        }

        /// Registers `wanted`, falling through `fallbacks` on conflict.
        pub fn acquire(&self, wanted: Accelerator, fallbacks: Vec<Accelerator>) -> Acquisition {
            let mut guard = self.active.lock().unwrap_or_else(|e| e.into_inner());
            let mut registrar = CarbonRegistrar {
                registration: guard.take(),
            };
            let outcome = acquire(&mut registrar, wanted, &fallbacks);
            *guard = registrar.registration;
            outcome
        }

        /// Registers whatever `settings.json` says, with the standard ladder. The
        /// parse + ladder + conflict walk is the shared free function, so macOS
        /// cannot drift from Windows.
        pub fn acquire_from_settings(&self, configured: &str) -> Acquisition {
            let mut guard = self.active.lock().unwrap_or_else(|e| e.into_inner());
            let mut registrar = CarbonRegistrar {
                registration: guard.take(),
            };
            let outcome = super::acquire_from_settings(&mut registrar, configured);
            *guard = registrar.registration;
            outcome
        }

        /// Unregisters the active chord (used when the user turns the hotkey off).
        pub fn clear(&self) {
            let mut guard = self.active.lock().unwrap_or_else(|e| e.into_inner());
            let mut registrar = CarbonRegistrar {
                registration: guard.take(),
            };
            registrar.release();
            *guard = registrar.registration;
        }

        /// The chord currently registered, for the measurement probe.
        pub fn active(&self) -> Option<Accelerator> {
            self.active
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .map(|registration| registration.accelerator)
        }
    }

    impl Drop for Hotkeys {
        fn drop(&mut self) {
            let mut guard = self.active.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(registration) = guard.take() {
                // SAFETY: as in `CarbonRegistrar::release`.
                unsafe {
                    UnregisterEventHotKey(registration.reference as EventHotKeyRef);
                }
            }
            *ON_FIRE.lock().unwrap_or_else(|e| e.into_inner()) = None;
        }
    }
}

#[cfg(target_os = "macos")]
pub use macos::{Hotkeys as PlatformHotkeys};

/// Every other host (Linux CI and dev containers): no global hotkey, and said so.
///
/// This is the *only* remaining `Failed` platform, and it is honest about why — a
/// settings panel that claimed "registered" here would be lying about a chord no
/// desktop will deliver.
#[cfg(not(any(windows, target_os = "macos")))]
mod other {
    use super::*;

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
                    detail: "this platform cannot register a global hotkey".into(),
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

#[cfg(not(any(windows, target_os = "macos")))]
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

    // ---- the macOS half: everything measurable without a Mac ---------------

    #[test]
    fn carbon_masks_are_the_hitoolbox_constants() {
        let accel = accelerator::parse("Ctrl+Alt+Shift+D").unwrap();
        assert_eq!(
            carbon_modifiers(accel),
            CARBON_CONTROL | CARBON_OPTION | CARBON_SHIFT,
            "a chord with Ctrl+Alt+Shift must be Control+Option+Shift on a Mac"
        );
        // The exact literal matters: a wrong constant would register a different
        // chord silently.
        assert_eq!(carbon_modifiers(accel), 0x1A00);

        let with_command = accelerator::parse("Ctrl+Alt+Shift+Win+D").unwrap();
        assert_eq!(
            carbon_modifiers(with_command),
            0x1A00 | CARBON_CMD,
            "Win/Meta is Command on a Mac"
        );
        // Each modifier on its own, so a swapped pair cannot cancel out.
        assert_eq!(carbon_modifiers(accelerator::parse("Ctrl+D").unwrap()), 0x1000);
        assert_eq!(carbon_modifiers(accelerator::parse("Alt+D").unwrap()), 0x0800);
        assert_eq!(carbon_modifiers(accelerator::parse("Win+D").unwrap()), 0x0100);
        // Shift alone cannot parse, so its mask is read from a valid chord.
        assert_eq!(
            carbon_modifiers(accelerator::parse("Ctrl+Shift+D").unwrap()) & CARBON_SHIFT,
            CARBON_SHIFT
        );
    }

    #[test]
    fn the_default_chord_is_kvk_ansi_d_with_control_option_shift() {
        let accel = accelerator::parse("Ctrl+Alt+Shift+D").unwrap();
        assert_eq!(carbon_key_code(accel.key), Some(0x02), "kVK_ANSI_D");
        assert_eq!(carbon_key_code(Key::Escape), Some(0x35), "kVK_Escape");
        assert_eq!(carbon_key_code(Key::Space), Some(0x31), "kVK_Space");
        assert_eq!(carbon_key_code(Key::Function(1)), Some(0x7A), "kVK_F1");
        assert_eq!(carbon_key_code(Key::Digit(0)), Some(0x1D), "kVK_ANSI_0");
        assert_eq!(carbon_key_code(Key::Up), Some(0x7E), "kVK_UpArrow");
        assert_eq!(carbon_key_code(Key::Delete), Some(0x75), "kVK_ForwardDelete");
        assert_eq!(carbon_key_code(Key::Backspace), Some(0x33), "kVK_Delete");
    }

    #[test]
    fn every_key_macos_has_gets_its_own_code() {
        // 80-odd table entries; a duplicated code would make two different chords
        // the same chord, and a wrong one would fire a different key. Both are
        // caught here rather than on a user's Mac.
        let mut keys: Vec<Key> = Vec::new();
        for letter in 'A'..='Z' {
            keys.push(Key::Letter(letter));
        }
        for digit in 0..=9u8 {
            keys.push(Key::Digit(digit));
        }
        for function in 1..=20u8 {
            keys.push(Key::Function(function));
        }
        keys.extend([
            Key::Space,
            Key::Enter,
            Key::Escape,
            Key::Tab,
            Key::Backspace,
            Key::Delete,
            Key::Home,
            Key::End,
            Key::PageUp,
            Key::PageDown,
            Key::Up,
            Key::Down,
            Key::Left,
            Key::Right,
        ]);

        let mut seen = std::collections::HashMap::new();
        for key in keys {
            let code = carbon_key_code(key).unwrap_or_else(|| panic!("{key:?} has no code"));
            if let Some(previous) = seen.insert(code, key) {
                panic!("{key:?} and {previous:?} both map to kVK 0x{code:02X}");
            }
        }
        assert!(
            seen.len() >= 70,
            "expected the whole macOS key table, mapped {} keys",
            seen.len()
        );
    }

    #[test]
    fn keys_that_do_not_exist_on_a_mac_are_refused_rather_than_guessed() {
        // macOS has no Insert key and no F21-F24: reporting `None` is what makes the
        // registrar say "this chord cannot be used" instead of registering something
        // the user can never press.
        assert_eq!(carbon_key_code(Key::Insert), None);
        for function in 21..=24u8 {
            assert_eq!(carbon_key_code(Key::Function(function)), None, "F{function}");
        }
    }

    #[test]
    fn a_taken_mac_chord_is_classified_as_a_conflict_and_nothing_else_is() {
        let taken = classify_carbon_status(EVENT_HOTKEY_EXISTS_ERR, "RegisterEventHotKey");
        assert!(taken.is_conflict(), "eventHotKeyExistsErr must be a conflict");
        assert_eq!(taken.code, CONFLICT_CODE, "the shared 1409 code");
        assert!(taken.detail.contains("already owns"), "{}", taken.detail);
        assert!(taken.detail.contains("-9878"), "keep the OS status: {}", taken.detail);

        let bad = classify_carbon_status(PARAM_ERR, "RegisterEventHotKey");
        assert!(!bad.is_conflict(), "paramErr must stop the ladder, not walk it");
        assert_eq!(bad.code, 50, "the code survives as an absolute value");
        assert!(bad.detail.contains("-50"), "{}", bad.detail);

        let other = classify_carbon_status(-1, "InstallEventHandler");
        assert!(!other.is_conflict());
        assert_eq!(other.code, 1);
        assert!(other.detail.contains("InstallEventHandler"));
    }

    #[test]
    fn a_mac_conflict_walks_the_same_ladder_as_a_windows_one() {
        // A registrar that refuses the first chord with a Carbon conflict — the
        // whole point of mapping -9878 onto the shared code.
        struct MacStub {
            first: String,
            asked: Vec<String>,
        }
        impl Registrar for MacStub {
            fn register(&mut self, accelerator: Accelerator) -> Result<(), RegError> {
                let name = accelerator.canonical();
                self.asked.push(name.clone());
                if name == self.first {
                    return Err(classify_carbon_status(
                        EVENT_HOTKEY_EXISTS_ERR,
                        "RegisterEventHotKey",
                    ));
                }
                Ok(())
            }
            fn unregister(&mut self, _accelerator: Accelerator) {}
        }

        let wanted = default_hotkey();
        let mut stub = MacStub {
            first: wanted.canonical(),
            asked: Vec::new(),
        };
        let outcome = acquire(&mut stub, wanted, &accelerator::conflict_ladder(wanted));
        let Acquisition::Registered {
            accelerator,
            attempts,
        } = outcome
        else {
            panic!("a Mac conflict must fall through to the next rung, got {outcome:?}");
        };
        assert_ne!(accelerator, wanted, "the taken chord is never claimed");
        assert_eq!(attempts.len(), 1);
        assert_eq!(attempts[0].code, CONFLICT_CODE);
        assert_eq!(
            stub.asked.len(),
            2,
            "asked for the wanted chord then one rung: {:?}",
            stub.asked
        );
    }
}
