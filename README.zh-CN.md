# MouseShow

[English](README.md) | **简体中文**

**把鼠标点击波纹和拖动轨迹直接画在桌面上 —— 任何录屏软件都能原样录进去，零时间戳漂移。**

## 为什么做这个

录教程或者录 bug 复现时，观众需要看清你**点了哪里**、**把什么拖到了哪里**。
现有的两种做法各有各的坑：

| 做法 | 问题 |
|---|---|
| 后期合成（OBS 插件、剪辑软件特效） | 必须把「点击时刻」映射到「视频第几帧」。两个时钟永远对不齐，标记会晚几十到几百毫秒出现。 |
| 直接在桌面画 | 原理上就没有对齐问题 —— 但这类工具几乎**只画点击波纹**，没有拖动轨迹，「我按住这里、拖到了那里」这个动作依然是不可见的。 |

MouseShow 既画在桌面上（零同步误差），**又画拖动轨迹**，线会跟着光标实时生长。

## 功能

- **每次按下 = 一个波纹。** 不做单击/双击判定，所以不可能判错：双击就是两次按下，三击就是三次。
- **拖动轨迹**，左右键都支持，颜色与对应按键的波纹一致。
- **真正的逐像素 alpha**（`UpdateLayeredWindow` + 预乘 DIB）：边缘抗锯齿、能真正淡出 —— 这是色键方案做不到的。
- **鼠标穿透。** 不挡操作、不抢焦点、不出现在 Alt-Tab 里。
- **空闲零开销。** 没有动画时窗口直接隐藏。
- **多显示器 / 高 DPI 适配**（per-monitor v2），覆盖整个虚拟屏幕。

明确不做：不做录制器、不做剪辑、不做屏幕批注（画笔、箭头、聚光灯）。目前仅支持 Windows。

## 用法

```bash
# 一直运行，Ctrl+C 退出
mouseshow

# 跑 10 分钟后自动退出 —— 给录制脚本用
mouseshow run --duration 600

# 命令行参数只对这一次生效，覆盖配置文件
mouseshow run --color-left "#FF3B30" --no-trail --scale 1.5

# 打印生效配置 / 生成默认配置 / 环境自检
mouseshow config
mouseshow init
mouseshow doctor
```

覆盖层就绪后，`mouseshow` 会在 stdout 打一行 `mouseshow: ready …`，录制脚本可以先等这一行再开始录，避免录到标记还没起来的那一小段。

## 配置

`%APPDATA%\MouseShow\config.toml`（用 `mouseshow init` 生成）。默认值：

```toml
[general]
fps = 60            # 4K 大屏上改成 30 可以减少一半开销
scale = 1.0         # 所有尺寸的倍数，4K 屏可以调大

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
fill_alpha = 0.10   # 圆环内部的淡淡填充；0.0 = 只有描边

[trail]
enabled = true
width = 3.2
tail_ms = 320.0     # 松手后轨迹保留多久
min_step = 2.0      # 位移小于这个值（像素）就不记点，避免抖动
```

## 实现要点

- **覆盖层**：每次运行创建一个无边框窗口，铺满虚拟屏幕，样式为
  `WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW | WS_EX_TOPMOST`。
  内容是 32bpp 预乘 alpha 的 DIB，交给 `UpdateLayeredWindow` 上屏。
- **输入**：`GetAsyncKeyState` + `GetCursorPos`，500Hz 轮询。**刻意不用 `WH_MOUSE_LL` 钩子** ——
  钩子会挂在每个进程的输入路径上，容易被杀软误报，还会给整个系统增加延迟。轮询坏不了任何东西。
- **绘制**：[tiny-skia](https://github.com/RazrFalcon/tiny-skia) 画进 `Pixmap`。
- **开销**：只重绘、只上传真正变化的区域（`UpdateLayeredWindowIndirect` + `prcDirty`）。
  3440×1440 @ 60fps 实测：**空闲约 2% 单核，持续拖动约 14% 单核。**

## 进度

已完成：覆盖层、点击波纹、拖动轨迹、配置文件、命令行、`doctor`。
未开始：托盘 / 图形设置界面（考虑 Tauri）、热键、光标常亮环、「只在录制时显示」。

`cargo build --release` → `target/release/mouseshow.exe`（单文件，1.2 MB）。

## 许可证

MIT，见 [LICENSE](LICENSE)。
