//! Atomic › SVG: rasterizes SVG path data (the `d` attribute: M L H V C S Q T Z, absolute and
//! relative) with an even-odd or non-zero fill and an optional stroke. Fill coverage is exact
//! horizontally over four sub-scanlines per pixel. Also the shared capsule (thick line)
//! rasterizer used by the stroke, the Text node and the scratches.

use materialcraft_image::{Image, Kind};
use rayon::prelude::*;

use crate::atomic::ATOMIC;
use crate::catalog::{CookResult, Ctx, NodeDef, node};

/// A leaf with a midrib slit, in 0..100 units.
const DEFAULT_PATH: &str = "M50 4 C80 20 94 55 52 96 L50 90 L48 96 C6 55 20 20 50 4 Z M49 22 Q46 55 50 84 Q54 55 51 22 Z";

/// Most points a path may flatten to (an allocation cap for hostile input).
const MAX_POINTS: usize = 400_000;

pub(crate) fn defs() -> Vec<NodeDef> {
    vec![
        node("svg", "SVG", ATOMIC)
            .desc("Draws SVG path data (M L H V C S Q T Z, absolute and relative; y points down) filled and optionally stroked, anti-aliased.")
            .atomic()
            .generator()
            .text("path", "Path data", DEFAULT_PATH)
            .choice("units", "Units", &["0..1", "0..100"], 1)
            .color("fill", "Fill color", [1.0, 1.0, 1.0, 1.0])
            .choice("fill_rule", "Fill rule", &["Even-odd", "Non-zero"], 1)
            .float("stroke_width", "Stroke width", 0.0, 0.0, 64.0)
            .color("stroke", "Stroke color", [0.5, 0.5, 0.5, 1.0])
            .color_out()
            .cook(svg),
    ]
}

/// A flattened subpath in path units; `closed` after Z.
#[derive(Clone, Debug, PartialEq)]
pub struct Poly {
    pub pts: Vec<[f32; 2]>,
    pub closed: bool,
}

struct Lexer<'a> {
    s: &'a [u8],
    i: usize,
}

impl Lexer<'_> {
    fn skip(&mut self) {
        while self.s.get(self.i).is_some_and(|c| c.is_ascii_whitespace() || *c == b',') {
            self.i += 1;
        }
    }

    fn peek(&mut self) -> Option<u8> {
        self.skip();
        self.s.get(self.i).copied()
    }

    fn at_number(&mut self) -> bool {
        self.peek().is_some_and(|c| c.is_ascii_digit() || matches!(c, b'-' | b'+' | b'.'))
    }

    fn number(&mut self) -> Result<f32, String> {
        self.skip();
        let (start, s) = (self.i, self.s);
        let digit = |j: usize| s.get(j).is_some_and(u8::is_ascii_digit);
        let mut j = start + matches!(s.get(start), Some(b'+' | b'-')) as usize;
        let mut digits = 0;
        while digit(j) {
            j += 1;
            digits += 1;
        }
        if s.get(j) == Some(&b'.') {
            j += 1;
            while digit(j) {
                j += 1;
                digits += 1;
            }
        }
        if digits == 0 {
            return Err(format!("expected a number at byte {start}"));
        }
        if matches!(s.get(j), Some(b'e' | b'E')) {
            let mut k = j + 1 + matches!(s.get(j + 1), Some(b'+' | b'-')) as usize;
            let k0 = k;
            while digit(k) {
                k += 1;
            }
            if k > k0 {
                j = k;
            }
        }
        let txt = s.get(start..j).and_then(|b| std::str::from_utf8(b).ok()).unwrap_or("");
        let v: f32 = txt.parse().map_err(|_| format!("bad number '{txt}' at byte {start}"))?;
        if !v.is_finite() {
            return Err(format!("number out of range at byte {start}"));
        }
        self.i = j;
        Ok(v)
    }

    fn pair(&mut self) -> Result<[f32; 2], String> {
        Ok([self.number()?, self.number()?])
    }
}

/// Builds flattened subpaths while parsing.
struct Pen {
    polys: Vec<Poly>,
    cur: [f32; 2],
    start: [f32; 2],
    /// The previous segment's last control point and whether it was cubic (for S and T).
    ctrl: Option<([f32; 2], bool)>,
    steps_per_unit: f32,
    points: usize,
}

