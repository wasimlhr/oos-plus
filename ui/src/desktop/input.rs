// Copyright (C) 2026 dekajoule
// SPDX-License-Identifier: GPL-2.0-or-later

//! Queue edges from Windows messages instead of sampling the final button state.
//! A complete click, Ctrl+O chord or modified wheel event between frames must arrive intact.

use minifb::{InputCallback, Key, Window};
use std::cell::RefCell;
use std::os::windows::ffi::OsStringExt;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::mpsc::{self, Receiver, Sender};
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};

// Use the system import libraries already bundled with the Windows toolchain.
// These few declarations avoid requiring an external assembler just to rebuild
// additional windows-sys raw-dylib binding modules on the GNU host.
type SubclassProc = Option<unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM, usize, usize) -> LRESULT>;
#[link(name = "comctl32")]
extern "system" {
    fn SetWindowSubclass(hwnd: HWND, procedure: SubclassProc, id: usize, data: usize) -> i32;
    fn RemoveWindowSubclass(hwnd: HWND, procedure: SubclassProc, id: usize) -> i32;
    fn DefSubclassProc(hwnd: HWND, message: u32, w: WPARAM, l: LPARAM) -> LRESULT;
}
#[link(name = "user32")]
extern "system" {
    fn GetClientRect(hwnd: HWND, rect: *mut RECT) -> i32;
    fn SetCapture(hwnd: HWND) -> HWND;
    fn ReleaseCapture() -> i32;
    fn ScreenToClient(hwnd: HWND, point: *mut POINT) -> i32;
    fn GetKeyState(key: i32) -> i16;
}
// Files dropped on the window: the handle of a drop arrives as the WPARAM of WM_DROPFILES.
#[link(name = "shell32")]
extern "system" {
    fn DragAcceptFiles(hwnd: HWND, accept: i32);
    fn DragQueryFileW(drop: usize, file: u32, name: *mut u16, length: u32) -> u32;
    fn DragFinish(drop: usize);
}

const WM_KILLFOCUS: u32 = 0x0008;
const WM_CANCELMODE: u32 = 0x001f;
const WM_NCDESTROY: u32 = 0x0082;
const WM_MOUSEMOVE: u32 = 0x0200;
const WM_LBUTTONDOWN: u32 = 0x0201;
const WM_LBUTTONUP: u32 = 0x0202;
const WM_RBUTTONDOWN: u32 = 0x0204;
const WM_MOUSEWHEEL: u32 = 0x020a;
const WM_CAPTURECHANGED: u32 = 0x0215;
const WM_DROPFILES: u32 = 0x0233;

