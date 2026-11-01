//! Hotkey accelerators: parsing, canonical spelling, and the conflict ladder.
//!
//! `ARCHITECTURE.md` §15 stores the hotkey as a string (`"Ctrl+Alt+Shift+D"`), so the
//! string is the source of truth in `settings.json`. Everything the OS needs is
//! derived here: a `win32`-compatible modifier mask and virtual-key code. The module
//! is deliberately platform-free (`u32` masks, no `windows` types) so it can be
//! tested on any runner and so the Windows registrar stays a thin wrapper.
//!
//! `SPEC.md` §2 only promises "a global hotkey". The rules below are what makes an
//! accelerator *registrable*: it must have at least one non-shift modifier (a
//! shift-only chord collides with typing), it must not be a Windows-reserved
//! combination, and it must name exactly one key.

use std::fmt;

/// `MOD_ALT`
pub const MOD_ALT: u32 = 0x0001;
/// `MOD_CONTROL`
pub const MOD_CONTROL: u32 = 0x0002;
/// `MOD_SHIFT`
pub const MOD_SHIFT: u32 = 0x0004;
/// `MOD_WIN`
pub const MOD_WIN: u32 = 0x0008;
/// `MOD_NOREPEAT` — hold the chord down and the wheel must not retrigger.
pub const MOD_NOREPEAT: u32 = 0x4000;

/// The key half of an accelerator, restricted to keys that are meaningful as a
/// global chord. Letters are stored uppercase so the canonical form is stable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    /// `A`-`Z`
    Letter(char),
    /// `0`-`9`
    Digit(u8),
    /// `F1`-`F24`
    Function(u8),
    Space,
    Enter,
    Escape,
    Tab,
    Backspace,
    Insert,
    Delete,
    Home,
    End,
    PageUp,
    PageDown,
    Up,
    Down,
    Left,
    Right,
}

impl Key {
    /// Virtual-key code (`VK_*`), the value `RegisterHotKey` expects.
    pub fn vk(self) -> u32 {
        match self {
            Key::Letter(c) => c as u32,      // 'A' == 0x41 == VK_A
            Key::Digit(d) => 0x30 + d as u32, // '0' == 0x30
            Key::Function(n) => 0x70 + (n as u32 - 1), // VK_F1 == 0x70
            Key::Backspace => 0x08,
            Key::Tab => 0x09,
            Key::Enter => 0x0D,
            Key::Escape => 0x1B,
            Key::Space => 0x20,
            Key::PageUp => 0x21,
            Key::PageDown => 0x22,
            Key::End => 0x23,
            Key::Home => 0x24,
            Key::Left => 0x25,
            Key::Up => 0x26,
            Key::Right => 0x27,
            Key::Down => 0x28,
            Key::Insert => 0x2D,
            Key::Delete => 0x2E,
        }
    }

    fn name(self) -> String {
        match self {
            Key::Letter(c) => c.to_string(),
            Key::Digit(d) => d.to_string(),
            Key::Function(n) => format!("F{n}"),
            Key::Space => "Space".into(),
            Key::Enter => "Enter".into(),
            Key::Escape => "Esc".into(),
            Key::Tab => "Tab".into(),
            Key::Backspace => "Backspace".into(),
            Key::Insert => "Insert".into(),
            Key::Delete => "Delete".into(),
            Key::Home => "Home".into(),
            Key::End => "End".into(),
            Key::PageUp => "PageUp".into(),
            Key::PageDown => "PageDown".into(),
            Key::Up => "Up".into(),
            Key::Down => "Down".into(),
            Key::Left => "Left".into(),
            Key::Right => "Right".into(),
        }
    }

    /// The letter that this key would be if it were a letter, used by the conflict
    /// ladder to walk a predictable sequence of alternatives.
    fn letter_or_none(self) -> Option<char> {
        match self {
            Key::Letter(c) => Some(c),
            _ => None,
        }
    }
}