impl Pen {
    fn push(&mut self, p: [f32; 2]) -> Result<(), String> {
        self.points += 1;
        if self.points > MAX_POINTS {
            return Err(format!("path too complex (over {MAX_POINTS} points)"));
        }
        match self.polys.last_mut() {
            Some(poly) if !poly.closed => poly.pts.push(p),
            // Drawing after Z starts a new subpath at the closed one's start.
            _ => self.polys.push(Poly { pts: vec![self.start, p], closed: false }),
        }
        self.cur = p;
        Ok(())
    }

    fn move_to(&mut self, p: [f32; 2]) {
        self.polys.push(Poly { pts: vec![p], closed: false });
        (self.cur, self.start) = (p, p);
    }

    /// A Bézier through `ctrl` (2 or 3 points after the current one), flattened in steps of
    /// about 1 / `steps_per_unit` along its control polygon.
    fn curve(&mut self, ctrl: &[[f32; 2]]) -> Result<(), String> {
        let mut pts = vec![self.cur];
        pts.extend_from_slice(ctrl);
        let len: f32 = pts.windows(2).map(|w| ((w[1][0] - w[0][0]).powi(2) + (w[1][1] - w[0][1]).powi(2)).sqrt()).sum();
        let n = (len * self.steps_per_unit).ceil().clamp(2.0, 256.0) as usize;
        for k in 1..=n {
            let t = k as f32 / n as f32;
            // De Casteljau.
            let mut q = pts.clone();
            while q.len() > 1 {
                q = q.windows(2).map(|w| [w[0][0] + (w[1][0] - w[0][0]) * t, w[0][1] + (w[1][1] - w[0][1]) * t]).collect();
            }
            self.push(q.first().copied().unwrap_or(self.cur))?;
        }
        Ok(())
    }

    fn reflect(&self, cubic: bool) -> [f32; 2] {
        match self.ctrl {
            Some((c, was_cubic)) if was_cubic == cubic => [2.0 * self.cur[0] - c[0], 2.0 * self.cur[1] - c[1]],
            _ => self.cur,
        }
    }
}

/// Parses SVG path data into flattened subpaths. Curves get about `steps_per_unit` segments per
/// unit of control-polygon length (2..256 per curve). Bad input is an error, never a panic.
pub fn parse_path(d: &str, steps_per_unit: f32) -> Result<Vec<Poly>, String> {
    let mut lx = Lexer { s: d.as_bytes(), i: 0 };
    let steps_per_unit = if steps_per_unit.is_finite() { steps_per_unit.max(0.0) } else { 0.0 };
    let mut pen = Pen { polys: Vec::new(), cur: [0.0; 2], start: [0.0; 2], ctrl: None, steps_per_unit, points: 0 };
    let mut prev: Option<u8> = None;
    while let Some(c) = lx.peek() {
        let op = if c.is_ascii_alphabetic() {
            lx.i += 1;
            c
        } else if lx.at_number() {
            // Repeated arguments repeat the command; extra pairs after a move are lines.
            match prev {
                Some(b'M') => b'L',
                Some(b'm') => b'l',
                Some(p) if !matches!(p, b'Z' | b'z') => p,
                _ => return Err(format!("number without a command at byte {}", lx.i)),
            }
        } else {
            return Err(format!("unexpected '{}' at byte {}", c as char, lx.i));
        };
        if prev.is_none() && !matches!(op, b'M' | b'm') {
            return Err("path data must start with a move (M)".into());
        }
        let rel = op.is_ascii_lowercase();
        let o = if rel { pen.cur } else { [0.0; 2] };
        let at = |p: [f32; 2]| [p[0] + o[0], p[1] + o[1]];
        let mut ctrl = None;
        match op.to_ascii_uppercase() {
            b'M' => pen.move_to(at(lx.pair()?)),
            b'L' => pen.push(at(lx.pair()?))?,
            b'H' => pen.push([lx.number()? + o[0], pen.cur[1]])?,
            b'V' => pen.push([pen.cur[0], lx.number()? + o[1]])?,
            b'C' => {
                let (c1, c2, p) = (at(lx.pair()?), at(lx.pair()?), at(lx.pair()?));
                pen.curve(&[c1, c2, p])?;
                ctrl = Some((c2, true));
            }
            b'S' => {
                let c1 = pen.reflect(true);
                let (c2, p) = (at(lx.pair()?), at(lx.pair()?));
                pen.curve(&[c1, c2, p])?;
                ctrl = Some((c2, true));
            }
            b'Q' => {
                let (c1, p) = (at(lx.pair()?), at(lx.pair()?));
                pen.curve(&[c1, p])?;
                ctrl = Some((c1, false));
            }
            b'T' => {
                let c1 = pen.reflect(false);
                pen.curve(&[c1, at(lx.pair()?)])?;
                ctrl = Some((c1, false));
            }
            b'Z' => {
                if let Some(p) = pen.polys.last_mut() {
                    p.closed = true;
                }
                pen.cur = pen.start;
            }
            b'A' => return Err("arc commands (A) are not supported; use curves".into()),
            _ => return Err(format!("unknown command '{}'", op as char)),
        }
        pen.ctrl = ctrl;
        prev = Some(op);
    }
    Ok(pen.polys)
}

