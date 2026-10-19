//! What gets drawn, and when.
//!
//! Two kinds of effect:
//! - **Ripple** — one per button press. Because it is bound to the press (not to
//!   "a click"), a double click is simply two ripples, a triple click three. No
//!   click/double-click classification is needed, and none can be wrong.
//! - **Trail** — the path the cursor travelled while a button was held, growing
//!   with the cursor and fading out shortly after release.

use std::time::{Duration, Instant};

use tiny_skia::{Color, LineCap, LineJoin, Paint, PathBuilder, Pixmap, Stroke, Transform};

use crate::config::Config;
use crate::input::Event;
use crate::overlay::Rect;

/// Effect geometry, pre-scaled and pre-parsed so the render loop does no work.
pub struct Style {
    pub ripple_enabled: bool,
    pub from_radius: f32,
    pub to_radius: f32,
    pub ripple_ms: f32,
    pub ripple_width: f32,
    pub ripple_fill_alpha: f32,
    pub trail_enabled: bool,
    pub trail_width: f32,
    pub trail_tail_ms: f32,
    pub min_step: f32,
    pub colors: [Color; 3],
}

impl Style {
    pub fn from_config(cfg: &Config) -> Self {
        let s = cfg.general.scale;
        Self {
            ripple_enabled: cfg.ripple.enabled,
            from_radius: cfg.ripple.from_radius * s,
            to_radius: cfg.ripple.to_radius * s,
            ripple_ms: cfg.ripple.duration_ms,
            ripple_width: cfg.ripple.width * s,
            ripple_fill_alpha: cfg.ripple.fill_alpha,
            trail_enabled: cfg.trail.enabled,
            trail_width: cfg.trail.width * s,
            trail_tail_ms: cfg.trail.tail_ms,
            min_step: cfg.trail.min_step,
            colors: [
                parse_color(&cfg.colors.left),
                parse_color(&cfg.colors.right),
                parse_color(&cfg.colors.middle),
            ],
        }
    }

    fn ripple_dur(&self) -> Duration {
        Duration::from_secs_f32(self.ripple_ms / 1000.0)
    }

    fn trail_tail(&self) -> Duration {
        Duration::from_secs_f32(self.trail_tail_ms / 1000.0)
    }
}

pub fn parse_color(text: &str) -> Color {
    let t = text.trim().trim_start_matches('#');
    let parse = |s: &str, i: usize, len: usize| -> u8 {
        u8::from_str_radix(&s[i..i + len], 16).unwrap_or(255)
    };
    match t.len() {
        6 => Color::from_rgba8(parse(t, 0, 2), parse(t, 2, 2), parse(t, 4, 2), 255),
        8 => Color::from_rgba8(
            parse(t, 0, 2),
            parse(t, 2, 2),
            parse(t, 4, 2),
            parse(t, 6, 2),
        ),
        _ => Color::from_rgba8(255, 214, 0, 255),
    }
}

struct Ripple {
    born: Instant,
    x: f32,
    y: f32,
    color: Color,
}

struct Trail {
    points: Vec<(f32, f32)>,
    color: Color,
    released: Option<Instant>,
}

const MAX_TRAIL_POINTS: usize = 4096;

pub struct Scene {
    pub style: Style,
    ripples: Vec<Ripple>,
    /// One trail per button, so dragging with two buttons at once stays sane.
    trails: [Option<Trail>; 3],
}

impl Scene {
    pub fn new(style: Style) -> Self {
        Self {
            style,
            ripples: Vec::new(),
            trails: [None, None, None],
        }
    }

    /// `offset` converts virtual-screen coordinates into overlay-local ones.
    pub fn handle(&mut self, ev: Event, offset: (i32, i32), now: Instant) {
        match ev {
            Event::Press { button, x, y } => {
                let (px, py) = ((x - offset.0) as f32, (y - offset.1) as f32);
                let color = self.style.colors[button.index()];

                if self.style.ripple_enabled {
                    self.ripples.push(Ripple {
                        born: now,
                        x: px,
                        y: py,
                        color,
                    });
                }
                if self.style.trail_enabled {
                    self.trails[button.index()] = Some(Trail {
                        points: vec![(px, py)],
                        color,
                        released: None,
                    });
                }
            }
            Event::Move { x, y } => {
                let (px, py) = ((x - offset.0) as f32, (y - offset.1) as f32);
                for t in self.trails.iter_mut().flatten() {
                    if t.released.is_some() {
                        continue;
                    }
                    if let Some(&(lx, ly)) = t.points.last() {
                        let dx = px - lx;
                        let dy = py - ly;
                        if (dx * dx + dy * dy).sqrt() >= self.style.min_step {
                            if t.points.len() >= MAX_TRAIL_POINTS {
                                t.points.remove(0);
                            }
                            t.points.push((px, py));
                        }
                    }
                }
            }
            Event::Release { button } => {
                if let Some(t) = self.trails[button.index()].as_mut() {
                    t.released = Some(now);
                }
            }
        }
    }

