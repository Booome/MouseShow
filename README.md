# MouseShow

**English** | [简体中文](README.zh-CN.md)

Mouse click ripples and drag trails, drawn onto the desktop as you work.

## Features

- **A ripple for every press** — no click / double-click classification, so it cannot misclassify: a double click is two ripples, a triple click is three.
- **Drag trails** that grow with the cursor, for both left and right button, coloured to match that button's ripple.
- **True per-pixel alpha** — anti-aliased edges and a real fade-out, not a colour key.
- **Click-through** — never blocks input, never steals focus, never appears in Alt-Tab.
- **Idle cost is zero** — the overlay window is hidden unless something is animating.
- **Multi-monitor and high-DPI aware** (per-monitor v2), covering the whole virtual screen.

Because the marks are part of the picture on screen, any screen recorder captures them as-is. There
is no timestamp mapping step, so the marks cannot drift out of sync with what you actually did.

## Install

Prebuilt binary: download `mouseshow-windows-x64.zip` from [Releases](https://github.com/Booome/MouseShow/releases)
and run `mouseshow.exe`.

From source (Rust 1.85+):

```bash
cargo build --release
# target/release/mouseshow.exe
```

## Usage

```bash
# run until Ctrl+C
mouseshow

# run for 10 minutes, then exit
mouseshow run --duration 600

# command-line flags override the config file for this run only
mouseshow run --color-left "#FF3B30" --no-trail --scale 1.5

# print the effective config, write a default one, or check the environment
mouseshow config
mouseshow init
mouseshow doctor
```

Once the overlay is up, `mouseshow` prints `mouseshow: ready …` on stdout, so an external process
can wait for that line before capturing.

## Configuration

`%APPDATA%\MouseShow\config.toml` — create it with `mouseshow init`. Defaults:

```toml
[general]
fps = 60            # 30 halves the cost on very large screens
scale = 1.0         # multiplies every size; raise on 4K

[colors]
left   = "#FFD600"
right  = "#00C8FF"
middle = "#FF5C8A"

[ripple]
enabled = true
from_radius = 4.5
to_radius = 21.0
duration_ms = 460.0
width = 3.0
fill_alpha = 0.10   # soft fill inside the ring; 0.0 = ring only

[trail]
enabled = true
width = 3.2
tail_ms = 320.0     # how long the trail lingers after release
min_step = 2.0      # ignore movements smaller than this (px)
```

## How it works

- **Overlay** — one borderless window sized to the virtual screen, with
  `WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW | WS_EX_TOPMOST`.
  Its content is a 32bpp premultiplied-alpha DIB passed to `UpdateLayeredWindow`.
- **Input** — `GetAsyncKeyState` and `GetCursorPos` polled at 500 Hz. Deliberately not a
  `WH_MOUSE_LL` hook: a hook sits in every process's input path, tends to be flagged by
  anti-virus software, and adds latency system-wide.
- **Drawing** — [tiny-skia](https://github.com/RazrFalcon/tiny-skia) renders into a `Pixmap`.
- **Cost** — only the changed region is redrawn and uploaded (`UpdateLayeredWindowIndirect` with a
  dirty rect). Measured at 3440×1440, 60 fps: about 2% of one core idle, 14% during a continuous drag.

## Limitations

- Windows only.
- Overlays do not appear over exclusive-fullscreen applications or hardware overlays.
- The overlay covers the work area, not the taskbar: marks are not drawn on the taskbar itself.
  Covering it would make Windows drop the taskbar's transparency for as long as a mark is on screen.

## License

MIT. See [LICENSE](LICENSE).
