//! The overlay window: a per-pixel-alpha layered window covering a screen region.
//!
//! Why not a colour-key window (what most similar tools do)? A colour key cannot
//! express partial transparency, so rings either pop in fully opaque or have hard
//! edges. `UpdateLayeredWindow` with a 32bpp premultiplied DIB gives real alpha:
//! anti-aliased edges and a genuine fade-out.
//!
//! Window styles, and why each one matters:
//! - `WS_EX_LAYERED`         required for `UpdateLayeredWindow`
//! - `WS_EX_TRANSPARENT`     clicks pass through, never blocks the user
//! - `WS_EX_NOACTIVATE`      never steals focus
//! - `WS_EX_TOOLWINDOW`      hidden from Alt-Tab and the taskbar
//! - `WS_EX_TOPMOST`         above normal windows

use std::ffi::c_void;
use std::ptr::null_mut;

use tiny_skia::Pixmap;
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, SIZE, WPARAM};
use windows::Win32::Graphics::Gdi::{
    AC_SRC_ALPHA, AC_SRC_OVER, BITMAPINFO, BITMAPINFOHEADER, BLENDFUNCTION, CreateCompatibleDC,
    CreateDIBSection, DIB_RGB_COLORS, DeleteObject, HDC, HGDIOBJ, SelectObject,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CS_HREDRAW, CS_VREDRAW, CreateWindowExW, DefWindowProcW, RegisterClassW, SW_HIDE,
    SW_SHOWNOACTIVATE, ShowWindow, ULW_ALPHA, UPDATELAYEREDWINDOWINFO, UpdateLayeredWindow,
    UpdateLayeredWindowIndirect, WINDOW_EX_STYLE, WNDCLASSW, WS_EX_LAYERED, WS_EX_NOACTIVATE,
    WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
};
use windows::core::{Result, w};

const CLASS_NAME: windows::core::PCWSTR = w!("MouseShowOverlay");

/// A rectangle in overlay-local pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
}

impl Rect {
    pub fn right(&self) -> i32 {
        self.x + self.w as i32
    }

    pub fn bottom(&self) -> i32 {
        self.y + self.h as i32
    }

    pub fn union(a: Option<Rect>, b: Option<Rect>) -> Option<Rect> {
        match (a, b) {
            (None, x) | (x, None) => x,
            (Some(a), Some(b)) => {
                let x = a.x.min(b.x);
                let y = a.y.min(b.y);
                let r = a.right().max(b.right());
                let bo = a.bottom().max(b.bottom());
                Some(Rect {
                    x,
                    y,
                    w: (r - x) as u32,
                    h: (bo - y) as u32,
                })
            }
        }
    }

    /// Clip to a `width` x `height` area, returning `None` if nothing is left.
    pub fn clip(&self, width: u32, height: u32) -> Option<Rect> {
        let x0 = self.x.max(0);
        let y0 = self.y.max(0);
        let x1 = self.right().min(width as i32);
        let y1 = self.bottom().min(height as i32);
        if x1 <= x0 || y1 <= y0 {
            return None;
        }
        Some(Rect {
            x: x0,
            y: y0,
            w: (x1 - x0) as u32,
            h: (y1 - y0) as u32,
        })
    }

}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe { DefWindowProcW(hwnd, msg, wp, lp) }
}

pub struct Overlay {
    pub hwnd: HWND,
    mem_dc: HDC,
    hbmp: HGDIOBJ,
    /// Top-down BGRA, premultiplied alpha.
    bits: *mut u8,
    pub width: u32,
    pub height: u32,
    /// Top-left corner in virtual-screen coordinates.
    pub origin: (i32, i32),
    visible: bool,
}

impl Overlay {
    pub fn new(origin: (i32, i32), width: u32, height: u32) -> Result<Self> {
        unsafe {
            let hinstance = GetModuleHandleW(None)?;

            let wc = WNDCLASSW {
                style: CS_HREDRAW | CS_VREDRAW,
                lpfnWndProc: Some(wndproc),
                hInstance: hinstance.into(),
                lpszClassName: CLASS_NAME,
                ..Default::default()
            };
            RegisterClassW(&wc);

            let ex = WINDOW_EX_STYLE(
                WS_EX_LAYERED.0
                    | WS_EX_TRANSPARENT.0
                    | WS_EX_NOACTIVATE.0
                    | WS_EX_TOOLWINDOW.0
                    | WS_EX_TOPMOST.0,
            );

            let hwnd = CreateWindowExW(
                ex,
                CLASS_NAME,
                w!("MouseShow"),
                WS_POPUP,
                origin.0,
                origin.1,
                width as i32,
                height as i32,
                None,
                None,
                Some(hinstance.into()),
                None,
            )?;

            // 32bpp top-down DIB: a negative height means row 0 is the top row,
            // matching the coordinate system of a Pixmap.
            let bmi = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: width as i32,
                    biHeight: -(height as i32),
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: 0, // BI_RGB
                    ..Default::default()
                },
                ..Default::default()
            };

