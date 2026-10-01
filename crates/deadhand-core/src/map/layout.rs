//! Map geometry: rectangles, insets and an ordered binary treemap. Knows nothing about repos.
//!
//! The treemap keeps items in the order given (repo paths are pre-sorted), so adding a file only
//! moves its near neighbours instead of reshuffling the whole map, as a size-sorted squarified
//! treemap would.

use serde::{Serialize, Serializer};

/// An axis-aligned rectangle in map units. Serialized rounded to two decimals.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
pub struct Rect {
    #[serde(serialize_with = "round2")]
    pub x: f64,
    #[serde(serialize_with = "round2")]
    pub y: f64,
    #[serde(serialize_with = "round2")]
    pub w: f64,
    #[serde(serialize_with = "round2")]
    pub h: f64,
}

fn round2<S: Serializer>(v: &f64, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_f64((v * 100.0).round() / 100.0)
}

/// Tolerance for containment checks, in map units.
const EPS: f64 = 1e-6;

impl Rect {
    pub fn new(x: f64, y: f64, w: f64, h: f64) -> Rect {
        Rect { x, y, w, h }
    }

    pub fn area(&self) -> f64 {
        self.w * self.h
    }

    /// The shorter side.
    pub fn short(&self) -> f64 {
        self.w.min(self.h)
    }

    /// Shrinks by `side` on the left, right and bottom and by `side + top` at the top. Never
    /// produces a negative size: an inset larger than the rectangle collapses it to its centre.
    pub fn inset(&self, side: f64, top: f64) -> Rect {
        let side = side.max(0.0).min(self.w / 2.0);
        let left_over = (self.h - 2.0 * side).max(0.0);
        let top = top.max(0.0).min(left_over);
        let side_v = side.min(self.h / 2.0);
        Rect {
            x: self.x + side,
            y: self.y + side_v + top,
            w: self.w - 2.0 * side,
            h: (self.h - 2.0 * side_v - top).max(0.0),
        }
    }

    /// True when `other` lies inside `self`.
    pub fn contains(&self, other: &Rect) -> bool {
        other.x >= self.x - EPS
            && other.y >= self.y - EPS
            && other.x + other.w <= self.x + self.w + EPS
            && other.y + other.h <= self.y + self.h + EPS
    }

    /// True when the interiors of `self` and `other` intersect.
    pub fn overlaps(&self, other: &Rect) -> bool {
        self.x + EPS < other.x + other.w
            && other.x + EPS < self.x + self.w
            && self.y + EPS < other.y + other.h
            && other.y + EPS < self.y + self.h
    }
}

/// Splits `rect` into one cell per weight, in order, with areas proportional to the weights.
///
/// Binary split: the items are cut where the running weight is closest to half, and the
/// rectangle is cut across its longer side. Non-positive weights count as zero; if every
/// weight is zero, all items share the space equally.
pub fn treemap(weights: &[f64], rect: Rect) -> Vec<Rect> {
    let mut w: Vec<f64> = weights.iter().map(|v| v.max(0.0)).collect();
    if w.iter().sum::<f64>() <= 0.0 {
        w.iter_mut().for_each(|v| *v = 1.0);
    }
    let mut out = vec![Rect::default(); w.len()];
    split(&w, 0, rect, &mut out);
    out
}

fn split(w: &[f64], offset: usize, rect: Rect, out: &mut [Rect]) {
    match w.len() {
        0 => return,
        1 => {
            out[offset] = rect;
            return;
        }
        _ => {}
    }
    let total: f64 = w.iter().sum();
    let (mut acc, mut best, mut k) = (0.0, f64::INFINITY, 1);
    for (i, v) in w.iter().enumerate().take(w.len() - 1) {
        acc += v;
        let d = (acc - total / 2.0).abs();
        if d < best {
            best = d;
            k = i + 1;
        }
    }
    let frac = if total > 0.0 { w[..k].iter().sum::<f64>() / total } else { 0.5 };
    let (a, b) = if rect.w >= rect.h {
        let cut = rect.w * frac;
        (Rect::new(rect.x, rect.y, cut, rect.h), Rect::new(rect.x + cut, rect.y, rect.w - cut, rect.h))
    } else {
        let cut = rect.h * frac;
        (Rect::new(rect.x, rect.y, rect.w, cut), Rect::new(rect.x, rect.y + cut, rect.w, rect.h - cut))
    };
    split(&w[..k], offset, a, out);
    split(&w[k..], offset + k, b, out);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn treemap_areas_are_proportional_and_disjoint() {
        let weights = [40.0, 5.0, 12.0, 1.0, 30.0, 7.0, 7.0, 100.0];
        let rect = Rect::new(10.0, 20.0, 300.0, 200.0);
        let cells = treemap(&weights, rect);
        let total: f64 = weights.iter().sum();
        for (i, c) in cells.iter().enumerate() {
            assert!(rect.contains(c), "{i} outside");
            let expected = rect.area() * weights[i] / total;
            assert!((c.area() - expected).abs() < 1e-6, "{i}: {} vs {expected}", c.area());
            for d in &cells[i + 1..] {
                assert!(!c.overlaps(d));
            }
        }
    }

    #[test]
    fn treemap_keeps_order_and_is_deterministic() {
        let weights = [3.0, 1.0, 4.0, 1.0, 5.0];
        let rect = Rect::new(0.0, 0.0, 100.0, 50.0);
        let a = treemap(&weights, rect);
        assert_eq!(a, treemap(&weights, rect));
        // The first item starts at the top-left corner; the last ends at the bottom-right.
        assert_eq!((a[0].x, a[0].y), (0.0, 0.0));
        let last = a[a.len() - 1];
        assert!((last.x + last.w - 100.0).abs() < 1e-9 && (last.y + last.h - 50.0).abs() < 1e-9);
    }

    #[test]
    fn treemap_edge_cases() {
        let rect = Rect::new(0.0, 0.0, 10.0, 10.0);
        assert!(treemap(&[], rect).is_empty());
        assert_eq!(treemap(&[5.0], rect), vec![rect]);
        let zero = treemap(&[0.0, 0.0], rect);
        assert!((zero[0].area() - 50.0).abs() < 1e-9);
    }

    #[test]
    fn inset_never_goes_negative() {
        let r = Rect::new(0.0, 0.0, 10.0, 4.0);
        let i = r.inset(1.0, 1.0);
        assert_eq!(i, Rect::new(1.0, 2.0, 8.0, 1.0));
        let tiny = r.inset(50.0, 50.0);
        assert!(tiny.w >= 0.0 && tiny.h >= 0.0 && r.contains(&tiny));
    }
}