/// A parsed accelerator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Accelerator {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub meta: bool,
    pub key: Key,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    Empty,
    /// A token that is not a modifier or a known key, quoted verbatim.
    UnknownToken(String),
    /// No key after the modifiers (`"Ctrl+Alt+"`).
    MissingKey,
    /// Two keys (`"Ctrl+A+B"`).
    TooManyKeys(String),
    /// Shift-only, or no modifier at all: this would fire while typing.
    NoModifier,
    /// `Ctrl+Alt+Del` and friends are reserved by the OS and can never be taken.
    Reserved(String),
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseError::Empty => write!(f, "the hotkey is empty"),
            ParseError::UnknownToken(token) => {
                write!(f, "{token:?} is not a key or modifier this app understands")
            }
            ParseError::MissingKey => write!(f, "the hotkey names no key"),
            ParseError::TooManyKeys(token) => {
                write!(f, "the hotkey names more than one key ({token})")
            }
            ParseError::NoModifier => write!(
                f,
                "the hotkey needs Ctrl, Alt or Cmd held with it, otherwise it fires while typing"
            ),
            ParseError::Reserved(what) => {
                write!(f, "{what} is reserved by the system and cannot be used")
            }
        }
    }
}

impl std::error::Error for ParseError {}

impl Accelerator {
    /// The bitmask `RegisterHotKey` wants (`MOD_*`).
    pub fn modifiers(self) -> u32 {
        let mut mask = 0;
        if self.alt {
            mask |= MOD_ALT;
        }
        if self.ctrl {
            mask |= MOD_CONTROL;
        }
        if self.shift {
            mask |= MOD_SHIFT;
        }
        if self.meta {
            mask |= MOD_WIN;
        }
        mask
    }

    /// `modifiers() | MOD_NOREPEAT`, what production actually registers.
    pub fn register_mask(self) -> u32 {
        self.modifiers() | MOD_NOREPEAT
    }

    pub fn vk(self) -> u32 {
        self.key.vk()
    }

    /// Stable spelling used in settings and in log lines: `Ctrl+Alt+Shift+D`, with
    /// modifiers always in Windows' order (Ctrl, Alt, Shift, Win).
    pub fn canonical(self) -> String {
        let mut parts: Vec<String> = Vec::new();
        if self.ctrl {
            parts.push("Ctrl".into());
        }
        if self.alt {
            parts.push("Alt".into());
        }
        if self.shift {
            parts.push("Shift".into());
        }
        if self.meta {
            parts.push("Win".into());
        }
        parts.push(self.key.name());
        parts.join("+")
    }

    fn with_key(self, key: Key) -> Self {
        Self { key, ..self }
    }

    fn with_shift(self, shift: bool) -> Self {
        Self { shift, ..self }
    }
}

impl fmt::Display for Accelerator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.canonical())
    }
}

/// Parses `Ctrl+Alt+Shift+D`, `ctrl+shift+f2`, `Cmd+Space`, … Case-insensitive,
/// `+`-separated, modifier order irrelevant, duplicate modifiers tolerated.
pub fn parse(input: &str) -> Result<Accelerator, ParseError> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(ParseError::Empty);
    }

    let (mut ctrl, mut alt, mut shift, mut meta) = (false, false, false, false);
    let mut key: Option<Key> = None;

    for raw in trimmed.split('+') {
        let token = raw.trim();
        if token.is_empty() {
            return Err(ParseError::MissingKey);
        }
        let lower = token.to_ascii_lowercase();
        match lower.as_str() {
            "ctrl" | "control" | "ctl" => ctrl = true,
            "alt" | "option" | "opt" => alt = true,
            "shift" => shift = true,
            "win" | "meta" | "cmd" | "command" | "super" => meta = true,
            _ => {
                let parsed = parse_key(token).ok_or_else(|| ParseError::UnknownToken(token.into()))?;
                if let Some(existing) = key {
                    return Err(ParseError::TooManyKeys(format!(
                        "{} and {}",
                        existing.name(),
                        parsed.name()
                    )));
                }
                key = Some(parsed);
            }
        }
    }

    let key = key.ok_or(ParseError::MissingKey)?;
    let accelerator = Accelerator {
        ctrl,
        alt,
        shift,
        meta,
        key,
    };

    // A chord with no Ctrl/Alt/Cmd would fire while the user types, so it is not a
    // hotkey we are willing to register.
    if !(accelerator.ctrl || accelerator.alt || accelerator.meta) {
        return Err(ParseError::NoModifier);
    }
    // Ctrl+Alt+Del is the secure attention sequence; the OS reserves the others.
    if accelerator.ctrl && accelerator.alt && accelerator.key == Key::Delete {
        return Err(ParseError::Reserved("Ctrl+Alt+Del".into()));
    }

    Ok(accelerator)
}

