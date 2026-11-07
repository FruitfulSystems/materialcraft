//! Texture generators › Noises: 3D noises that still tile. Perlin and Worley use a 3D lattice that
//! is periodic in x and y (whole-number scales) and free in z, so scrubbing `z` animates the
//! pattern. The simplex variant samples 4D simplex noise on a flat torus (u and v each mapped to a
//! circle), which tiles at any scale; `z` slides the torus through the 4D field.

use std::f32::consts::TAU;

use crate::catalog::{B, CookResult, Ctx, NodeDef, node};
use crate::generators::NOISES;
use crate::noise::{hash, hash3};
use crate::noises_more::fractal;

#[inline]
fn wrap(i: i32, p: i32) -> i32 {
    if p <= 0 { i } else { i.rem_euclid(p) }
}

/// A hash of a 3D lattice point, periodic in x and y.
#[inline]
fn hash_xyz(i: i32, j: i32, k: i32, px: i32, py: i32, seed: u32) -> u32 {
    hash(hash3(wrap(i, px), wrap(j, py), seed) ^ (k as u32).wrapping_mul(0x9e37_79b1))
}

#[inline]
fn fade(t: f32) -> f32 {
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

/// 3D gradient (Perlin) noise in about -1..1, periods `px`, `py` cells in x and y.
pub fn perlin3(x: f32, y: f32, z: f32, px: i32, py: i32, seed: u32) -> f32 {
    const G: [[f32; 3]; 12] = [
        [1.0, 1.0, 0.0],
        [-1.0, 1.0, 0.0],
        [1.0, -1.0, 0.0],
        [-1.0, -1.0, 0.0],
        [1.0, 0.0, 1.0],
        [-1.0, 0.0, 1.0],
        [1.0, 0.0, -1.0],
        [-1.0, 0.0, -1.0],
        [0.0, 1.0, 1.0],
        [0.0, -1.0, 1.0],
        [0.0, 1.0, -1.0],
        [0.0, -1.0, -1.0],
    ];
    let (x0, y0, z0) = (x.floor(), y.floor(), z.floor());
    let (fx, fy, fz) = (x - x0, y - y0, z - z0);
    let (ix, iy, iz) = (x0 as i32, y0 as i32, z0 as i32);
    let g = |a: i32, b: i32, c: i32| {
        let gr = G[(hash_xyz(ix + a, iy + b, iz + c, px, py, seed) % 12) as usize];
        gr[0] * (fx - a as f32) + gr[1] * (fy - b as f32) + gr[2] * (fz - c as f32)
    };
    let (u, v, w) = (fade(fx), fade(fy), fade(fz));
    let lerp = |a: f32, b: f32, t: f32| a + (b - a) * t;
    let x00 = lerp(g(0, 0, 0), g(1, 0, 0), u);
    let x10 = lerp(g(0, 1, 0), g(1, 1, 0), u);
    let x01 = lerp(g(0, 0, 1), g(1, 0, 1), u);
    let x11 = lerp(g(0, 1, 1), g(1, 1, 1), u);
    lerp(lerp(x00, x10, v), lerp(x01, x11, v), w)
}

/// Uniform 0..1 from a hash.
#[inline]
fn unit(h: u32) -> f32 {
    (h >> 8) as f32 / (1u32 << 24) as f32
}

/// 3D Worley noise: nearest and second-nearest feature distances (cells), periodic in x and y.
pub fn worley3(x: f32, y: f32, z: f32, px: i32, py: i32, jitter: f32, seed: u32) -> (f32, f32) {
    let (ix, iy, iz) = (x.floor() as i32, y.floor() as i32, z.floor() as i32);
    let (mut f1, mut f2) = (f32::MAX, f32::MAX);
    for k in -1..=1 {
        for j in -1..=1 {
            for i in -1..=1 {
                let (cx, cy, cz) = (ix + i, iy + j, iz + k);
                let h = hash_xyz(cx, cy, cz, px, py, seed);
                let o = |s: u32| 0.5 + (unit(hash(h ^ s)) - 0.5) * jitter;
                let (dx, dy, dz) = (cx as f32 + o(1) - x, cy as f32 + o(2) - y, cz as f32 + o(3) - z);
                let d = (dx * dx + dy * dy + dz * dz).sqrt();
                if d < f1 {
                    f2 = f1;
                    f1 = d;
                } else if d < f2 {
                    f2 = d;
                }
            }
        }
    }
    (f1, f2)
}

/// 4D simplex noise in about -1..1 (after Gustavson's public description of the algorithm).
pub fn simplex4(p: [f32; 4], seed: u32) -> f32 {
    const F4: f32 = 0.309_017; // (√5 − 1) / 4
    const G4: f32 = 0.138_196_6; // (5 − √5) / 20
    let s = (p[0] + p[1] + p[2] + p[3]) * F4;
    let i: [f32; 4] = std::array::from_fn(|k| (p[k] + s).floor());
    let t = (i[0] + i[1] + i[2] + i[3]) * G4;
    let x0: [f32; 4] = std::array::from_fn(|k| p[k] - (i[k] - t));
    // Rank the coordinates to find the simplex the point is in.
    let mut rank = [0i32; 4];
    for a in 0..4 {
        for b in a + 1..4 {
            if x0[a] > x0[b] {
                rank[a] += 1;
            } else {
                rank[b] += 1;
            }
        }
    }
    let ii: [i32; 4] = std::array::from_fn(|k| i[k] as i32);
    let mut n = 0.0;
    for c in 0..5 {
        let off: [i32; 4] = std::array::from_fn(|k| (rank[k] >= 4 - c) as i32);
        let x: [f32; 4] = std::array::from_fn(|k| x0[k] - off[k] as f32 + c as f32 * G4);
        let t = 0.6 - x.iter().map(|v| v * v).sum::<f32>();
        if t <= 0.0 {
            continue;
        }
        let h = hash(hash3(ii[0] + off[0], ii[1] + off[1], seed) ^ hash3(ii[2] + off[2], ii[3] + off[3], seed ^ 0x68e3_1da4));
        // 32 gradients: one coordinate zero, the other three ±1.
        let zero = (h >> 3) as usize & 3;
        let mut dot = 0.0;
        let mut bit = 0;
        for k in 0..4 {
            if k != zero {
                dot += if h >> bit & 1 == 0 { x[k] } else { -x[k] };
                bit += 1;
            }
        }
        n += t * t * t * t * dot;
    }
    27.0 * n
}

/// Fractal 4D simplex on a torus at `(u, v)`: tiles in u and v for any scale.
fn simplex_torus(u: f32, v: f32, z: f32, scale: f32, octaves: i32, gain: f32, seed: u32) -> f32 {
    let (su, cu) = (u * TAU).sin_cos();
    let (sv, cv) = (v * TAU).sin_cos();
    // Circumference `scale` cells per axis.
    let r = scale / TAU;
    fractal(octaves, gain, |m, o| {
        let (r, z) = (r * m, z * m);
        let p = [r * cu + z * 0.7, r * su + z * 0.3, r * cv - z * 0.5, r * sv + z * 0.4];
        (simplex4(p, seed.wrapping_add(o * 977)) * 0.5 + 0.5).clamp(0.0, 1.0)
    })
}

/// Shared noise parameters.
fn noise_params(b: B, scale: f32, octaves: i32) -> B {
    b.generator()
        .float("scale", "Scale", scale, 1.0, 64.0)
        .int("octaves", "Octaves", octaves, 1, 10)
        .float("roughness", "Roughness", 0.5, 0.0, 1.0)
        .float("z", "Z (slice)", 0.0, 0.0, 16.0)
        .float("contrast", "Contrast", 0.5, 0.0, 1.0)
        .gray_out()
}

pub(crate) fn defs() -> Vec<NodeDef> {
    let n = |id, label| node(id, label, NOISES);
    vec![
        noise_params(n("perlin_noise_3d", "Perlin Noise 3D").desc("Fractal 3D gradient noise; Z scrubs through the volume. Tiles."), 4.0, 5)
            .cook(perlin_noise_3d),
        noise_params(
            n("simplex_noise_3d", "Simplex Noise 3D").desc("Fractal simplex noise sampled on a 4D torus; Z moves through it. Tiles at any scale."),
            4.0,
            5,
        )
        .cook(simplex_noise_3d),
        noise_params(
            n("worley_noise_3d", "Worley Noise 3D")
                .desc("Cellular noise from 3D feature points; Z scrubs through the cells. Tiles.")
                .choice("distance", "Distance", &["F1", "F2", "F2 - F1"], 0)
                .float("jitter", "Jitter", 1.0, 0.0, 1.0),
            6.0,
            1,
        )
        .cook(worley_noise_3d),
    ]
}

/// Common noise settings: whole-number scale, octaves, gain, z, contrast gain, seed.
fn settings(ctx: &Ctx) -> (f32, i32, f32, f32, f32, u32) {
    (ctx.f("scale").max(1.0).round(), ctx.i("octaves"), ctx.f("roughness"), ctx.f("z"), 1.0 + ctx.f("contrast") * 3.0, ctx.seed)
}

/// Expand around mid gray by `k`.
#[inline]
fn spread(v: f32, k: f32) -> f32 {
    ((v - 0.5) * k + 0.5).clamp(0.0, 1.0)
}

fn perlin_noise_3d(ctx: &Ctx) -> CookResult {
    let (s, oct, gain, z, k, seed) = settings(ctx);
    Ok(vec![ctx.gen_gray(|u, v| {
        let n = fractal(oct, gain, |m, o| {
            let p = s * m;
            perlin3(u * p, v * p, z * m + o as f32 * 17.31, p as i32, p as i32, seed.wrapping_add(o * 977)) * 0.5 + 0.5
        });
        spread(n, k)
    })])
}

fn simplex_noise_3d(ctx: &Ctx) -> CookResult {
    let (_, oct, gain, z, k, seed) = settings(ctx);
    let s = ctx.f("scale").max(0.1);
    Ok(vec![ctx.gen_gray(|u, v| spread(simplex_torus(u, v, z, s, oct, gain, seed), k))])
}

fn worley_noise_3d(ctx: &Ctx) -> CookResult {
    let (s, oct, gain, z, k, seed) = settings(ctx);
    let (mode, jit) = (ctx.i("distance"), ctx.f("jitter").clamp(0.0, 1.0));
    Ok(vec![ctx.gen_gray(|u, v| {
        let n = fractal(oct, gain, |m, o| {
            let p = s * m;
            let (f1, f2) = worley3(u * p, v * p, z * m, p as i32, p as i32, jit, seed.wrapping_add(o * 977));
            match mode {
                1 => f2 * 0.75,
                2 => (f2 - f1) * 1.5,
                _ => f1,
            }
            .clamp(0.0, 1.0)
        });
        // Cell distances already span 0..1: contrast 0.5 leaves them as they are.
        spread(n, k * 0.4)
    })])
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::catalog::{find, resolve_params};

    #[test]
    fn primitives_tile_and_stay_in_range() {
        for k in 0..20 {
            let (y, z) = (k as f32 * 0.37, k as f32 * 0.11);
            assert!((perlin3(0.0, y, z, 4, 4, 7) - perlin3(4.0, y, z, 4, 4, 7)).abs() < 1e-5);
            assert!((perlin3(y, 0.0, z, 4, 4, 7) - perlin3(y, 4.0, z, 4, 4, 7)).abs() < 1e-5);
            assert!((worley3(0.0, y, z, 5, 5, 1.0, 3).0 - worley3(5.0, y, z, 5, 5, 1.0, 3).0).abs() < 1e-4);
        }
        let mut lo = f32::MAX;
        let mut hi = f32::MIN;
        for k in 0..20000 {
            let p = [k as f32 * 0.131, k as f32 * 0.071 + 0.3, (k as f32 * 0.017).sin() * 9.0, k as f32 * 0.029];
            let v = simplex4(p, 1);
            lo = lo.min(v);
            hi = hi.max(v);
        }
        assert!(lo > -1.2 && hi < 1.2 && hi - lo > 1.0, "{lo} {hi}");
    }

    /// Each 3D noise matches across u = 0/1 and v = 0/1, and z changes it.
    #[test]
    fn noises_3d_tile_both_ways() {
        let no_res = |_: &str| None;
        let vars = BTreeMap::new();
        for id in ["perlin_noise_3d", "simplex_noise_3d", "worley_noise_3d"] {
            let def = find(id).unwrap();
            let cook = |z: f32| {
                let mut p = resolve_params(def, &BTreeMap::new());
                p.insert("z".into(), materialcraft_core::Value::Float(z));
                let ctx = Ctx { w: 64, h: 64, seed: 5, tile: true, inputs: &[], params: &p, resource: &no_res, vars: &vars };
                (def.cook)(&ctx).unwrap().remove(0)
            };
            let img = cook(0.5);
            let (mut seam_x, mut seam_y, mut inner) = (0.0f32, 0.0f32, 0.0f32);
            for i in 0..64 {
                seam_x += (img.get(63, i, false)[0] - img.get(0, i, false)[0]).abs();
                seam_y += (img.get(i, 63, false)[0] - img.get(i, 0, false)[0]).abs();
                inner += (img.get(31, i, false)[0] - img.get(32, i, false)[0]).abs() + (img.get(i, 31, false)[0] - img.get(i, 32, false)[0]).abs();
            }
            assert!(seam_x <= inner * 1.5 + 0.2 && seam_y <= inner * 1.5 + 0.2, "{id}: seams {seam_x} {seam_y} vs inner {inner}");
            assert_ne!(img, cook(1.7), "{id}: z changes the slice");
            let (lo, hi) = img.range();
            assert!(hi - lo > 0.3, "{id}: contrast {lo}..{hi}");
        }
    }
}
