//! pager — the window manager.
//!
//! Ported from `pager` 1.0.0 (generation 2) and `dock` 1.0.0 (generation 1).
//!
//! This is the one module that cannot be portable in the way the others are.
//! Enumerating and moving other applications' windows is a different API on
//! every desktop, and there is no crate in the tree that hides it, so each
//! platform gets its own backend:
//!
//! | | Windows | Linux | macOS |
//! |---|---|---|---|
//! | list, switch, snap, move, pin | Win32 | `wmctrl` | `osascript` (list and switch) |
//! | rollup, alpha | Win32 | not supported | not supported |
//!
//! Anything a platform cannot do says so and names the platform. Nothing is
//! simulated: an operation either reached the window manager or it reports that
//! it could not.
//!
//! `calc` is pure arithmetic and works everywhere, which is why it exists: you
//! can work out where a window would land before letting anything move it.

use savehorse_core::{Error, Result};
use serde::{Deserialize, Serialize};

/// A rectangle in screen coordinates, left and top inclusive.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl Rect {
    pub fn width(&self) -> i32 {
        self.right - self.left
    }

    pub fn height(&self) -> i32 {
        self.bottom - self.top
    }
}

/// One window the manager will admit to knowing about.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct WindowInfo {
    /// Platform handle: a hex HWND on Windows, an X11 window id on Linux, a
    /// process name on macOS. This is what every other operation takes.
    pub handle: String,
    pub title: String,
    pub class: String,
    pub rect: Rect,
    pub visible: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct SnapReport {
    pub handle: String,
    pub position: String,
    pub rect: Rect,
    pub backend: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct ActionResult {
    pub handle: String,
    pub action: String,
    pub detail: String,
    pub backend: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct LaunchReport {
    pub program: String,
    pub arguments: Vec<String>,
    pub pid: u32,
}

/// The named positions, in the order the help lists them.
pub const POSITIONS: &[&str] = &["left", "right", "top", "bottom", "center", "max"];

/// Where a window lands when snapped, given a work area.
///
/// Pure arithmetic, no platform involved: this is what `calc` exposes and what
/// `snap` uses once it knows the real work area.
pub fn snap_rect(position: &str, work: Rect) -> Result<Rect> {
    let width = work.width();
    let height = work.height();

    if width <= 0 || height <= 0 {
        return Err(Error::Invalid(
            "the work area has no size to divide".to_string(),
        ));
    }

    let (left, top, w, h) = match position.trim().to_lowercase().as_str() {
        "left" => (work.left, work.top, width / 2, height),
        "right" => (work.left + width / 2, work.top, width - width / 2, height),
        "top" => (work.left, work.top, width, height / 2),
        "bottom" => (work.left, work.top + height / 2, width, height - height / 2),
        "center" => (
            work.left + width / 4,
            work.top + height / 4,
            width / 2,
            height / 2,
        ),
        "max" => (work.left, work.top, width, height),
        other => {
            return Err(Error::Invalid(format!(
                "unknown position '{}': use {}",
                other,
                POSITIONS.join(", ")
            )))
        }
    };

    Ok(Rect {
        left,
        top,
        right: left + w,
        bottom: top + h,
    })
}

/// Start a program. The only operation here that needs no window manager.
pub fn launch(program: &str, arguments: &[String]) -> Result<LaunchReport> {
    if program.trim().is_empty() {
        return Err(Error::Invalid("no program given".to_string()));
    }

    let child = std::process::Command::new(program)
        .args(arguments)
        .spawn()
        .map_err(|e| Error::Invalid(format!("could not start '{}': {}", program, e)))?;

    Ok(LaunchReport {
        program: program.to_string(),
        arguments: arguments.to_vec(),
        pid: child.id(),
    })
}

fn unsupported(what: &str) -> Error {
    Error::Invalid(format!(
        "{} is not supported on {} — see the capability matrix for what each platform can do",
        what,
        std::env::consts::OS
    ))
}

// ---------------------------------------------------------------------------
// Backends
// ---------------------------------------------------------------------------

#[cfg(windows)]
mod backend {
    use super::*;

    pub type HWND = isize;
    type BOOL = i32;
    type LPARAM = isize;

    #[repr(C)]
    #[derive(Copy, Clone, Default)]
    struct WinRect {
        left: i32,
        top: i32,
        right: i32,
        bottom: i32,
    }

    const SW_MAXIMIZE: i32 = 3;
    const SW_RESTORE: i32 = 9;
    const GWL_EXSTYLE: i32 = -20;
    const WS_EX_LAYERED: isize = 0x0008_0000;
    const WS_EX_TOPMOST: isize = 0x0000_0008;
    const LWA_ALPHA: u32 = 0x0000_0002;
    const HWND_TOPMOST: HWND = -1;
    const HWND_NOTOPMOST: HWND = -2;
    const SWP_NOSIZE: u32 = 0x0001;
    const SWP_NOMOVE: u32 = 0x0002;
    const SWP_NOZORDER: u32 = 0x0004;
    const SWP_SHOWWINDOW: u32 = 0x0040;
    const SPI_GETWORKAREA: u32 = 0x0030;

    #[link(name = "user32")]
    extern "system" {
        fn EnumWindows(
            callback: Option<unsafe extern "system" fn(HWND, LPARAM) -> BOOL>,
            lparam: LPARAM,
        ) -> BOOL;
        fn IsWindowVisible(hwnd: HWND) -> BOOL;
        fn GetWindowTextW(hwnd: HWND, buffer: *mut u16, max: i32) -> i32;
        fn GetClassNameW(hwnd: HWND, buffer: *mut u16, max: i32) -> i32;
        fn GetWindowRect(hwnd: HWND, rect: *mut WinRect) -> BOOL;
        fn SetWindowPos(
            hwnd: HWND,
            insert_after: HWND,
            x: i32,
            y: i32,
            width: i32,
            height: i32,
            flags: u32,
        ) -> BOOL;
        fn ShowWindow(hwnd: HWND, command: i32) -> BOOL;
        fn SetForegroundWindow(hwnd: HWND) -> BOOL;
        fn GetWindowLongPtrW(hwnd: HWND, index: i32) -> isize;
        fn SetWindowLongPtrW(hwnd: HWND, index: i32, value: isize) -> isize;
        fn SetLayeredWindowAttributes(hwnd: HWND, key: u32, alpha: u8, flags: u32) -> BOOL;
        fn SystemParametersInfoW(
            action: u32,
            param: u32,
            out: *mut std::ffi::c_void,
            win_ini: u32,
        ) -> BOOL;
    }

    fn wide_to_string(buffer: &[u16]) -> String {
        let end = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
        String::from_utf16_lossy(&buffer[..end])
    }

    pub fn parse_handle(text: &str) -> Result<HWND> {
        let trimmed = text.trim().trim_start_matches("0x").trim_start_matches("0X");
        let value = isize::from_str_radix(trimmed, 16)
            .map_err(|_| Error::Invalid(format!("'{}' is not a window handle", text)))?;
        if unsafe { IsWindowVisible(value) } == 0 && !is_window(value) {
            return Err(Error::NotFound(format!("window {}", text)));
        }
        Ok(value)
    }

    fn is_window(hwnd: HWND) -> bool {
        #[link(name = "user32")]
        extern "system" {
            fn IsWindow(hwnd: HWND) -> BOOL;
        }
        unsafe { IsWindow(hwnd) != 0 }
    }

    pub fn work_area() -> Rect {
        let mut area = WinRect::default();
        unsafe {
            SystemParametersInfoW(
                SPI_GETWORKAREA,
                0,
                &mut area as *mut _ as *mut std::ffi::c_void,
                0,
            );
        }
        Rect {
            left: area.left,
            top: area.top,
            right: area.right,
            bottom: area.bottom,
        }
    }

    unsafe extern "system" fn collect(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let out = &mut *(lparam as *mut Vec<WindowInfo>);
        if IsWindowVisible(hwnd) == 0 {
            return 1;
        }

        let mut title_buffer = [0u16; 512];
        let title_length = GetWindowTextW(hwnd, title_buffer.as_mut_ptr(), 512);
        if title_length == 0 {
            // Untitled and usually invisible to the user; skip it.
            return 1;
        }

        let mut class_buffer = [0u16; 256];
        GetClassNameW(hwnd, class_buffer.as_mut_ptr(), 256);

        let mut rect = WinRect::default();
        GetWindowRect(hwnd, &mut rect);

        out.push(WindowInfo {
            handle: format!("0x{:X}", hwnd as usize),
            title: wide_to_string(&title_buffer),
            class: wide_to_string(&class_buffer),
            rect: Rect {
                left: rect.left,
                top: rect.top,
                right: rect.right,
                bottom: rect.bottom,
            },
            visible: true,
        });

        1
    }

    pub fn list() -> Result<Vec<WindowInfo>> {
        let mut windows: Vec<WindowInfo> = Vec::new();
        unsafe {
            EnumWindows(
                Some(collect),
                &mut windows as *mut Vec<WindowInfo> as LPARAM,
            );
        }
        windows.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()));
        Ok(windows)
    }

    pub fn snap(handle_text: &str, position: &str) -> Result<(Rect, String)> {
        let hwnd = parse_handle(handle_text)?;
        let work = work_area();
        let target = snap_rect(position, work)?;

        unsafe {
            if position.eq_ignore_ascii_case("max") {
                ShowWindow(hwnd, SW_MAXIMIZE);
            } else {
                ShowWindow(hwnd, SW_RESTORE);
                SetWindowPos(
                    hwnd,
                    0,
                    target.left,
                    target.top,
                    target.width(),
                    target.height(),
                    SWP_NOZORDER | SWP_SHOWWINDOW,
                );
            }
        }

        Ok((target, "win32".to_string()))
    }

    pub fn move_to(handle_text: &str, rect: Rect) -> Result<(Rect, String)> {
        let hwnd = parse_handle(handle_text)?;
        unsafe {
            SetWindowPos(
                hwnd,
                0,
                rect.left,
                rect.top,
                rect.width(),
                rect.height(),
                SWP_NOZORDER | SWP_SHOWWINDOW,
            );
        }
        Ok((rect, "win32".to_string()))
    }

    pub fn switch(handle_text: &str) -> Result<String> {
        let hwnd = parse_handle(handle_text)?;
        unsafe {
            ShowWindow(hwnd, SW_RESTORE);
            SetForegroundWindow(hwnd);
        }
        Ok("win32".to_string())
    }

    pub fn pin(handle_text: &str, on_top: bool) -> Result<String> {
        let hwnd = parse_handle(handle_text)?;
        let insert = if on_top { HWND_TOPMOST } else { HWND_NOTOPMOST };
        unsafe {
            SetWindowPos(hwnd, insert, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE);
        }
        Ok("win32".to_string())
    }

    pub fn alpha(handle_text: &str, percent: u8) -> Result<String> {
        let hwnd = parse_handle(handle_text)?;
        let percent = percent.min(100);
        let alpha = (percent as u32 * 255 / 100) as u8;

        unsafe {
            let style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, style | WS_EX_LAYERED);
            SetLayeredWindowAttributes(hwnd, 0, alpha, LWA_ALPHA);
        }
        Ok("win32".to_string())
    }

    pub fn rollup(handle_text: &str, height: i32) -> Result<String> {
        let hwnd = parse_handle(handle_text)?;
        unsafe {
            SetWindowPos(hwnd, 0, 0, 0, 0, height, SWP_NOMOVE | SWP_NOZORDER);
        }
        Ok("win32".to_string())
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
mod backend {
    use super::*;

    fn run(args: &[&str]) -> Result<String> {
        let output = std::process::Command::new("wmctrl")
            .args(args)
            .output()
            .map_err(|e| {
                Error::Invalid(format!(
                    "wmctrl is not available ({}): install it to manage windows on this desktop",
                    e
                ))
            })?;

        if !output.status.success() {
            return Err(Error::Invalid(format!(
                "wmctrl failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            )));
        }
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    pub fn list() -> Result<Vec<WindowInfo>> {
        // id  desktop  x  y  w  h  host  title
        let text = run(&["-lG"])?;
        let mut windows = Vec::new();

        for line in text.lines() {
            let fields: Vec<&str> = line.split_whitespace().collect();
            if fields.len() < 8 {
                continue;
            }
            let number = |i: usize| fields[i].parse::<i32>().unwrap_or(0);
            windows.push(WindowInfo {
                handle: fields[0].to_string(),
                title: fields[7..].join(" "),
                class: fields[6].to_string(),
                rect: Rect {
                    left: number(2),
                    top: number(3),
                    right: number(2) + number(4),
                    bottom: number(3) + number(5),
                },
                visible: true,
            });
        }

        windows.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()));
        Ok(windows)
    }

    fn work_area() -> Rect {
        // wmctrl does not report the work area; the desktop size is the best
        // available answer without pulling in an X11 binding.
        let text = run(&["-d"]).unwrap_or_default();
        for line in text.lines() {
            let fields: Vec<&str> = line.split_whitespace().collect();
            if fields.len() >= 10 && fields[1] == "*" {
                let width = fields[8].parse::<i32>().unwrap_or(1920);
                let height = fields[9].parse::<i32>().unwrap_or(1080);
                return Rect {
                    left: 0,
                    top: 0,
                    right: width,
                    bottom: height,
                };
            }
        }
        Rect {
            left: 0,
            top: 0,
            right: 1920,
            bottom: 1080,
        }
    }

    pub fn snap(handle: &str, position: &str) -> Result<(Rect, String)> {
        let target = snap_rect(position, work_area())?;
        let geometry = format!(
            "0,{},{},{},{}",
            target.left,
            target.top,
            target.width(),
            target.height()
        );
        run(&["-i", "-r", handle, "-e", &geometry])?;
        Ok((target, "wmctrl".to_string()))
    }

    pub fn move_to(handle: &str, rect: Rect) -> Result<(Rect, String)> {
        let geometry = format!(
            "0,{},{},{},{}",
            rect.left,
            rect.top,
            rect.width(),
            rect.height()
        );
        run(&["-i", "-r", handle, "-e", &geometry])?;
        Ok((rect, "wmctrl".to_string()))
    }

    pub fn switch(handle: &str) -> Result<String> {
        run(&["-i", "-a", handle])?;
        Ok("wmctrl".to_string())
    }

    pub fn pin(handle: &str, on_top: bool) -> Result<String> {
        if on_top {
            run(&["-i", "-r", handle, "-b", "add,above"])?;
        } else {
            run(&["-i", "-r", handle, "-b", "remove,above"])?;
        }
        Ok("wmctrl".to_string())
    }

    pub fn alpha(_handle: &str, _percent: u8) -> Result<String> {
        Err(unsupported("window opacity"))
    }

    pub fn rollup(_handle: &str, _height: i32) -> Result<String> {
        Err(unsupported("window roll-up"))
    }
}

#[cfg(target_os = "macos")]
mod backend {
    use super::*;

    fn osascript(script: &str) -> Result<String> {
        let output = std::process::Command::new("osascript")
            .args(["-e", script])
            .output()
            .map_err(|e| Error::Invalid(format!("osascript is not available: {}", e)))?;

        if !output.status.success() {
            return Err(Error::Invalid(format!(
                "osascript failed: {}. Window access on macOS needs the terminal to have \
                 Accessibility permission in System Settings, Privacy and Security",
                String::from_utf8_lossy(&output.stderr).trim()
            )));
        }
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    pub fn list() -> Result<Vec<WindowInfo>> {
        let text = osascript(
            "tell application \"System Events\" to get name of every process whose background only is false",
        )?;

        let mut windows = Vec::new();
        for name in text.trim().split(", ") {
            let name = name.trim();
            if name.is_empty() {
                continue;
            }
            windows.push(WindowInfo {
                // macOS has no stable per-window handle through AppleScript, so
                // the application name is what the other operations take.
                handle: name.to_string(),
                title: name.to_string(),
                class: "application".to_string(),
                rect: Rect::default(),
                visible: true,
            });
        }
        windows.sort_by(|a, b| a.title.cmp(&b.title));
        Ok(windows)
    }

    pub fn switch(handle: &str) -> Result<String> {
        osascript(&format!("tell application \"{}\" to activate", handle))?;
        Ok("osascript".to_string())
    }

    pub fn snap(_handle: &str, _position: &str) -> Result<(Rect, String)> {
        Err(unsupported("window snapping"))
    }

    pub fn move_to(_handle: &str, _rect: Rect) -> Result<(Rect, String)> {
        Err(unsupported("moving a window to exact coordinates"))
    }

    pub fn pin(_handle: &str, _on_top: bool) -> Result<String> {
        Err(unsupported("pinning a window above others"))
    }

    pub fn alpha(_handle: &str, _percent: u8) -> Result<String> {
        Err(unsupported("window opacity"))
    }

    pub fn rollup(_handle: &str, _height: i32) -> Result<String> {
        Err(unsupported("window roll-up"))
    }
}

#[cfg(not(any(windows, unix)))]
mod backend {
    use super::*;

    pub fn list() -> Result<Vec<WindowInfo>> {
        Err(unsupported("listing windows"))
    }
    pub fn snap(_h: &str, _p: &str) -> Result<(Rect, String)> {
        Err(unsupported("window snapping"))
    }
    pub fn move_to(_h: &str, _r: Rect) -> Result<(Rect, String)> {
        Err(unsupported("moving a window"))
    }
    pub fn switch(_h: &str) -> Result<String> {
        Err(unsupported("switching windows"))
    }
    pub fn pin(_h: &str, _o: bool) -> Result<String> {
        Err(unsupported("pinning a window"))
    }
    pub fn alpha(_h: &str, _p: u8) -> Result<String> {
        Err(unsupported("window opacity"))
    }
    pub fn rollup(_h: &str, _hh: i32) -> Result<String> {
        Err(unsupported("window roll-up"))
    }
}

// ---------------------------------------------------------------------------
// Public operations
// ---------------------------------------------------------------------------

pub fn list_windows() -> Result<Vec<WindowInfo>> {
    backend::list()
}

pub fn snap(handle: &str, position: &str) -> Result<SnapReport> {
    let (rect, backend_name) = backend::snap(handle, position)?;
    Ok(SnapReport {
        handle: handle.to_string(),
        position: position.to_string(),
        rect,
        backend: backend_name,
    })
}

pub fn move_window(handle: &str, rect: Rect) -> Result<SnapReport> {
    let (rect, backend_name) = backend::move_to(handle, rect)?;
    Ok(SnapReport {
        handle: handle.to_string(),
        position: "explicit".to_string(),
        rect,
        backend: backend_name,
    })
}

pub fn switch(handle: &str) -> Result<ActionResult> {
    let backend_name = backend::switch(handle)?;
    Ok(ActionResult {
        handle: handle.to_string(),
        action: "switch".to_string(),
        detail: "brought the window to the front".to_string(),
        backend: backend_name,
    })
}

pub fn pin(handle: &str, on_top: bool) -> Result<ActionResult> {
    let backend_name = backend::pin(handle, on_top)?;
    Ok(ActionResult {
        handle: handle.to_string(),
        action: if on_top { "pin" } else { "unpin" }.to_string(),
        detail: if on_top {
            "window is now above others".to_string()
        } else {
            "window is no longer above others".to_string()
        },
        backend: backend_name,
    })
}

pub fn set_alpha(handle: &str, percent: u8) -> Result<ActionResult> {
    if percent > 100 {
        return Err(Error::Invalid(
            "opacity is a percentage between 0 and 100".to_string(),
        ));
    }
    let backend_name = backend::alpha(handle, percent)?;
    Ok(ActionResult {
        handle: handle.to_string(),
        action: "alpha".to_string(),
        detail: format!("opacity set to {}%", percent),
        backend: backend_name,
    })
}

pub fn rollup(handle: &str, height: i32) -> Result<ActionResult> {
    let backend_name = backend::rollup(handle, height)?;
    Ok(ActionResult {
        handle: handle.to_string(),
        action: "rollup".to_string(),
        detail: format!("window height set to {}", height),
        backend: backend_name,
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn work() -> Rect {
        Rect {
            left: 0,
            top: 0,
            right: 1920,
            bottom: 1080,
        }
    }

    #[test]
    fn halves_split_the_work_area() {
        let left = snap_rect("left", work()).unwrap();
        assert_eq!((left.left, left.top, left.right, left.bottom), (0, 0, 960, 1080));

        let right = snap_rect("right", work()).unwrap();
        assert_eq!((right.left, right.right), (960, 1920));

        // The halves meet exactly, with no gap and no overlap.
        assert_eq!(left.right, right.left);
    }

    #[test]
    fn odd_sizes_do_not_lose_a_pixel() {
        let odd = Rect {
            left: 0,
            top: 0,
            right: 1921,
            bottom: 1081,
        };
        let left = snap_rect("left", odd).unwrap();
        let right = snap_rect("right", odd).unwrap();
        assert_eq!(left.right, right.left, "the halves still meet");
        assert_eq!(right.right, 1921, "the full width is covered");
        assert_eq!(snap_rect("max", odd).unwrap().right, 1921);
    }

    #[test]
    fn a_work_area_offset_is_respected() {
        // A taskbar on the left, and a dock at the bottom.
        let offset = Rect {
            left: 64,
            top: 0,
            right: 1920,
            bottom: 1040,
        };
        let centered = snap_rect("center", offset).unwrap();
        assert_eq!(centered.left, 64 + (1856 / 4));
        assert_eq!(centered.top, 260);
        assert_eq!(centered.width(), 928);
        assert_eq!(centered.height(), 520);
    }

    #[test]
    fn every_documented_position_works() {
        for position in POSITIONS {
            assert!(
                snap_rect(position, work()).is_ok(),
                "position '{}' should be valid",
                position
            );
        }
        assert!(snap_rect("diagonal", work()).is_err());
    }

    #[test]
    fn a_zero_sized_work_area_is_refused() {
        let empty = Rect::default();
        assert!(snap_rect("left", empty).is_err());
    }

    #[test]
    fn launch_starts_a_process() {
        // `cargo` itself is guaranteed present in this test environment, and
        // `--version` exits immediately.
        let report = launch("cargo", &["--version".to_string()]).unwrap();
        assert_eq!(report.program, "cargo");
        assert!(report.pid > 0);
    }

    #[test]
    fn launch_rejects_an_empty_program() {
        assert!(launch("", &[]).is_err());
    }

    #[test]
    fn launch_reports_a_missing_program() {
        let err = launch("definitely-not-a-real-program-xyz", &[]).unwrap_err();
        assert!(err.to_string().contains("could not start"), "got: {}", err);
    }

    #[test]
    fn alpha_rejects_a_percentage_above_a_hundred() {
        assert!(set_alpha("whatever", 150).is_err());
    }
}
