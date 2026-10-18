# MouseShow

**English** | [简体中文](README.zh-CN.md)

**Mouse click ripples and drag trails drawn straight onto the desktop — so any screen recorder captures them, with zero timestamp drift.**

## Why

When you record a tutorial or a bug report, viewers need to see *where* you clicked and *what* you dragged.
The two common approaches both have a catch:

| Approach | Catch |
|---|---|
| Post-processing (OBS plugin, editor effect) | Click timestamps have to be mapped onto video time. The two clocks never line up exactly, so the marks land tens to hundreds of milliseconds late. |
| Tools that draw on the desktop | Correct by construction — but nearly all of them only draw ripples. **None of them show a drag**, so the “I grabbed this and dropped it there” gesture stays invisible. |

MouseShow draws on the desktop (zero sync error) **and** draws drag trails that grow with the cursor.

## What it does

- **Every press = one ripple.** No click / double-click classification, so it cannot misclassify: a double click is two presses, a triple click is three.
- **Drag trails**, left and right button, coloured to match that button's ripple.
- **Real per-pixel alpha** (`UpdateLayeredWindow` + premultiplied DIB): anti-aliased edges and a genuine fade-out, which a colour-key overlay cannot do.
- **Click-through.** Never blocks input, never steals focus, never shows up in Alt-Tab.
- **Zero cost when idle.** The window is hidden unless something is animating.
- **Multi-monitor / high-DPI aware** (per-monitor v2), covers the whole virtual screen.

Non-goals: no recorder, no editor, no on-screen annotation (pens, arrows, spotlights). Windows only for now.

## Usage

```bash
# run until Ctrl+C
mouseshow

# run for 10 minutes, then exit — handy from a recording script
mouseshow run --duration 600

# CLI flags override the config file for this run only
mouseshow run --color-left "#FF3B30" --no-trail --scale 1.5

# print the effective config, write a default one, or check the environment
mouseshow config
mouseshow init
mouseshow doctor
```

`mouseshow` prints `mouseshow: ready …` on stdout once the overlay is live, so a recording
script can wait for it before starting the capture.

## Configuration

`%APPDATA%\MouseShow\config.toml` (create it with `mouseshow init`). Defaults:

```toml
[general]
fps = 60            # 30 halves the cost on huge screens
scale = 1.0         # multiplies every size; raise on 4K

[colors]
left   = "#FFD600"
right  = "#00C8FF"
middle = "#FF5C8A"

[ripple]
enabled = true
from_radius = 9.0
to_radius = 42.0
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

- **Overlay**: one borderless window per run, sized to the virtual screen, with
  `WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW | WS_EX_TOPMOST`.
  Content is a 32bpp premultiplied-alpha DIB handed to `UpdateLayeredWindow`.
- **Input**: `GetAsyncKeyState` + `GetCursorPos` polled at 500 Hz. Deliberately *not* a
  `WH_MOUSE_LL` hook — a hook sits in every process's input path, is frequently flagged by
  anti-virus software, and adds latency system-wide. Polling cannot break anything.
- **Drawing**: [tiny-skia](https://github.com/RazrFalcon/tiny-skia) renders into a `Pixmap`.
- **Cost**: only the region that actually changed is redrawn and uploaded
  (`UpdateLayeredWindowIndirect` with a dirty rect). Measured on 3440×1440 @ 60 fps:
  **~2% of one core while idle, ~14% during a continuous drag.**

## Status

Working today: overlay, ripples, drag trails, config file, CLI, `doctor`.
Not started: tray / GUI (likely Tauri), hotkeys, cursor highlight ring, "only while recording".

`cargo build --release` → `target/release/mouseshow.exe` (single self-contained binary, 1.2 MB).

## License

MIT. See [LICENSE](LICENSE).