fn parse_key(token: &str) -> Option<Key> {
    let upper = token.to_ascii_uppercase();
    if upper.len() == 1 {
        let c = upper.chars().next()?;
        if c.is_ascii_uppercase() {
            return Some(Key::Letter(c));
        }
        if let Some(d) = c.to_digit(10) {
            return Some(Key::Digit(d as u8));
        }
        return None;
    }
    if let Some(rest) = upper.strip_prefix('F') {
        if let Ok(n) = rest.parse::<u8>() {
            if (1..=24).contains(&n) {
                return Some(Key::Function(n));
            }
            return None;
        }
    }
    match upper.as_str() {
        "SPACE" => Some(Key::Space),
        "ENTER" | "RETURN" => Some(Key::Enter),
        "ESC" | "ESCAPE" => Some(Key::Escape),
        "TAB" => Some(Key::Tab),
        "BACKSPACE" => Some(Key::Backspace),
        "INSERT" | "INS" => Some(Key::Insert),
        "DELETE" | "DEL" => Some(Key::Delete),
        "HOME" => Some(Key::Home),
        "END" => Some(Key::End),
        "PAGEUP" | "PGUP" => Some(Key::PageUp),
        "PAGEDOWN" | "PGDN" => Some(Key::PageDown),
        "UP" => Some(Key::Up),
        "DOWN" => Some(Key::Down),
        "LEFT" => Some(Key::Left),
        "RIGHT" => Some(Key::Right),
        _ => None,
    }
}

