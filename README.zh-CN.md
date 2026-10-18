# MouseShow

[English](README.md) | **简体中文**

把鼠标点击波纹和拖动轨迹直接画在桌面上的工具。

## 功能

- **每次按下出一个波纹** —— 不做单击/双击判定，所以不可能判错：双击就是两个波纹，三击就是三个。
- **拖动轨迹**，随光标实时生长，左右键都支持，颜色与对应按键的波纹一致。
- **真正的逐像素 alpha** —— 边缘抗锯齿、能真正淡出，不是色键。
- **鼠标穿透** —— 不挡操作、不抢焦点、不出现在 Alt-Tab 里。
- **空闲零开销** —— 没有动画时覆盖层窗口直接隐藏。
- **多显示器与高 DPI 适配**（per-monitor v2），覆盖整个虚拟屏幕。

标记是屏幕上画面的一部分，任何录屏软件都会原样录进去。中间没有时间戳映射这一步，所以标记不可能和实际操作错位。

## 安装

预编译版本：从 [Releases](https://github.com/Booome/MouseShow/releases) 下载 `mouseshow-windows-x64.zip`，直接运行 `mouseshow.exe`。

从源码构建（需要 Rust 1.85+）：

```bash
cargo build --release
# 产物：target/release/mouseshow.exe
```

## 用法

```bash
# 一直运行，Ctrl+C 退出
mouseshow

# 运行 10 分钟后自动退出
mouseshow run --duration 600

# 命令行参数只对本次生效，覆盖配置文件
mouseshow run --color-left "#FF3B30" --no-trail --scale 1.5

# 打印生效配置 / 生成默认配置 / 环境自检
mouseshow config
mouseshow init
mouseshow doctor
```

覆盖层就绪后会在 stdout 打印一行 `mouseshow: ready …`，外部程序可以等这一行再开始捕获。

## 配置

`%APPDATA%\MouseShow\config.toml`，用 `mouseshow init` 生成。默认值：

```toml
[general]
fps = 60            # 超大屏幕上改成 30 可以减少一半开销
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
fill_alpha = 0.10   # 圆环内部的淡填充；0.0 = 只有描边

[trail]
enabled = true
width = 3.2
tail_ms = 320.0     # 松手后轨迹保留多久
min_step = 2.0      # 位移小于该值（像素）不记点，避免抖动
```

## 实现要点

- **覆盖层** —— 一个铺满虚拟屏幕的无边框窗口，样式为
  `WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW | WS_EX_TOPMOST`，
  内容是 32bpp 预乘 alpha 的 DIB，交给 `UpdateLayeredWindow` 上屏。
- **输入** —— `GetAsyncKeyState` 与 `GetCursorPos`，500Hz 轮询。刻意不使用 `WH_MOUSE_LL` 钩子：
  钩子挂在每个进程的输入路径上，容易被杀软误报，还会给整个系统带来延迟。
- **绘制** —— [tiny-skia](https://github.com/RazrFalcon/tiny-skia) 画进 `Pixmap`。
- **开销** —— 只重绘、只上传变化的区域（`UpdateLayeredWindowIndirect` + 脏矩形）。
  3440×1440 @ 60fps 实测：空闲约 2% 单核，持续拖动约 14% 单核。

## 已知限制

- 仅支持 Windows。
- 独占全屏应用和硬件叠加层上方不会显示覆盖层。
- 覆盖层铺满整个虚拟屏幕，光标在哪就在哪画。

## 许可证

MIT，见 [LICENSE](LICENSE)。
