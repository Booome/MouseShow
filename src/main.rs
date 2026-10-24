//! `mouseshow` — command-line front end.
//!
//! The overlay itself lives in the library; this binary only parses arguments
//! and keeps the process alive for as long as the user asked.

use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use clap::{Args, Parser, Subcommand};

use mouseshow::config::Config;
use mouseshow::{Marks, probe_overlay};

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

    /// Left-button ripple colour
    #[arg(long, global = true, value_name = "#RRGGBB")]
    color_left: Option<String>,

    /// Right-button ripple colour
    #[arg(long, global = true, value_name = "#RRGGBB")]
    color_right: Option<String>,

    /// Middle-button ripple colour
    #[arg(long, global = true, value_name = "#RRGGBB")]
    color_middle: Option<String>,

    /// Render rate in frames per second (10–240)
    #[arg(long, global = true)]
    fps: Option<u32>,

    /// Multiplies every size, for high-DPI or 4K screens
    #[arg(long, global = true)]
    scale: Option<f32>,

    /// Draw drag trails only, no click ripples
    #[arg(long, global = true)]
    no_ripple: bool,

    /// Draw click ripples only, no drag trails
    #[arg(long, global = true)]
    no_trail: bool,
}

impl Overrides {
    fn apply(&self, cfg: &mut Config) {
        // A colour that is not `#RRGGBB`/`#RRGGBBAA` is rejected here rather
        // than at render time, where `parse_color` used to fall back to
        // yellow: `--color-left red` printed nothing and drew yellow.
        let color = |slot: &mut String, value: &str| {
            if is_color(value) {
                *slot = value.to_string();
            } else {
                eprintln!(
                    "mouseshow: {value:?} is not a #RRGGBB or #RRGGBBAA colour — keeping the current one"
                );
            }
        };
        if let Some(v) = &self.color_left {
            color(&mut cfg.colors.left, v);
        }
        if let Some(v) = &self.color_right {
            color(&mut cfg.colors.right, v);
        }
        if let Some(v) = &self.color_middle {
            color(&mut cfg.colors.middle, v);
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

/// `#RRGGBB` or `#RRGGBBAA`, and nothing else.
fn is_color(text: &str) -> bool {
    let hex = text.strip_prefix('#').unwrap_or("");
    (hex.len() == 6 || hex.len() == 8) && hex.bytes().all(|b| b.is_ascii_hexdigit())
}

fn main() {
    let cli = Cli::parse();
    let path = cli.overrides.config.clone().unwrap_or_else(Config::path);

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
            run(cfg, duration)
        }
        Some(Command::Init) => cmd_init(&cfg, &path),
        Some(Command::Config) => match cfg.to_toml() {
            Ok(text) => {
                print!("{text}");
                Ok(())
            }
            // A config that cannot be written back is exactly what `config`
            // is asked to report; failing here beats printing a truncated
            // document the user would take as the whole truth.
            Err(e) => Err(format!("cannot serialise config: {e}").into()),
        },
        Some(Command::Doctor) => cmd_doctor(&cfg, &path),
    };

    if let Err(e) = result {
        eprintln!("mouseshow: {e}");
        std::process::exit(1);
    }
}

fn run(cfg: Config, duration: f64) -> Result<(), Box<dyn Error>> {
    let fps = cfg.general.fps;
    let marks = Marks::start(cfg)?;

    println!(
        "mouseshow: ready  overlay {}x{} at ({},{})  {fps} fps",
        marks.info.w, marks.info.h, marks.info.x, marks.info.y
    );

    if duration > 0.0 {
        std::thread::sleep(Duration::from_secs_f64(duration));
        marks.stop();
    } else {
        // Ctrl+C ends the process; there is no state to flush.
        loop {
            std::thread::park();
        }
    }
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
    fs::write(path, cfg.to_toml()?)?;
    println!("wrote {}", path.display());
    Ok(())
}

fn cmd_doctor(cfg: &Config, path: &Path) -> Result<(), Box<dyn Error>> {
    println!("mouseshow {}", env!("CARGO_PKG_VERSION"));
    match mouseshow::work_area() {
        Some((x, y, w, h)) => println!("work area      : {w}x{h} at ({x},{y})"),
        None => println!("work area      : (unknown)"),
    }
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
    match probe_overlay() {
        Ok(()) => println!("overlay        : ok"),
        Err(e) => println!("overlay        : FAILED ({e})"),
    }
    Ok(())
}