/// Fill coverage (0..1 per pixel, row-major) of polygons in pixel coordinates (each implicitly
/// closed): exact horizontal span coverage on four sub-scanlines per pixel row.
pub(crate) fn fill(w: u32, h: u32, polys: &[Vec<[f32; 2]>], even_odd: bool) -> Vec<f32> {
    // Edges as (x0, y0, x1, y1, winding) with y0 < y1.
    let mut edges: Vec<(f32, f32, f32, f32, i32)> = Vec::new();
    for p in polys {
        for (k, a) in p.iter().enumerate() {
            let b = p.get(k + 1).or(p.first()).copied().unwrap_or(*a);
            if a[1] < b[1] {
                edges.push((a[0], a[1], b[0], b[1], 1));
            } else if b[1] < a[1] {
                edges.push((b[0], b[1], a[0], a[1], -1));
            }
        }
    }
    let wu = w as usize;
    let rows: Vec<Vec<f32>> = (0..h)
        .into_par_iter()
        .map(|y| {
            let mut row = vec![0.0f32; wu];
            let mut xs: Vec<(f32, i32)> = Vec::new();
            for s in 0..4 {
                let sy = y as f32 + (s as f32 + 0.5) * 0.25;
                xs.clear();
                xs.extend(edges.iter().filter(|e| e.1 <= sy && sy < e.3).map(|e| (e.0 + (sy - e.1) * (e.2 - e.0) / (e.3 - e.1), e.4)));
                xs.sort_by(|a, b| a.0.total_cmp(&b.0));
                let mut wind = 0;
                for k in 0..xs.len() {
                    wind += xs[k].1;
                    let inside = if even_odd { wind & 1 != 0 } else { wind != 0 };
                    if let (true, Some(next)) = (inside, xs.get(k + 1)) {
                        add_span(&mut row, xs[k].0, next.0, 0.25);
                    }
                }
            }
            row
        })
        .collect();
    rows.concat()
}

/// Adds `wt` × the overlap of `[a, b)` with each pixel of `row`.
fn add_span(row: &mut [f32], a: f32, b: f32, wt: f32) {
    let n = row.len() as f32;
    let (a, b) = (a.clamp(0.0, n), b.clamp(0.0, n));
    if b <= a {
        return;
    }
    for px in a.floor() as usize..(b.ceil() as usize).min(row.len()) {
        let ov = b.min(px as f32 + 1.0) - a.max(px as f32);
        if let Some(v) = row.get_mut(px) {
            *v += ov.max(0.0) * wt;
        }
    }
}

/// A line segment with a weight (its peak coverage).
#[derive(Clone, Copy, Debug)]
pub(crate) struct Seg {
    pub a: [f32; 2],
    pub b: [f32; 2],
    pub wt: f32,
}

/// Thick line segments ("capsules") of radius `r` in pixel space, binned on a grid for fast
/// lookups. Coverage ramps over one pixel at the border (anti-aliased); with `tile` it wraps.
pub(crate) struct Capsules {
    segs: Vec<Seg>,
    r: f32,
    g: usize,
    cell: [f32; 2],
    bins: Vec<Vec<u32>>,
    size: [f32; 2],
    tile: bool,
}

impl Capsules {
    pub fn new(segs: Vec<Seg>, r: f32, w: u32, h: u32, tile: bool) -> Capsules {
        let r = if r.is_finite() { r.max(0.0) } else { 0.0 };
        let reach = r + 1.0;
        let g = ((w.max(h) as f32 / (reach * 4.0).max(8.0)) as usize).clamp(1, 128);
        let cell = [w as f32 / g as f32, h as f32 / g as f32];
        let mut bins: Vec<Vec<u32>> = vec![Vec::new(); g * g];
        for (i, s) in segs.iter().enumerate() {
            if !s.a.iter().chain(&s.b).all(|v| v.is_finite()) {
                continue;
            }
            let span = |a: f32, b: f32, c: f32| {
                let lo = ((a.min(b) - reach) / c).floor() as i64;
                (lo, (((a.max(b) + reach) / c).floor() as i64).min(lo + g as i64 - 1))
            };
            let ((x0, x1), (y0, y1)) = (span(s.a[0], s.b[0], cell[0]), span(s.a[1], s.b[1], cell[1]));
            for gy in y0..=y1 {
                for gx in x0..=x1 {
                    if !tile && (gx < 0 || gy < 0 || gx >= g as i64 || gy >= g as i64) {
                        continue;
                    }
                    if let Some(b) = bins.get_mut(gy.rem_euclid(g as i64) as usize * g + gx.rem_euclid(g as i64) as usize) {
                        b.push(i as u32);
                    }
                }
            }
        }
        Capsules { segs, r, g, cell, bins, size: [w as f32, h as f32], tile }
    }

