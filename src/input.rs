//! Mouse sampling.
//!
//! Polling `GetAsyncKeyState` + `GetCursorPos` at ~500 Hz instead of installing a
//! `WH_MOUSE_LL` hook. A low-level hook is more "correct" but runs in every
//! process's input path, is frequently flagged by anti-virus software, and adds
//! latency to the whole system. Polling costs ~0.1% of one core and cannot break
//! anything.

use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant};

use windows::Win32::Foundation::POINT;
use windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Button {
    Left,
    Right,
    Middle,
}

impl Button {
    pub fn index(self) -> usize {
        match self {
            Button::Left => 0,
            Button::Right => 1,
            Button::Middle => 2,
        }
    }

    fn vk(self) -> i32 {
        match self {
            Button::Left => 0x01,   // VK_LBUTTON
            Button::Right => 0x02,  // VK_RBUTTON
            Button::Middle => 0x04, // VK_MBUTTON
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum Event {
    Press { button: Button, x: i32, y: i32 },
    Move { x: i32, y: i32 },
    Release { button: Button },
}

const BUTTONS: [Button; 3] = [Button::Left, Button::Right, Button::Middle];
const POLL: Duration = Duration::from_micros(2000);

fn cursor() -> Option<(i32, i32)> {
    let mut p = POINT::default();
    unsafe { GetCursorPos(&mut p).ok()? };
    Some((p.x, p.y))
}

fn is_down(button: Button) -> bool {
    unsafe { GetAsyncKeyState(button.vk()) & 0x8000u16 as i16 != 0 }
}

/// Spawn the polling thread. It exits when the receiver is dropped.
pub fn spawn() -> Receiver<Event> {
    let (tx, rx) = mpsc::channel();

    thread::spawn(move || {
        let mut held = [false; 3];
        let mut last_pos = cursor().unwrap_or((0, 0));
        let mut next = Instant::now();

        loop {
            for (i, button) in BUTTONS.iter().enumerate() {
                let down = is_down(*button);
                if down != held[i] {
                    held[i] = down;
                    let (x, y) = cursor().unwrap_or(last_pos);
                    let ev = if down {
                        Event::Press {
                            button: *button,
                            x,
                            y,
                        }
                    } else {
                        Event::Release { button: *button }
                    };
                    if tx.send(ev).is_err() {
                        return;
                    }
                }
            }

            if let Some(pos) = cursor() {
                if pos != last_pos {
                    last_pos = pos;
                    if tx.send(Event::Move { x: pos.0, y: pos.1 }).is_err() {
                        return;
                    }
                }
            }

            next += POLL;
            let now = Instant::now();
            if next > now {
                thread::sleep(next - now);
            } else {
                next = now; // we fell behind; don't try to catch up
            }
        }
    });

    rx
}
