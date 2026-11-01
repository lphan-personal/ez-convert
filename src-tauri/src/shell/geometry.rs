//! Where the wheel goes when the hotkey fires (`ARCHITECTURE.md` §10.2, `SPEC.md` §2).
//!
//! The S1 spike clamped the window to the **virtual screen** with a lower bound of
//! `0`, which is only correct when the primary monitor is the leftmost/topmost one.
//! A monitor to the left of the primary occupies negative virtual coordinates, so a
//! summon near its edge landed the wheel on the primary — and the clamp could not
//! express "fit inside the cursor's work area" at all (a maximised taskbar steals
//! the bottom strip). This module is the corrected rule, written once and used by
//! every caller:
//!
//! ```text
//! mon = MonitorFromPoint(cursor, MONITOR_DEFAULTTONEAREST)
//! GetMonitorInfoW(mon) -> rcMonitor, rcWork
//! x = clamp(cursor.x - W/2, rcWork.left, rcWork.right  - W)
//! y = clamp(cursor.y - H/2, rcWork.top,  rcWork.bottom - H)
//! ```
//!
//! Kept free of Tauri and of any OS call so the rule can be tested on a runner with
//! no window: the OS side only supplies the monitor list
//! (`shell/hotkey.rs::cursor_monitor`).

/// A rectangle in virtual-desktop coordinates. Windows' `RECT`, kept as a plain
/// struct so the rule is testable without an `HWND`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl Rect {
    pub const fn new(left: i32, top: i32, right: i32, bottom: i32) -> Self {
        Self {
            left,
            top,
            right,
            bottom,
        }
    }

    pub const fn from_origin(x: i32, y: i32, width: i32, height: i32) -> Self {
        Self {
            left: x,
            top: y,
            right: x + width,
            bottom: y + height,
        }
    }

    pub const fn width(&self) -> i32 {
        self.right - self.left
    }

    pub const fn height(&self) -> i32 {
        self.bottom - self.top
    }

    /// True when `(x, y)` is inside the half-open rectangle (`RECT` semantics: the
    /// right/bottom edges are outside).
    pub const fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.left && x < self.right && y >= self.top && y < self.bottom
    }

    /// `candidate` entirely inside `self`.
    pub fn encloses(&self, candidate: &Rect) -> bool {
        candidate.left >= self.left
            && candidate.top >= self.top
            && candidate.right <= self.right
            && candidate.bottom <= self.bottom
    }

    /// Distance from `(x, y)` to the rectangle: 0 inside, otherwise the Euclidean
    /// distance to the nearest point on the edge. This is the part of
    /// `MONITOR_DEFAULTTONEAREST` that is worth reproducing exactly — the monitor
    /// whose rectangle is closest to the cursor, which decides the "between two
    /// monitors" case deterministically.
    pub fn distance_to(&self, x: i32, y: i32) -> f64 {
        let dx = if x < self.left {
            (self.left - x) as f64
        } else if x >= self.right {
            (x - self.right + 1) as f64
        } else {
            0.0
        };
        let dy = if y < self.top {
            (self.top - y) as f64
        } else if y >= self.bottom {
            (y - self.bottom + 1) as f64
        } else {
            0.0
        };
        (dx * dx + dy * dy).sqrt()
    }
}

/// One monitor: its full rect and its work area (the full rect minus app bars and
/// the taskbar). Both are in virtual-desktop coordinates, so a monitor left of the
/// primary has negative `left`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Monitor {
    pub device: Rect,
    pub work: Rect,
}

impl Monitor {
    pub const fn new(device: Rect, work: Rect) -> Self {
        Self { device, work }
    }

    /// A monitor whose work area is its device rect (no taskbar).
    pub const fn plain(left: i32, top: i32, right: i32, bottom: i32) -> Self {
        let rect = Rect::new(left, top, right, bottom);
        Self {
            device: rect,
            work: rect,
        }
    }
}