    /// Coverage (0..1, the strongest segment's weight) at pixel coordinates `(x, y)`.
    pub fn coverage(&self, x: f32, y: f32) -> f32 {
        let (gx, gy) = (((x / self.cell[0]) as usize).min(self.g - 1), ((y / self.cell[1]) as usize).min(self.g - 1));
        let mut best = 0.0f32;
        for &i in self.bins.get(gy * self.g + gx).into_iter().flatten() {
            let Some(s) = self.segs.get(i as usize) else { continue };
            let (mut px, mut py) = (x - s.a[0], y - s.a[1]);
            if self.tile {
                px -= (px / self.size[0]).round() * self.size[0];
                py -= (py / self.size[1]).round() * self.size[1];
            }
            let (dx, dy) = (s.b[0] - s.a[0], s.b[1] - s.a[1]);
            let t = ((px * dx + py * dy) / (dx * dx + dy * dy).max(1e-12)).clamp(0.0, 1.0);
            let d = ((px - dx * t).powi(2) + (py - dy * t).powi(2)).sqrt();
            best = best.max((self.r - d + 0.5).clamp(0.0, 1.0) * s.wt);
        }
        best
    }
}

/// A color image from fill and stroke coverage over transparent black (premultiplied).
pub(crate) fn composite(
    w: u32,
    h: u32,
    fill: [f32; 4],
    cover: impl Fn(u32, u32) -> f32 + Sync,
    stroke: [f32; 4],
    stroke_cover: impl Fn(u32, u32) -> f32 + Sync,
) -> Image {
    Image::generate(w, h, Kind::Color, |x, y| {
        let f = cover(x, y).clamp(0.0, 1.0) * fill[3];
        let s = stroke_cover(x, y).clamp(0.0, 1.0) * stroke[3];
        let k = f * (1.0 - s);
        [fill[0] * k + stroke[0] * s, fill[1] * k + stroke[1] * s, fill[2] * k + stroke[2] * s, k + s]
    })
}