    /// Drop finished effects.
    pub fn tick(&mut self, now: Instant) {
        let ripple_dur = self.style.ripple_dur();
        self.ripples
            .retain(|r| now.duration_since(r.born) < ripple_dur);

        let tail = self.style.trail_tail();
        for slot in self.trails.iter_mut() {
            if let Some(t) = slot {
                if let Some(rel) = t.released {
                    if now.duration_since(rel) >= tail {
                        *slot = None;
                    }
                }
            }
        }
    }

    /// Bounding box of everything currently visible, in overlay-local pixels.
    pub fn bounds(&self) -> Option<Rect> {
        let mut acc: Option<Rect> = None;

        for r in &self.ripples {
            let reach = self.style.to_radius + self.style.ripple_width + 2.0;
            let b = Rect {
                x: (r.x - reach) as i32,
                y: (r.y - reach) as i32,
                w: (reach * 2.0) as u32,
                h: (reach * 2.0) as u32,
            };
            acc = Rect::union(acc, Some(b));
        }

        for t in self.trails.iter().flatten() {
            if t.points.len() < 2 {
                continue;
            }
            let (mut x0, mut y0) = (f32::MAX, f32::MAX);
            let (mut x1, mut y1) = (f32::MIN, f32::MIN);
            for &(x, y) in &t.points {
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x);
                y1 = y1.max(y);
            }
            let m = self.style.trail_width + 2.0;
            let b = Rect {
                x: (x0 - m) as i32,
                y: (y0 - m) as i32,
                w: (x1 - x0 + m * 2.0) as u32,
                h: (y1 - y0 + m * 2.0) as u32,
            };
            acc = Rect::union(acc, Some(b));
        }

        acc
    }

    pub fn render(&self, pixmap: &mut Pixmap, now: Instant) {
        for r in &self.ripples {
            let p = (now.duration_since(r.born).as_secs_f32() * 1000.0 / self.style.ripple_ms)
                .clamp(0.0, 1.0);
            let eased = ease_out_cubic(p);
            let radius =
                self.style.from_radius + (self.style.to_radius - self.style.from_radius) * eased;
            let alpha = (1.0 - p).powf(0.85);

            let mut pb = PathBuilder::new();
            pb.push_circle(r.x, r.y, radius);
            let Some(path) = pb.finish() else { continue };

            if self.style.ripple_fill_alpha > 0.0 {
                let mut fill = Paint::default();
                fill.set_color(with_alpha(r.color, alpha * self.style.ripple_fill_alpha));
                fill.anti_alias = true;
                pixmap.fill_path(
                    &path,
                    &fill,
                    tiny_skia::FillRule::Winding,
                    Transform::identity(),
                    None,
                );
            }

            let mut paint = Paint::default();
            paint.set_color(with_alpha(r.color, alpha));
            paint.anti_alias = true;
            let stroke = Stroke {
                width: self.style.ripple_width,
                line_cap: LineCap::Round,
                ..Default::default()
            };
            pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
        }

        for t in self.trails.iter().flatten() {
            if t.points.len() < 2 {
                continue;
            }
            let alpha = match t.released {
                None => 1.0,
                Some(rel) => {
                    let tail = self.style.trail_tail_ms / 1000.0;
                    if tail <= 0.0 {
                        0.0
                    } else {
                        (1.0 - now.duration_since(rel).as_secs_f32() / tail).clamp(0.0, 1.0)
                    }
                }
            };

            let mut pb = PathBuilder::new();
            pb.move_to(t.points[0].0, t.points[0].1);
            for &(x, y) in &t.points[1..] {
                pb.line_to(x, y);
            }
            let Some(path) = pb.finish() else { continue };

            let mut paint = Paint::default();
            paint.set_color(with_alpha(t.color, alpha));
            paint.anti_alias = true;
            let stroke = Stroke {
                width: self.style.trail_width,
                line_cap: LineCap::Round,
                line_join: LineJoin::Round,
                ..Default::default()
            };
            pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
        }
    }
}

fn with_alpha(c: Color, a: f32) -> Color {
    Color::from_rgba(c.red(), c.green(), c.blue(), a.clamp(0.0, 1.0)).unwrap_or(Color::TRANSPARENT)
}

fn ease_out_cubic(p: f32) -> f32 {
    1.0 - (1.0 - p).powi(3)
}
