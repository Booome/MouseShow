//! MouseShow — click ripples and drag trails drawn straight onto the desktop.
//!
//! Drawing on the desktop (instead of compositing marks in post) means any
//! recorder picks the marks up with the pixels: no timestamp mapping, no drift.
//!
//! # Library use
//!
//! ```no_run
//! let cfg = mouseshow::Config::default();
//! let marks = mouseshow::Marks::start(cfg).unwrap();
//! // …record something…
//! marks.stop();
//! ```
//!
//! `Marks` owns a thread that creates the overlay, pumps its messages and
//! renders until it is stopped. The binary in this crate is a thin CLI on top of
//! the same API.

pub mod config;
mod input;
mod overlay;
mod scene;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use tiny_skia::{Color, Pixmap};
use windows::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext,
};
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetSystemMetrics, MSG, PM_REMOVE, PeekMessageW, SM_CXVIRTUALSCREEN,
    SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN, TranslateMessage,
};

pub use config::Config;
use overlay::{Overlay, Rect};
use scene::{Scene, Style};

/// Where the overlay ended up, and how it is configured.
#[derive(Clone, Copy, Debug)]
pub struct Info {
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
    pub fps: u32,
}

#[derive(Debug)]
pub struct MarksError(pub String);

impl std::fmt::Display for MarksError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for MarksError {}

/// A running overlay. Dropping it stops the overlay.
pub struct Marks {
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
    pub info: Info,
}

impl Marks {
    /// Start the overlay and wait until it is actually on screen.
    ///
    /// Returns as soon as the window exists, so a caller can start recording
    /// immediately without polling for a log line.
    pub fn start(cfg: Config) -> Result<Self, MarksError> {
        let (tx, rx) = mpsc::channel::<Result<Info, String>>();
        let stop = Arc::new(AtomicBool::new(false));
        let stop_inner = Arc::clone(&stop);

        let handle = std::thread::Builder::new()
            .name("mouseshow-overlay".into())
            .spawn(move || {
                if let Err(e) = run_loop(&cfg, &stop_inner, &tx) {
                    // If the caller is already gone this send is ignored.
                    let _ = tx.send(Err(e));
                }
            })
            .map_err(|e| MarksError(format!("cannot spawn the overlay thread: {e}")))?;

        match rx.recv_timeout(Duration::from_secs(5)) {
            Ok(Ok(info)) => Ok(Self {
                stop,
                handle: Some(handle),
                info,
            }),
            Ok(Err(e)) => Err(MarksError(e)),
            Err(_) => Err(MarksError("timed out waiting for the overlay".into())),
        }
    }

    /// Stop the overlay and wait for its thread to finish.
    pub fn stop(mut self) {
        self.shutdown();
    }