fn svg(ctx: &Ctx) -> CookResult {
    let unit = if ctx.i("units") == 1 { 100.0 } else { 1.0 };
    let (sx, sy) = (ctx.w as f32 / unit, ctx.h as f32 / unit);
    // About one curve segment per 3 pixels.
    let polys = parse_path(ctx.text("path"), sx.max(sy) / 3.0)?;
    let rings: Vec<Vec<[f32; 2]>> = polys.iter().map(|p| p.pts.iter().map(|q| [q[0] * sx, q[1] * sy]).collect()).collect();
    let cov = fill(ctx.w, ctx.h, &rings, ctx.i("fill_rule") == 0);
    let sw = ctx.f("stroke_width").max(0.0) * ctx.w as f32 / 256.0;
    let caps = (sw > 0.0).then(|| {
        let mut segs = Vec::new();
        for (p, ring) in polys.iter().zip(&rings) {
            segs.extend(ring.windows(2).map(|w| Seg { a: w[0], b: w[1], wt: 1.0 }));
            if let (true, Some(a), Some(b)) = (p.closed, ring.last(), ring.first()) {
                segs.push(Seg { a: *a, b: *b, wt: 1.0 });
            }
            if ring.len() == 1 {
                segs.extend(ring.first().map(|a| Seg { a: *a, b: *a, wt: 1.0 }));
            }
        }
        Capsules::new(segs, sw * 0.5, ctx.w, ctx.h, false)
    });
    let w = ctx.w as usize;
    Ok(vec![composite(
        ctx.w,
        ctx.h,
        ctx.v4("fill"),
        |x, y| cov.get(y as usize * w + x as usize).copied().unwrap_or(0.0),
        ctx.v4("stroke"),
        |x, y| caps.as_ref().map_or(0.0, |c| c.coverage(x as f32 + 0.5, y as f32 + 0.5)),
    )])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pts(d: &str) -> Vec<Vec<[f32; 2]>> {
        parse_path(d, 1.0).unwrap().into_iter().map(|p| p.pts).collect()
    }

    #[test]
    fn parses_lines_absolute_and_relative() {
        assert_eq!(pts("M0 0 L10 0 L10 10 Z"), vec![vec![[0.0, 0.0], [10.0, 0.0], [10.0, 10.0]]]);
        assert_eq!(pts("m1,1 l2,0 0,2 h-2 v-1"), vec![vec![[1.0, 1.0], [3.0, 1.0], [3.0, 3.0], [1.0, 3.0], [1.0, 2.0]]]);
        // Extra pairs after M are lines; compact number syntax.
        assert_eq!(pts("M0 0 1 1 2-2"), vec![vec![[0.0, 0.0], [1.0, 1.0], [2.0, -2.0]]]);
        assert_eq!(pts("M.5.5L1e1-1E0"), vec![vec![[0.5, 0.5], [10.0, -1.0]]]);
        assert_eq!(pts("M0 0H5V5H0z M8 8 L9 9").len(), 2);
    }

    #[test]
    fn parses_curves_to_their_endpoints() {
        for d in
            ["M0 0 C0 10 10 10 10 0", "M0 0 c0 10 10 10 10 0", "M0 0 Q5 10 10 0", "M0 0 q5 10 10 0", "M0 0 Q5 5 5 0 T10 0", "M0 0 C0 5 5 5 5 0 S10 -5 10 0"]
        {
            let p = pts(d);
            let last = *p[0].last().unwrap();
            assert!((last[0] - 10.0).abs() < 1e-4 && last[1].abs() < 1e-4, "{d}: {last:?}");
            assert!(p[0].len() > 3, "{d} is flattened");
        }
        // The cubic's midpoint bulges to 7.5.
        let p = parse_path("M0 0 C0 10 10 10 10 0", 0.0).unwrap();
        assert_eq!(p[0].pts.len(), 3);
        assert!((p[0].pts[1][1] - 7.5).abs() < 1e-4);
    }

    #[test]
    fn bad_paths_are_errors() {
        for d in ["L0 0", "M0", "M0 0 L1", "M0 0 X1 1", "M0 0 L1 1e99", "M0 0 A1 1 0 0 1 2 2", "5 5", "M0 0 Z 3 4", "M 0 0 L - 1"] {
            assert!(parse_path(d, 1.0).is_err(), "{d}");
        }
        assert!(parse_path("", 1.0).unwrap().is_empty());
        assert!(parse_path("M0 0 C1 1 2 2 3 3", f32::NAN).is_ok());
    }

    #[test]
    fn fill_rules_and_coverage() {
        let sq = |a: f32, b: f32, ccw: bool| {
            let mut v = vec![[a, a], [b, a], [b, b], [a, b]];
            if ccw {
                v.reverse();
            }
            v
        };
        // Square of 4..12 on a 16×16 grid: area 64; a half-pixel edge gives half coverage.
        let c = fill(16, 16, &[sq(4.0, 12.0, false)], false);
        assert!((c.iter().sum::<f32>() - 64.0).abs() < 1e-3);
        let c = fill(16, 16, &[sq(4.5, 12.0, false)], false);
        assert!((c[8 * 16 + 4] - 0.5).abs() < 1e-4);
        // A hole: even-odd always; non-zero only with opposite winding.
        let eo = fill(16, 16, &[sq(2.0, 14.0, false), sq(6.0, 10.0, false)], true);
        let nz_same = fill(16, 16, &[sq(2.0, 14.0, false), sq(6.0, 10.0, false)], false);
        let nz_opp = fill(16, 16, &[sq(2.0, 14.0, false), sq(6.0, 10.0, true)], false);
        let center = 8 * 16 + 8;
        assert_eq!((eo[center], nz_same[center], nz_opp[center]), (0.0, 1.0, 0.0));
    }

    #[test]
    fn default_svg_draws_the_leaf() {
        let def = crate::catalog::find("svg").unwrap();
        let p = crate::catalog::resolve_params(def, &Default::default());
        let no_res = |_: &str| None;
        let vars = Default::default();
        let ctx = Ctx { w: 64, h: 64, seed: 0, tile: true, inputs: &[], params: &p, resource: &no_res, vars: &vars };
        let img = &svg(&ctx).unwrap()[0];
        let a = img.mean()[3];
        assert!(a > 0.2 && a < 0.7, "{a}");
        assert!(img.get(20, 32, false)[3] > 0.9 && img.get(2, 2, false)[3] == 0.0);
    }
}