const SUBCLASS_ID: usize = 0x4f4f_5349;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Pointer {
    Down,
    Move,
    Up,
    Context,
    Cancel,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Event {
    Pointer { phase: Pointer, x: i32, y: i32, size: (usize, usize) },
    Key { key: Key, ctrl: bool, repeat: bool },
    Wheel { x: i32, y: i32, size: (usize, usize), delta: i32, ctrl: bool, shift: bool, alt: bool },
}

fn wheel_event(w: WPARAM, point: POINT, size: (usize, usize), alt: bool) -> Event {
    Event::Wheel {
        x: point.x,
        y: point.y,
        size,
        delta: ((w >> 16) as u16) as i16 as i32,
        ctrl: w & 0x0008 != 0,
        shift: w & 0x0004 != 0,
        alt,
    }
}

struct Keys {
    left_ctrl: bool,
    right_ctrl: bool,
    down: [bool; 512],
}

impl Default for Keys {
    fn default() -> Self {
        Self { left_ctrl: false, right_ctrl: false, down: [false; 512] }
    }
}

impl Keys {
    fn update(&mut self, key: Key, state: bool) -> Option<Event> {
        match key {
            Key::LeftCtrl => self.left_ctrl = state,
            Key::RightCtrl => self.right_ctrl = state,
            _ => {}
        }
        let repeat = self.down.get(key as usize).copied().unwrap_or(false);
        if let Some(down) = self.down.get_mut(key as usize) {
            *down = state;
        }
        state.then_some(Event::Key { key, ctrl: self.left_ctrl || self.right_ctrl, repeat })
    }
}

struct Keyboard {
    keys: Rc<RefCell<Keys>>,
    send: Sender<Event>,
}

impl InputCallback for Keyboard {
    fn add_char(&mut self, _: u32) {}
    fn set_key_state(&mut self, key: Key, state: bool) {
        let event = self.keys.borrow_mut().update(key, state);
        if let Some(event) = event {
            let _ = self.send.send(event);
        }
    }
}

struct Capture {
    send: Sender<Event>,
    keys: Rc<RefCell<Keys>>,
    dropped: Sender<PathBuf>,
}

/// The files of one drop, in the order the shell gives them. `query(index, buffer)` is `DragQueryFileW`: with index
/// `u32::MAX` it returns the number of files; with an empty buffer the length of a name in UTF-16 units, without its
/// end mark; with a buffer it writes the name and returns the units written. A name that comes back empty is left out.
fn dropped_names(mut query: impl FnMut(u32, &mut [u16]) -> u32) -> Vec<PathBuf> {
    let count = query(u32::MAX, &mut []);
    (0..count)
        .filter_map(|file| {
            let mut name = vec![0u16; query(file, &mut []) as usize + 1];
            let written = (query(file, &mut name) as usize).min(name.len());
            (written > 0).then(|| std::ffi::OsString::from_wide(&name[..written]).into())
        })
        .collect()
}

unsafe extern "system" fn procedure(hwnd: HWND, message: u32, w: WPARAM, l: LPARAM, _: usize, data: usize) -> LRESULT {
    let capture = &*(data as *const Capture);
    if message == WM_DROPFILES {
        let drop = w;
        for path in dropped_names(|file, name| {
            DragQueryFileW(
                drop,
                file,
                if name.is_empty() { std::ptr::null_mut() } else { name.as_mut_ptr() },
                name.len() as u32,
            )
        }) {
            let _ = capture.dropped.send(path);
        }
        DragFinish(drop);
        return 0;
    }
    if message == WM_MOUSEWHEEL {
        // WM_MOUSEWHEEL positions are screen coordinates; preserve the event's
        // client point and modifiers rather than sampling them after key release.
        let mut point = POINT { x: (l as u16) as i16 as i32, y: ((l as usize >> 16) as u16) as i16 as i32 };
        let mut rect: RECT = std::mem::zeroed();
        if ScreenToClient(hwnd, &mut point) != 0 && GetClientRect(hwnd, &mut rect) != 0 {
            let size = ((rect.right - rect.left).max(1) as usize, (rect.bottom - rect.top).max(1) as usize);
            let _ = capture.send.send(wheel_event(w, point, size, GetKeyState(0x12) < 0));
        }
    }
    let phase = match message {
        WM_LBUTTONDOWN => {
            SetCapture(hwnd);
            Some(Pointer::Down)
        }
        WM_MOUSEMOVE if w & 1 != 0 => Some(Pointer::Move),
        WM_LBUTTONUP => Some(Pointer::Up),
        WM_RBUTTONDOWN => Some(Pointer::Context),
        WM_KILLFOCUS | WM_CANCELMODE | WM_CAPTURECHANGED => {
            if message == WM_KILLFOCUS {
                *capture.keys.borrow_mut() = Keys::default();
            }
            Some(Pointer::Cancel)
        }
        WM_NCDESTROY => {
            RemoveWindowSubclass(hwnd, Some(procedure), SUBCLASS_ID);
            None
        }
        _ => None,
    };
    if let Some(phase) = phase {
        let mut rect: RECT = std::mem::zeroed();
        GetClientRect(hwnd, &mut rect);
        let x = (l as u16) as i16 as i32;
        let y = ((l as usize >> 16) as u16) as i16 as i32;
        let size = ((rect.right - rect.left).max(1) as usize, (rect.bottom - rect.top).max(1) as usize);
        let _ = capture.send.send(Event::Pointer { phase, x, y, size });
    }
    // Releasing capture synchronously sends WM_CAPTURECHANGED. Queue Up first
    // so that cancellation cannot consume a quick click before its release.
    if message == WM_LBUTTONUP {
        ReleaseCapture();
    }
    DefSubclassProc(hwnd, message, w, l)
}

pub struct Input {
    hwnd: HWND,
    capture: Box<Capture>,
    received: Receiver<Event>,
    dropped: Receiver<PathBuf>,
    mouse_captured: bool,
}

impl Input {
    pub fn attach(window: &mut Window) -> Self {
        let (send, received) = mpsc::channel();
        let keys = Rc::new(RefCell::new(Keys::default()));
        window.set_input_callback(Box::new(Keyboard { keys: keys.clone(), send: send.clone() }));
        let (drop_send, dropped) = mpsc::channel();
        let mut capture = Box::new(Capture { send, keys, dropped: drop_send });
        let hwnd = window.get_window_handle();
        // Same window thread for installation, callbacks, and removal. The Box
        // remains stable until Drop unregisters the callback before freeing it.
        let mouse_captured = unsafe {
            SetWindowSubclass(hwnd, Some(procedure), SUBCLASS_ID, (&mut *capture as *mut Capture) as usize) != 0
        };
        // The same procedure takes the drop: without it the window does not say it accepts files.
        if mouse_captured {
            unsafe {
                DragAcceptFiles(hwnd, 1);
            }
        }
        Self { hwnd, capture, received, dropped, mouse_captured }
    }

    /// The files dropped on the window since the last call.
    pub fn dropped(&self) -> Vec<PathBuf> {
        self.dropped.try_iter().collect()
    }

    pub fn mouse_captured(&self) -> bool {
        self.mouse_captured
    }
    pub fn events(&self) -> Vec<Event> {
        self.received.try_iter().collect()
    }
    pub fn discard(&self) {
        for _ in self.received.try_iter() {}
        *self.capture.keys.borrow_mut() = Keys::default();
    }
}

impl Drop for Input {
    fn drop(&mut self) {
        if self.mouse_captured {
            unsafe {
                RemoveWindowSubclass(self.hwnd, Some(procedure), SUBCLASS_ID);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `DragQueryFileW` over these names, as the shell answers it.
    fn shell<'a>(names: &'a [&'a str]) -> impl FnMut(u32, &mut [u16]) -> u32 + 'a {
        move |file, buffer| {
            if file == u32::MAX {
                return names.len() as u32;
            }
            let name: Vec<u16> = names[file as usize].encode_utf16().collect();
            if buffer.is_empty() {
                return name.len() as u32;
            }
            // the shell writes what fits and ends it with a zero
            let fits = name.len().min(buffer.len() - 1);
            buffer[..fits].copy_from_slice(&name[..fits]);
            buffer[fits] = 0;
            fits as u32
        }
    }

    #[test]
    fn the_names_of_a_drop_come_whole_and_in_order() {
        let names = [
            "C:/captures/ch1 rail.pwwf".to_string(),
            "D:/long/".repeat(60) + "bench.oss",
            "C:/captures/ripple 50 µs.bin".to_string(),
        ];
        let names: Vec<&str> = names.iter().map(String::as_str).collect();
        let got = dropped_names(shell(&names));
        assert_eq!(got, names.iter().map(PathBuf::from).collect::<Vec<_>>());
        assert!(dropped_names(shell(&[])).is_empty());
        // a name the shell gives back empty is left out, the others are kept
        assert_eq!(dropped_names(shell(&["", "a.csv"])), [PathBuf::from("a.csv")]);
    }

    #[test]
    fn wheel_retains_its_ctrl_point_and_signed_delta_after_the_chord_is_released() {
        let mut keys = Keys::default();
        keys.update(Key::LeftCtrl, true);
        let event = wheel_event((120 << 16) | 0x0008, POINT { x: 341, y: 230 }, (1024, 600), false);
        keys.update(Key::LeftCtrl, false);
        assert!(!keys.left_ctrl);
        assert_eq!(
            event,
            Event::Wheel { x: 341, y: 230, size: (1024, 600), delta: 120, ctrl: true, shift: false, alt: false }
        );
        assert_eq!(
            wheel_event(((-120i16 as u16 as usize) << 16) | 0x0004, POINT { x: -4, y: 50 }, (800, 600), true),
            Event::Wheel { x: -4, y: 50, size: (800, 600), delta: -120, ctrl: false, shift: true, alt: true }
        );
    }

    #[test]
    fn a_complete_short_ctrl_o_chord_keeps_its_modifier_at_the_press() {
        let mut keys = Keys::default();
        keys.update(Key::LeftCtrl, true);
        let open = keys.update(Key::O, true);
        keys.update(Key::O, false);
        keys.update(Key::LeftCtrl, false);
        assert_eq!(open, Some(Event::Key { key: Key::O, ctrl: true, repeat: false }));
        assert!(!keys.left_ctrl);
    }

    #[test]
    fn focus_reset_does_not_leave_a_stuck_modifier_or_repeat_after_a_file_dialog() {
        let mut keys = Keys::default();
        keys.update(Key::LeftCtrl, true);
        keys.update(Key::O, true);
        keys = Keys::default();
        assert_eq!(keys.update(Key::O, true), Some(Event::Key { key: Key::O, ctrl: false, repeat: false }));
    }
}
