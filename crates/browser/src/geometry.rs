//! Pure geometry for drawing [`Shape`]s in a window: where a shape is in points, its
//! clamped corner radii, the outline as a polyline, and how a fading border thins out.
//! No window types, so frontends stay thin and this is unit-tested.

use engine::Fade;

use crate::Shape;

/// A rectangle in points: left, top, right, bottom.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pts {
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
}

impl Pts {
    pub fn w(&self) -> f32 {
        self.x1 - self.x0
    }
    pub fn h(&self) -> f32 {
        self.y1 - self.y0
    }
    fn grown(&self, d: f32) -> Pts {
        Pts { x0: self.x0 - d, y0: self.y0 - d, x1: self.x1 + d, y1: self.y1 + d }
    }
}

/// The shape's box in points, for a grid whose top left is `(ox, oy)` and whose cells are
/// `cw` x `ch` points, cut to the shape's clip rows. `None` when nothing is left.
pub fn bounds(s: &Shape, ox: f32, oy: f32, cw: f32, ch: f32) -> Option<Pts> {
    let (mut y0, mut y1) = (oy + s.y * ch, oy + (s.y + s.h) * ch);
    if let Some((top, bottom)) = s.clip {
        y0 = y0.max(oy + top * ch);
        y1 = y1.min(oy + bottom * ch);
    }
    let r = Pts { x0: ox + s.x * cw, y0, x1: ox + (s.x + s.w) * cw, y1 };
    (r.w() > 0.0 && r.h() > 0.0).then_some(r)
}

/// Radii that fit the box: none is more than half of the shorter side, and two that share
/// a side never overlap (so 999 gives a pill).
pub fn fit_radii(r: [f32; 4], w: f32, h: f32) -> [f32; 4] {
    let max = (w.min(h) / 2.0).max(0.0);
    r.map(|v| v.clamp(0.0, max))
}

/// The closed outline of a rounded box as points, each corner as `steps` segments. It
/// starts at the top left corner's end and runs clockwise.
pub fn outline(b: Pts, radii: [f32; 4], steps: usize) -> Vec<(f32, f32)> {
    let [tl, tr, br, bl] = fit_radii(radii, b.w(), b.h());
    let steps = steps.max(1);
    let mut pts = Vec::new();
    // (corner center x, y, radius, start angle in quarter turns)
    let corners = [
        (b.x0 + tl, b.y0 + tl, tl, 2.0),
        (b.x1 - tr, b.y0 + tr, tr, 3.0),
        (b.x1 - br, b.y1 - br, br, 0.0),
        (b.x0 + bl, b.y1 - bl, bl, 1.0),
    ];
    for (cx, cy, r, start) in corners {
        for i in 0..=steps {
            let a = (start + i as f32 / steps as f32) * std::f32::consts::FRAC_PI_2;
            pts.push((cx + r * a.cos(), cy + r * a.sin()));
        }
    }
    pts
}

/// How much of a border's color is left at point `(x, y)` of the box `b`, 0 to 1. The
/// border is strongest on the named side and gone on the opposite one.
pub fn fade_alpha(fade: Fade, b: Pts, x: f32, y: f32) -> f32 {
    let t = |v: f32, lo: f32, hi: f32| if hi > lo { ((v - lo) / (hi - lo)).clamp(0.0, 1.0) } else { 0.0 };
    match fade {
        Fade::None => 1.0,
        Fade::Soft => 0.45,
        Fade::Bottom => t(y, b.y0, b.y1),
        Fade::Top => 1.0 - t(y, b.y0, b.y1),
        Fade::Right => t(x, b.x0, b.x1),
        Fade::Left => 1.0 - t(x, b.x0, b.x1),
    }
}

/// The ring of a focused control: the box grown by `gap`, radii grown with it.
pub fn ring(b: Pts, radii: [f32; 4], gap: f32) -> (Pts, [f32; 4]) {
    (b.grown(gap), radii.map(|r| if r > 0.0 { r + gap } else { 0.0 }))
}