/// The alternatives tried, in order, when the wanted chord is already taken.
///
/// Deliberately predictable rather than clever: first the same chord with Shift
/// toggled, then the same chord walking a fixed letter sequence from the wanted key.
/// A user who sees "Ctrl+Alt+Shift+D was taken; using Ctrl+Alt+Shift+E" can act on
/// it; a random pick cannot be reproduced.
pub fn conflict_ladder(wanted: Accelerator) -> Vec<Accelerator> {
    const LETTERS: [char; 7] = ['D', 'E', 'F', 'G', 'H', 'J', 'K'];
    let mut ladder: Vec<Accelerator> = Vec::new();

    let shifted = wanted.with_shift(!wanted.shift);
    if shifted != wanted {
        ladder.push(shifted);
    }

    let start = match wanted.key.letter_or_none() {
        Some(c) => LETTERS.iter().position(|l| *l == c).unwrap_or(0),
        None => 0,
    };
    for offset in 0..LETTERS.len() {
        let letter = LETTERS[(start + offset) % LETTERS.len()];
        let candidate = wanted.with_key(Key::Letter(letter));
        if candidate != wanted && !ladder.contains(&candidate) {
            ladder.push(candidate);
        }
    }

    // `wanted` itself is never in the ladder: the caller tries it first.
    ladder.retain(|candidate| *candidate != wanted);
    ladder
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_hotkey_parses_and_round_trips() {
        let accel = parse("Ctrl+Alt+Shift+D").expect("default hotkey must parse");
        assert!(accel.ctrl && accel.alt && accel.shift && !accel.meta);
        assert_eq!(accel.key, Key::Letter('D'));
        assert_eq!(accel.vk(), 0x44);
        assert_eq!(accel.canonical(), "Ctrl+Alt+Shift+D");
        // The string in settings.json must survive a parse/print cycle unchanged.
        assert_eq!(parse(&accel.canonical()).unwrap(), accel);
    }

    #[test]
    fn modifier_masks_match_the_win32_constants() {
        let accel = parse("Ctrl+Alt+Shift+Win+D").unwrap();
        assert_eq!(accel.modifiers(), MOD_ALT | MOD_CONTROL | MOD_SHIFT | MOD_WIN);
        assert_eq!(accel.register_mask() & MOD_NOREPEAT, MOD_NOREPEAT);
        assert_eq!(accel.register_mask() & !MOD_NOREPEAT, accel.modifiers());
    }

    #[test]
    fn spelling_is_case_insensitive_and_order_irrelevant() {
        for input in [
            "ctrl+alt+shift+d",
            "Shift+Alt+Ctrl+D",
            "  CTRL + ALT + SHIFT + d  ",
            "Control+Option+Shift+d",
        ] {
            let accel = parse(input).unwrap_or_else(|e| panic!("{input:?}: {e}"));
            assert_eq!(accel.canonical(), "Ctrl+Alt+Shift+D", "input {input:?}");
        }
    }

    #[test]
    fn keys_have_the_virtual_key_codes_register_hot_key_expects() {
        assert_eq!(parse("Ctrl+F1").unwrap().vk(), 0x70);
        assert_eq!(parse("Ctrl+F24").unwrap().vk(), 0x87);
        assert_eq!(parse("Ctrl+0").unwrap().vk(), 0x30);
        assert_eq!(parse("Ctrl+Space").unwrap().vk(), 0x20);
        assert_eq!(parse("Ctrl+Esc").unwrap().vk(), 0x1B);
        assert_eq!(parse("Ctrl+PageDown").unwrap().vk(), 0x22);
    }

    #[test]
    fn a_shift_only_chord_is_refused_because_it_would_fire_while_typing() {
        assert_eq!(parse("Shift+D"), Err(ParseError::NoModifier));
        assert_eq!(parse("D"), Err(ParseError::NoModifier));
    }

    #[test]
    fn ctrl_alt_del_is_refused_as_reserved() {
        assert!(matches!(
            parse("Ctrl+Alt+Delete"),
            Err(ParseError::Reserved(_))
        ));
    }

    #[test]
    fn malformed_input_names_the_offending_token() {
        assert_eq!(parse(""), Err(ParseError::Empty));
        assert_eq!(parse("Ctrl+Alt+"), Err(ParseError::MissingKey));
        assert_eq!(parse("Ctrl+Alt"), Err(ParseError::MissingKey));
        assert_eq!(
            parse("Ctrl+Alt+Banana"),
            Err(ParseError::UnknownToken("Banana".into()))
        );
        assert!(matches!(parse("Ctrl+A+B"), Err(ParseError::TooManyKeys(_))));
        assert_eq!(parse("Ctrl+F25"), Err(ParseError::UnknownToken("F25".into())));
    }

    #[test]
    fn the_conflict_ladder_never_repeats_the_wanted_chord_and_is_deterministic() {
        let wanted = parse("Ctrl+Alt+Shift+D").unwrap();
        let ladder = conflict_ladder(wanted);
        assert!(!ladder.is_empty());
        assert!(!ladder.contains(&wanted));
        assert_eq!(ladder, conflict_ladder(wanted), "must be deterministic");
        // First rung: same chord, Shift toggled off.
        assert_eq!(ladder[0].canonical(), "Ctrl+Alt+D");
        // Then the letter walk, starting at the wanted key: D is the wanted chord, so
        // the first letter candidate is E.
        assert_eq!(ladder[1].canonical(), "Ctrl+Alt+Shift+E");
        let mut seen = std::collections::HashSet::new();
        for candidate in &ladder {
            assert!(seen.insert(candidate.canonical()), "duplicate rung {candidate}");
        }
    }

    #[test]
    fn every_rung_of_the_ladder_is_itself_a_valid_accelerator() {
        let wanted = parse("Ctrl+Shift+K").unwrap();
        for candidate in conflict_ladder(wanted) {
            // Canonical spelling must parse back to the same accelerator, or a
            // fallback that the OS accepted could not be persisted.
            assert_eq!(parse(&candidate.canonical()).unwrap(), candidate);
        }
    }
}