    fn shutdown(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

impl Drop for Marks {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn virtual_screen() -> (i32, i32, u32, u32) {
    unsafe {
        (
            GetSystemMetrics(SM_XVIRTUALSCREEN),
            GetSystemMetrics(SM_YVIRTUALSCREEN),
            GetSystemMetrics(SM_CXVIRTUALSCREEN) as u32,
            GetSystemMetrics(SM_CYVIRTUALSCREEN) as u32,
        )
    }
}

/// The desktop area excluding the taskbar.
///
/// The overlay is a topmost layered window. If it covers the taskbar, Windows
/// turns the taskbar's acrylic off for as long as the window is visible, so the
/// bar flashes grey every time a mark is drawn. Staying inside the work area
/// avoids overlapping it. The cost: marks are not drawn on the taskbar.
pub fn work_area() -> Option<(i32, i32, u32, u32)> {
    use windows::Win32::Foundation::RECT;
    use windows::Win32::UI::WindowsAndMessaging::{
        SPI_GETWORKAREA, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, SystemParametersInfoW,
    };

    let mut r = RECT::default();
    unsafe {
        SystemParametersInfoW(
            SPI_GETWORKAREA,
            0,
            Some(&mut r as *mut RECT as *mut core::ffi::c_void),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
    }
    .ok()?;
    let w = (r.right - r.left).max(0) as u32;
    let h = (r.bottom - r.top).max(0) as u32;
    if w == 0 || h == 0 {
        return None;
    }
    Some((r.left, r.top, w, h))
}

/// Check that an overlay window can be created here, without running one.
pub fn probe_overlay() -> Result<(), String> {
    let (vx, vy, vw, vh) = virtual_screen();
    if vw == 0 || vh == 0 {
        return Err("no display detected".into());
    }
    let (x, y, w, h) = fit_to_work_area(vx, vy, vw, vh);
    Overlay::new((x, y), w, h)
        .map(|_| ())
        .map_err(|e| e.to_string())
}

fn fit_to_work_area(vx: i32, vy: i32, vw: u32, vh: u32) -> (i32, i32, u32, u32) {
    match work_area() {
        Some((wx, wy, ww, wh))
            if wx >= vx
                && wy >= vy
                && wx + ww as i32 <= vx + vw as i32
                && wy + wh as i32 <= vy + vh as i32 =>
        {
            (wx, wy, ww, wh)
        }
        _ => (vx, vy, vw, vh),
    }
}

fn pump_messages() {
    let mut msg = MSG::default();
    unsafe {
        while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

fn clear_rect(pixmap: &mut Pixmap, r: Rect) {
    let stride = pixmap.width() as usize * 4;
    let height = pixmap.height() as usize;
    let data = pixmap.data_mut();
    for y in r.y as usize..r.bottom() as usize {
        if y >= height {
            break;
        }
        let off = y * stride + r.x as usize * 4;
        data[off..off + r.w as usize * 4].fill(0);
    }
}

fn run_loop(cfg: &Config, stop: &AtomicBool, ready: &mpsc::Sender<Result<Info, String>>) -> Result<(), String> {
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }

    let (vx, vy, vw, vh) = virtual_screen();
    if vw == 0 || vh == 0 {
        return Err("no display detected".into());
    }
    let (ox, oy, ow, oh) = fit_to_work_area(vx, vy, vw, vh);

    let mut ov = Overlay::new((ox, oy), ow, oh).map_err(|e| e.to_string())?;
    let mut pixmap =
        Pixmap::new(ow, oh).ok_or_else(|| "failed to allocate the pixel buffer".to_string())?;
    let mut scene = Scene::new(Style::from_config(cfg));
    let events = input::spawn();

    let frame = Duration::from_secs_f32(1.0 / cfg.general.fps as f32);
    let mut prev_dirty: Option<Rect> = None;
    let mut next_frame = Instant::now();

    // Handshake back to the caller: the overlay exists and can be captured.
    let _ = ready.send(Ok(Info {
        x: ox,
        y: oy,
        w: ow,
        h: oh,
        fps: cfg.general.fps,
    }));

    loop {
        pump_messages();
        if stop.load(Ordering::Relaxed) {
            break;
        }

        let now = Instant::now();
        while let Ok(ev) = events.try_recv() {
            scene.handle(ev, (ox, oy), now);
        }
        scene.tick(now);

        let current = scene.bounds().and_then(|r| r.clip(ow, oh));

        // Coming back from idle: the compositor has no cached content, so wipe the
        // whole buffer once and do a single full upload. While idle we must NOT
        // touch the buffer — clearing it every frame cost more than drawing.
        let waking = !ov.is_visible() && current.is_some();
        if waking {
            pixmap.fill(Color::TRANSPARENT);
            prev_dirty = None;
        }

        match Rect::union(prev_dirty, current) {
            Some(dirty) => {
                if let Some(d) = dirty.clip(ow, oh) {
                    clear_rect(&mut pixmap, d);
                    scene.render(&mut pixmap, now);
                    let r = if waking {
                        ov.present_all(&pixmap)
                    } else {
                        ov.present_rect(&pixmap, d)
                    };
                    if let Err(e) = r {
                        return Err(format!("cannot present the overlay: {e}"));
                    }
                    prev_dirty = current;
                }
            }
            None => {
                ov.hide();
                prev_dirty = None;
            }
        }

        next_frame += frame;
        let now = Instant::now();
        if next_frame > now {
            std::thread::sleep(next_frame - now);
        } else {
            next_frame = now;
        }
    }

    ov.hide();
    Ok(())
}