/// Index of the monitor closest to `(x, y)`; `None` only for an empty layout.
///
/// Ties go to the earlier monitor, which is what makes the result deterministic when
/// the cursor sits exactly on a shared edge.
pub fn monitor_at(monitors: &[Monitor], x: i32, y: i32) -> Option<usize> {
    let mut best: Option<(usize, f64)> = None;
    for (index, monitor) in monitors.iter().enumerate() {
        let distance = monitor.device.distance_to(x, y);
        if best.map(|(_, d)| distance < d).unwrap_or(true) {
            best = Some((index, distance));
        }
    }
    best.map(|(index, _)| index)
}

/// The wheel rect for a cursor position on one monitor.
///
/// Centres the wheel on the cursor, then clamps it into the monitor's **work area**
/// — never the virtual screen, never with a `0` lower bound. When the wheel is larger
/// than the work area it is pinned to the work-area origin and the caller is
/// responsible for the (logged) overflow; shrinking the wheel is a UI decision, not
/// a placement one.
pub fn place_on_monitor(cursor: (i32, i32), monitor: &Monitor, size: (i32, i32)) -> Rect {
    let width = size.0.max(1);
    let height = size.1.max(1);
    let work = monitor.work;

    let max_left = work.right - width;
    let max_top = work.bottom - height;

    let left = if max_left < work.left {
        work.left
    } else {
        (cursor.0 - width / 2).clamp(work.left, max_left)
    };
    let top = if max_top < work.top {
        work.top
    } else {
        (cursor.1 - height / 2).clamp(work.top, max_top)
    };

    Rect::from_origin(left, top, width, height)
}

/// The whole rule: pick the cursor's monitor, then place the wheel on it. `None`
/// only when the OS gave us no monitors at all.
pub fn place_on_layout(monitors: &[Monitor], cursor: (i32, i32), size: (i32, i32)) -> Option<Rect> {
    let index = monitor_at(monitors, cursor.0, cursor.1)?;
    Some(place_on_monitor(cursor, &monitors[index], size))
}

/// 520×520 is the S1-proven wheel size (`ARCHITECTURE.md` §10.1).
pub const WHEEL_SIZE: (i32, i32) = (520, 520);

#[cfg(test)]
mod tests {
    use super::*;

    /// Primary 1920×1080 at the origin; a second monitor to its **left** occupying
    /// negative virtual coordinates — the exact case the spike could not express.
    fn two_monitors_with_negative_origin() -> Vec<Monitor> {
        vec![
            Monitor::plain(0, 0, 1920, 1080),
            Monitor::plain(-1920, 0, 0, 1080),
        ]
    }

    #[test]
    fn a_summon_on_a_negative_coordinate_monitor_stays_on_that_monitor() {
        let monitors = two_monitors_with_negative_origin();
        // 100 px from the right edge of the left monitor, mid-height.
        let cursor = (-100, 540);
        let wheel = place_on_layout(&monitors, cursor, WHEEL_SIZE).expect("layout");

        assert!(
            (wheel.left..wheel.right).contains(&cursor.0),
            "wheel {:?} does not span the cursor x={}",
            wheel,
            cursor.0
        );
        assert!(
            wheel.right <= 0,
            "wheel {:?} crossed onto the primary monitor (right edge > 0)",
            wheel
        );
        assert_eq!(monitor_at(&monitors, cursor.0, cursor.1), Some(1));
    }

    #[test]
    fn the_cursor_is_the_centre_of_the_wheel_on_every_monitor() {
        let monitors = two_monitors_with_negative_origin();
        for monitors_index in 0..monitors.len() {
            let device = monitors[monitors_index].device;
            let cursor = (
                device.left + device.width() / 2,
                device.top + device.height() / 2,
            );
            let wheel = place_on_layout(&monitors, cursor, WHEEL_SIZE).expect("layout");
            assert_eq!(
                (wheel.left + wheel.width() / 2, wheel.top + wheel.height() / 2),
                cursor,
                "monitor {monitors_index}: wheel {:?} is not centred on cursor {:?}",
                wheel,
                cursor
            );
        }
    }