            let mut bits: *mut c_void = null_mut();
            let hbmp = CreateDIBSection(None, &bmi, DIB_RGB_COLORS, &mut bits, None, 0)?;
            let mem_dc = CreateCompatibleDC(None);
            SelectObject(mem_dc, hbmp.into());

            Ok(Self {
                hwnd,
                mem_dc,
                hbmp: hbmp.into(),
                bits: bits as *mut u8,
                width,
                height,
                origin,
                visible: false,
            })
        }
    }

    /// Copy a region of the pixmap into the DIB, converting RGBA -> BGRA.
    ///
    /// tiny-skia stores premultiplied RGBA; a 32bpp DIB expects premultiplied BGRA,
    /// so only the R and B channels swap.
    fn blit(&mut self, pixmap: &Pixmap, region: Option<Rect>) {
        let r = region.unwrap_or(Rect {
            x: 0,
            y: 0,
            w: self.width,
            h: self.height,
        });
        let src_stride = pixmap.width() as usize * 4;
        let src = pixmap.data();

        for row in 0..r.h as usize {
            let y = r.y as usize + row;
            if y >= self.height as usize {
                break;
            }
            let src_off = y * src_stride + r.x as usize * 4;
            let dst_off = (y * self.width as usize + r.x as usize) * 4;
            let n = r.w as usize * 4;
            let s = &src[src_off..src_off + n];
            let d = unsafe { std::slice::from_raw_parts_mut(self.bits.add(dst_off), n) };
            for (sp, dp) in s.chunks_exact(4).zip(d.chunks_exact_mut(4)) {
                dp[0] = sp[2];
                dp[1] = sp[1];
                dp[2] = sp[0];
                dp[3] = sp[3];
            }
        }
    }

    fn show_if_needed(&mut self) {
        if !self.visible {
            unsafe {
                let _ = ShowWindow(self.hwnd, SW_SHOWNOACTIVATE);
            }
            self.visible = true;
        }
    }

    /// Upload the whole bitmap. Use this for the first frame of an animation.
    pub fn present_all(&mut self, pixmap: &Pixmap) -> Result<()> {
        self.blit(pixmap, None);
        self.show_if_needed();

        unsafe {
            let dst_pt = POINT {
                x: self.origin.0,
                y: self.origin.1,
            };
            let size = SIZE {
                cx: self.width as i32,
                cy: self.height as i32,
            };
            let src_pt = POINT { x: 0, y: 0 };
            let blend = BLENDFUNCTION {
                BlendOp: AC_SRC_OVER as u8,
                BlendFlags: 0,
                SourceConstantAlpha: 255,
                AlphaFormat: AC_SRC_ALPHA as u8,
            };
            UpdateLayeredWindow(
                self.hwnd,
                None,
                Some(&dst_pt),
                Some(&size),
                Some(self.mem_dc),
                Some(&src_pt),
                COLORREF(0),
                Some(&blend),
                ULW_ALPHA,
            )
        }
    }

    /// Upload only `rect`, keeping the rest of the window content cached by the
    /// compositor. This is what keeps a 4K overlay cheap during animation.
    pub fn present_rect(&mut self, pixmap: &Pixmap, rect: Rect) -> Result<()> {
        self.blit(pixmap, Some(rect));
        self.show_if_needed();

        unsafe {
            let dst_pt = POINT {
                x: self.origin.0,
                y: self.origin.1,
            };
            let size = SIZE {
                cx: self.width as i32,
                cy: self.height as i32,
            };
            let src_pt = POINT { x: 0, y: 0 };
            let blend = BLENDFUNCTION {
                BlendOp: AC_SRC_OVER as u8,
                BlendFlags: 0,
                SourceConstantAlpha: 255,
                AlphaFormat: AC_SRC_ALPHA as u8,
            };
            let dirty = windows::Win32::Foundation::RECT {
                left: rect.x,
                top: rect.y,
                right: rect.right(),
                bottom: rect.bottom(),
            };
            let info = UPDATELAYEREDWINDOWINFO {
                cbSize: size_of::<UPDATELAYEREDWINDOWINFO>() as u32,
                hdcDst: HDC::default(),
                pptDst: &dst_pt,
                psize: &size,
                hdcSrc: self.mem_dc,
                pptSrc: &src_pt,
                crKey: COLORREF(0),
                pblend: &blend,
                dwFlags: ULW_ALPHA,
                prcDirty: &dirty,
            };
            UpdateLayeredWindowIndirect(self.hwnd, &info).ok()
        }
    }

    /// Idle: hide the window. A hidden layered window is not composited at all,
    /// so the idle cost is literally zero.
    pub fn hide(&mut self) {
        if self.visible {
            unsafe {
                let _ = ShowWindow(self.hwnd, SW_HIDE);
            }
            self.visible = false;
        }
    }

    pub fn is_visible(&self) -> bool {
        self.visible
    }
}

impl Drop for Overlay {
    fn drop(&mut self) {
        unsafe {
            if !self.hbmp.is_invalid() {
                let _ = DeleteObject(self.hbmp);
            }
        }
    }
}
