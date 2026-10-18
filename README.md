# MouseShow

**Mouse click ripples and drag trails that get recorded — because they're drawn on the desktop, not composited in post.**

[中文说明见下方](#中文)

> ⚠️ **Pre-alpha.** The repo is set up and the overlay prototype compiles, but nothing is usable yet.
> Watch / star to follow along.

## Why

When you record a tutorial or a bug report, viewers need to see *where* you clicked and *what* you dragged.
The common approaches both have problems:

| Approach | Problem |
|---|---|
| Post-processing (OBS plugin, editor effect) | Has to map click timestamps onto video time. The two clocks never line up exactly, so marks drift by tens to hundreds of ms — the ripple lands a frame or two after the click. |
| Tools that draw on the desktop | Correct by construction, but almost all of them only draw ripples. **None of them show a drag** — the “I grabbed this and dropped it there” gesture is invisible. |

MouseShow draws on the desktop (so any recorder — OBS, ffmpeg, Xbox Game Bar, ScreenToGif — captures it
with zero sync error), and it draws **drag trails that grow with the cursor**.

## Planned features (v0.1)

- **Every press = one expanding ripple.** No click/double-click guessing: a double click is simply two presses.
- **Drag trails**, left and right button, colour-matched to the ripple of that button.
- **True per-pixel alpha** (`UpdateLayeredWindow`), so rings are anti-aliased and actually fade out.
- Click-through: never steals focus, never appears in Alt-Tab, never blocks input.
- Zero cost when idle: the overlay window is hidden unless something is animating.
- Multi-monitor and per-monitor DPI aware.
- TOML config + CLI overrides.

Non-goals: no recorder, no editor, no on-screen annotation (pens/arrows), Windows only for now.

## Usage (planned)

```bash
# runs until Ctrl+C or --duration elapses; CLI flags override the config file
mouseshow run --duration 600 --color-left "#FFD600"

mouseshow doctor     # prints monitors, DPI, overlay self-check
mouseshow config     # prints the effective config and where each value came from
```

## Status

| Area | State |
|---|---|
| Overlay window (per-pixel alpha, click-through) | prototype |
| Ripple / trail rendering | not started |
| Config + CLI | not started |
| Tray / GUI | later (Tauri), not in v0.1 |

## License

MIT. See [LICENSE](LICENSE).

---

## 中文

**把点击波纹和拖动轨迹直接画在桌面上，被录屏软件原样录进去 —— 所以不存在时间戳对齐问题。**

- 为什么不用后期叠加：后期要先把「点击时刻」映射到「视频第几帧」，两个时钟对不齐，标记必然晚几十到几百毫秒。画在桌面上就没有这个问题。
- 和同类工具的区别：其它工具（ClickShow、OBS Cursor Tools 等）基本只画点击波纹，**没有拖动轨迹** —— 而「我按住这里、拖到这里」恰恰是演示里最难讲清的动作。
- 每次按下 = 一个扩散圆环，所以单击、双击、多击天然都能表现出来。

目前是 **pre-alpha**，只有覆盖层原型，还不能用。