    #[test]
    fn five_pixels_from_every_edge_stays_inside_the_work_area() {
        let monitor = Monitor::new(
            Rect::new(0, 0, 1920, 1080),
            // Maximised taskbar: 40 px stolen from the bottom, as on the test VM.
            Rect::new(0, 0, 1920, 1040),
        );
        let corners = [
            (5, 5),
            (1915, 5),
            (5, 1035),
            (1915, 1035),
            (960, 1080 - 5), // over the taskbar strip
        ];
        for cursor in corners {
            let wheel = place_on_monitor(cursor, &monitor, WHEEL_SIZE);
            assert!(
                monitor.work.encloses(&wheel),
                "cursor {:?} -> wheel {:?} escapes work {:?}",
                cursor,
                wheel,
                monitor.work
            );
        }
    }

    #[test]
    fn the_taskbar_strip_is_never_covered() {
        let monitor = Monitor::new(
            Rect::new(0, 0, 1920, 1080),
            Rect::new(0, 0, 1920, 1040),
        );
        let wheel = place_on_monitor((960, 1079), &monitor, WHEEL_SIZE);
        assert_eq!(
            wheel.bottom, 1040,
            "wheel must stop at the work area's bottom edge"
        );
    }

    #[test]
    fn a_secondary_monitor_above_the_primary_places_upward() {
        // Secondary at negative y (a monitor placed above the primary).
        let monitors = vec![
            Monitor::plain(0, 0, 1920, 1080),
            Monitor::plain(0, -1080, 1920, 0),
        ];
        let cursor = (900, -80);
        let wheel = place_on_layout(&monitors, cursor, WHEEL_SIZE).expect("layout");
        assert!(wheel.top >= -1080 && wheel.bottom <= 0, "wheel {:?}", wheel);
        assert_eq!(monitor_at(&monitors, cursor.0, cursor.1), Some(1));
    }

    #[test]
    fn a_monitor_to_the_right_keeps_positive_virtual_coordinates() {
        let monitors = vec![
            Monitor::plain(0, 0, 1920, 1080),
            Monitor::plain(1920, 0, 3840, 1080),
        ];
        let wheel = place_on_layout(&monitors, (1930, 100), WHEEL_SIZE).expect("layout");
        assert!(wheel.left >= 1920, "wheel {:?} leaked onto the primary", wheel);
    }

    #[test]
    fn a_wheel_larger_than_the_work_area_is_pinned_to_its_origin_not_centred() {
        let monitor = Monitor::plain(100, 100, 400, 400); // 300×300 work area
        let wheel = place_on_monitor((250, 250), &monitor, (520, 520));
        assert_eq!((wheel.left, wheel.top), (100, 100));
        assert_eq!((wheel.width(), wheel.height()), (520, 520));
    }

    #[test]
    fn between_two_monitors_the_nearest_one_wins() {
        let monitors = two_monitors_with_negative_origin();
        // Exactly on the shared edge: the tie goes to the earlier monitor.
        assert_eq!(monitor_at(&monitors, 0, 500), Some(0));
        // One pixel into the left monitor.
        assert_eq!(monitor_at(&monitors, -1, 500), Some(1));
        // Far outside any monitor (cursor pinned to a corner by a game): nearest.
        assert_eq!(monitor_at(&monitors, -5000, 500), Some(1));
        assert_eq!(monitor_at(&monitors, 5000, 500), Some(0));
    }

    #[test]
    fn an_empty_layout_places_nothing() {
        assert!(place_on_layout(&[], (0, 0), WHEEL_SIZE).is_none());
    }

    #[test]
    fn a_one_pixel_wheel_is_still_a_rect() {
        // Defensive: a caller that passes 0 must not produce a zero-width rect that
        // later reads as "no window".
        let monitor = Monitor::plain(0, 0, 100, 100);
        let wheel = place_on_monitor((50, 50), &monitor, (0, 0));
        assert_eq!((wheel.width(), wheel.height()), (1, 1));
    }
}