/// Layers of a soft shadow under a box: `(offset down, growth, alpha 0..1)`, farthest first.
pub fn shadow_layers() -> [(f32, f32, f32); 3] {
    [(4.0, 4.0, 0.08), (3.0, 2.5, 0.12), (2.0, 1.0, 0.18)]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shape(x: f32, y: f32, w: f32, h: f32) -> Shape {
        Shape::new(x, y, w, h)
    }

    #[test]
    fn bounds_scale_by_the_cell_and_cut_to_the_clip() {
        let b = bounds(&shape(1.0, 2.0, 3.0, 1.0), 10.0, 100.0, 8.0, 20.0).unwrap();
        assert_eq!(b, Pts { x0: 18.0, y0: 140.0, x1: 42.0, y1: 160.0 });
        let mut s = shape(0.0, 1.0, 2.0, 3.0);
        s.clip = Some((2.0, 3.0));
        let b = bounds(&s, 0.0, 0.0, 8.0, 20.0).unwrap();
        assert_eq!((b.y0, b.y1), (40.0, 60.0));
        s.clip = Some((9.0, 10.0));
        assert!(bounds(&s, 0.0, 0.0, 8.0, 20.0).is_none());
    }

    #[test]
    fn radii_never_exceed_half_the_short_side() {
        assert_eq!(fit_radii([999.0, 4.0, 0.0, 50.0], 100.0, 20.0), [10.0, 4.0, 0.0, 10.0]);
        assert_eq!(fit_radii([-3.0; 4], 10.0, 10.0), [0.0; 4]);
    }

    #[test]
    fn a_square_box_outline_has_its_corners_and_stays_inside() {
        let b = Pts { x0: 0.0, y0: 0.0, x1: 40.0, y1: 20.0 };
        let p = outline(b, [0.0; 4], 3);
        assert!(p.iter().all(|&(x, y)| (0.0..=40.0).contains(&x) && (0.0..=20.0).contains(&y)));
        assert!(p.iter().any(|&(x, y)| (x - 0.0).abs() < 1e-4 && (y - 0.0).abs() < 1e-4));
        assert!(p.iter().any(|&(x, y)| (x - 40.0).abs() < 1e-4 && (y - 20.0).abs() < 1e-4));
    }

    #[test]
    fn a_pill_is_round_at_both_ends() {
        let b = Pts { x0: 0.0, y0: 0.0, x1: 100.0, y1: 20.0 };
        let p = outline(b, [999.0; 4], 8);
        assert!(p.iter().all(|&(x, y)| (-1e-3..=100.001).contains(&x) && (-1e-3..=20.001).contains(&y)));
        // The corner of the box itself is never reached.
        assert!(!p.iter().any(|&(x, y)| x < 1.0 && y < 1.0));
        // The leftmost point is half way down.
        let left = p.iter().cloned().fold((f32::MAX, 0.0), |a, q| if q.0 < a.0 { q } else { a });
        assert!((left.1 - 10.0).abs() < 1.5, "{left:?}");
    }

    #[test]
    fn fading_goes_from_strong_to_nothing() {
        let b = Pts { x0: 0.0, y0: 0.0, x1: 10.0, y1: 10.0 };
        assert_eq!(fade_alpha(Fade::None, b, 3.0, 3.0), 1.0);
        assert_eq!(fade_alpha(Fade::Bottom, b, 5.0, 10.0), 1.0);
        assert_eq!(fade_alpha(Fade::Bottom, b, 5.0, 0.0), 0.0);
        assert_eq!(fade_alpha(Fade::Top, b, 5.0, 0.0), 1.0);
        assert_eq!(fade_alpha(Fade::Left, b, 10.0, 5.0), 0.0);
        assert_eq!(fade_alpha(Fade::Right, b, 5.0, 5.0), 0.5);
        assert!(fade_alpha(Fade::Soft, b, 1.0, 1.0) < 1.0);
        // A box with no size does not divide by zero.
        let z = Pts { x0: 1.0, y0: 1.0, x1: 1.0, y1: 1.0 };
        assert_eq!(fade_alpha(Fade::Top, z, 1.0, 1.0), 1.0);
    }

    #[test]
    fn a_ring_sits_just_outside() {
        let b = Pts { x0: 10.0, y0: 10.0, x1: 30.0, y1: 20.0 };
        let (r, radii) = ring(b, [5.0, 5.0, 0.0, 0.0], 2.0);
        assert_eq!((r.x0, r.y0, r.x1, r.y1), (8.0, 8.0, 32.0, 22.0));
        assert_eq!(radii, [7.0, 7.0, 0.0, 0.0]);
    }
}
