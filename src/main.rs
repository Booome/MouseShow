//! MouseShow — click ripples and drag trails drawn straight onto the desktop.
//!
//! Drawing on the desktop (instead of compositing marks in post) means any
//! recorder picks the marks up with the pixels: no timestamp mapping, no drift.

mod config;
mod input;
mod overlay;
mod scene;

use std::error::Error;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use clap::{Args, Parser, Subcommand};
use tiny_skia::{Color, Pixmap};
use windows::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext,
};
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetSystemMetrics, MSG, PM_REMOVE, PeekMessageW, SM_CXVIRTUALSCREEN,
    SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN, TranslateMessage,
};

use crate::config::Config;
use crate::overlay::{Overlay, Rect};
use crate::scene::{Scene, Style};

#[derive(Parser, Debug)]
#[command(
    name = "mouseshow",
    version,
    about = "Click ripples and drag trails that any screen recorder captures"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    #[command(flatten)]
    overrides: Overrides,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Run the overlay (default when no subcommand is given)
    Run {
        /// Stop after this many seconds. 0 = run until Ctrl+C.
        #[arg(short, long, default_value_t = 0.0)]
        duration: f64,
    },
    /// Write a default config file
    Init,
    /// Print the effective configuration
    Config,
    /// Print environment diagnostics
    Doctor,
}

#[derive(Args, Debug, Clone)]
struct Overrides {
    /// Path to the config file (default: %APPDATA%\MouseShow\config.toml)
    #[arg(short, long, global = true)]
    config: Option<PathBuf>,

    #[arg(long, global = true, value_name = "#RRGGBB")]
    color_left: Option<String>,

    #[arg(long, global = true, value_name = "#RRGGBB")]
    color_right: Option<String>,

    #[arg(long, global = true, value_name = "#RRGGBB")]
    color_middle: Option<String>,

    #[arg(long, global = true)]
    fps: Option<u32>,

    /// Multiplies every size, for high-DPI or 4K screens
    #[arg(long, global = true)]
    scale: Option<f32>,

    #[arg(long, global = true)]
    no_ripple: bool,

    #[arg(long, global = true)]
    no_trail: bool,
}

impl Overrides {
    fn apply(&self, cfg: &mut Config) {
        if let Some(v) = &self.color_left {
            cfg.colors.left = v.clone();
        }
        if let Some(v) = &self.color_right {
            cfg.colors.right = v.clone();
        }
        if let Some(v) = &self.color_middle {
            cfg.colors.middle = v.clone();
        }
        if let Some(v) = self.fps {
            cfg.general.fps = v;
        }
        if let Some(v) = self.scale {
            cfg.general.scale = v;
        }
        if self.no_ripple {
            cfg.ripple.enabled = false;
        }
        if self.no_trail {
            cfg.trail.enabled = false;
        }
    }
}

fn main() {
    let cli = Cli::parse();

    let path = cli
        .overrides
        .config
        .clone()
        .unwrap_or_else(Config::path);

    let mut cfg = match Config::load(&path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("mouseshow: cannot read config: {e}");
            std::process::exit(2);
        }
    };
    cli.overrides.apply(&mut cfg);
    cfg.sanitize();

    let result = match cli.command {
        None | Some(Command::Run { .. }) => {
            let duration = match cli.command {
                Some(Command::Run { duration }) => duration,
                _ => 0.0,
            };
            run(&cfg, duration)
        }
        Some(Command::Init) => cmd_init(&cfg, &path),
        Some(Command::Config) => {
            print!("{}", cfg.to_toml());
            Ok(())
        }
        Some(Command::Doctor) => cmd_doctor(&cfg, &path),
    };

    if let Err(e) = result {
        eprintln!("mouseshow: {e}");
        std::process::exit(1);
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

fn run(cfg: &Config, duration: f64) -> Result<(), Box<dyn Error>> {
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }

    let (vx, vy, vw, vh) = virtual_screen();
    if vw == 0 || vh == 0 {
        return Err("no display detected".into());
    }

    let mut ov = Overlay::new((vx, vy), vw, vh)?;
    let mut pixmap = Pixmap::new(vw, vh).ok_or("failed to allocate the pixel buffer")?;
    let mut scene = Scene::new(Style::from_config(cfg));
    let events = input::spawn();

    let frame = Duration::from_secs_f32(1.0 / cfg.general.fps as f32);
    let start = Instant::now();
    let mut prev_dirty: Option<Rect> = None;
    let mut next_frame = Instant::now();

    // Handshake for scripts: once this line appears, the overlay is alive and
    // the recorder can start.
    println!(
        "mouseshow: ready  overlay {vw}x{vh} at ({vx},{vy})  {} fps  scale {:.2}",
        cfg.general.fps, cfg.general.scale
    );
    let _ = std::io::stdout().flush();

    loop {
        pump_messages();

        let now = Instant::now();
        while let Ok(ev) = events.try_recv() {
            scene.handle(ev, (vx, vy), now);
        }
        scene.tick(now);

        let current = scene.bounds().and_then(|r| r.clip(vw, vh));

        // Coming back from idle: the compositor has no cached content, so wipe the
        // whole buffer once and do a single full upload. While idle we must NOT
        // touch the buffer — clearing 3440x1440 every frame cost more than drawing.
        let waking = !ov.is_visible() && current.is_some();
        if waking {
            pixmap.fill(Color::TRANSPARENT);
            prev_dirty = None;
        }

        match Rect::union(prev_dirty, current) {
            Some(dirty) => {
                if let Some(d) = dirty.clip(vw, vh) {
                    clear_rect(&mut pixmap, d);
                    scene.render(&mut pixmap, now);
                    if waking {
                        ov.present_all(&pixmap)?;
                    } else {
                        ov.present_rect(&pixmap, d)?;
                    }
                    prev_dirty = current;
                }
            }
            None => {
                ov.hide();
                prev_dirty = None;
            }
        }

        if duration > 0.0 && start.elapsed().as_secs_f64() >= duration {
            break;
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

fn cmd_init(cfg: &Config, path: &Path) -> Result<(), Box<dyn Error>> {
    if path.exists() {
        println!("config already exists: {}", path.display());
        return Ok(());
    }
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    fs::write(path, cfg.to_toml())?;
    println!("wrote {}", path.display());
    Ok(())
}

fn cmd_doctor(cfg: &Config, path: &Path) -> Result<(), Box<dyn Error>> {
    let (vx, vy, vw, vh) = virtual_screen();
    println!("mouseshow {}", env!("CARGO_PKG_VERSION"));
    println!("virtual screen : {vw}x{vh} at ({vx},{vy})");
    println!("config         : {}", path.display());
    println!("config exists  : {}", path.exists());
    println!("fps            : {}", cfg.general.fps);
    println!("scale          : {}", cfg.general.scale);
    println!(
        "ripple         : {}  r {}..{}  {}ms",
        cfg.ripple.enabled, cfg.ripple.from_radius, cfg.ripple.to_radius, cfg.ripple.duration_ms
    );
    println!(
        "trail          : {}  width {}  tail {}ms",
        cfg.trail.enabled, cfg.trail.width, cfg.trail.tail_ms
    );
    println!(
        "colors         : left {}  right {}  middle {}",
        cfg.colors.left, cfg.colors.right, cfg.colors.middle
    );

    // Can we actually create the overlay?
    match Overlay::new((vx, vy), vw.max(1), vh.max(1)) {
        Ok(_) => println!("overlay        : ok"),
        Err(e) => println!("overlay        : FAILED ({e})"),
    }
    Ok(())
}
