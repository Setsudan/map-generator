use crate::cli::WorldShape;
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};

#[derive(Clone, Debug)]
pub struct TectonicPlate {
    #[allow(dead_code)]
    pub id: usize,
    pub center_x: f64,
    pub center_y: f64,
    pub velocity_x: f64,
    pub velocity_y: f64,
    pub continental: bool,
    pub base_elevation: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoundaryType {
    None,
    Convergent,
    Divergent,
    Transform,
}

#[derive(Clone, Debug)]
pub struct TectonicsResult {
    pub plates: Vec<TectonicPlate>,
    pub plate_id: Vec<u16>,
    pub continentalness: Vec<f64>,
    pub tectonic: Vec<f64>,
    pub boundary: Vec<u8>,
}

const BOUNDARY_NONE: u8 = 0;
const BOUNDARY_CONVERGENT: u8 = 1;
const BOUNDARY_DIVERGENT: u8 = 2;
const BOUNDARY_TRANSFORM: u8 = 3;

pub fn generate_tectonics(
    width: u32,
    height: u32,
    seed: u32,
    plate_count: u32,
    world_shape: WorldShape,
    continent_count: u32,
    fragmentation: f64,
    plate_motion: f64,
    mountain_strength: f64,
    rift_strength: f64,
    volcanic_activity: f64,
) -> TectonicsResult {
    let mut rng = StdRng::seed_from_u64(seed as u64);
    let n = plate_count.max(2) as usize;
    let w = width as f64;
    let h = height as f64;

    let plates = create_plates(
        &mut rng,
        n,
        w,
        h,
        world_shape,
        continent_count,
        plate_motion,
    );

    let size = (width * height) as usize;
    let mut plate_id = vec![0u16; size];
    let mut continentalness = vec![0.0f64; size];
    let mut tectonic = vec![0.0f64; size];
    let mut boundary = vec![BOUNDARY_NONE; size];

    // Wider soft zone + warped lookups kill straight Voronoi cuts
    let boundary_width = (w.min(h) * 0.08).max(16.0);
    let warp_strength = w.min(h) * 0.08;
    let soft_power = 2.1;

    for y in 0..height {
        for x in 0..width {
            let idx = (y * width + x) as usize;
            let px = x as f64;
            let py = y as f64;

            // Domain-warp sample position so plate edges meander
            let (sx, sy) = warp_point(seed, px, py, warp_strength);

            let (nearest, second, d1, d2) = nearest_two(&plates, sx, sy);
            plate_id[idx] = nearest as u16;

            // Soft distance-weighted crust from several nearby plates
            continentalness[idx] =
                soft_continentalness(&plates, sx, sy, soft_power, seed, fragmentation);

            let dist_to_boundary = (d2 - d1).max(0.0);
            // Extra noise on boundary distance so the uplift band itself is wiggly
            let boundary_jitter =
                hash_noise(seed.wrapping_add(5200), sx * 0.025, sy * 0.025) * boundary_width * 0.35;
            let effective_dist = (dist_to_boundary + boundary_jitter).max(0.0);

            if effective_dist >= boundary_width {
                continue;
            }

            let a = &plates[nearest];
            let b = &plates[second];

            let (boundary_type, uplift) = classify_boundary(
                a,
                b,
                mountain_strength,
                rift_strength,
                volcanic_activity,
            );

            let falloff = 1.0 - (effective_dist / boundary_width);
            let falloff = falloff * falloff;
            let edge_jitter =
                0.55 + 0.45 * hash_noise(seed.wrapping_add(5000), sx * 0.03, sy * 0.03);
            // Break continuous straight ridges into irregular mountain clumps
            let ridge_break = 0.25
                + 0.75
                    * (0.5
                        + 0.5
                            * hash_noise(seed.wrapping_add(5300), sx * 0.035, sy * 0.035));
            let clump = (0.5
                + 0.5 * hash_noise(seed.wrapping_add(5400), sx * 0.012, sy * 0.012))
            .powf(1.4);

            tectonic[idx] = uplift * falloff * edge_jitter * ridge_break * clump;
            boundary[idx] = match boundary_type {
                BoundaryType::None => BOUNDARY_NONE,
                BoundaryType::Convergent => BOUNDARY_CONVERGENT,
                BoundaryType::Divergent => BOUNDARY_DIVERGENT,
                BoundaryType::Transform => BOUNDARY_TRANSFORM,
            };
        }
    }

    // Blur crust fields so land/ocean and mountain belts have organic edges
    continentalness = box_blur(&continentalness, width, height, 4);
    tectonic = box_blur(&tectonic, width, height, 5);
    tectonic = box_blur(&tectonic, width, height, 3);
    for v in continentalness.iter_mut() {
        *v = v.clamp(0.0, 1.0);
    }

    TectonicsResult {
        plates,
        plate_id,
        continentalness,
        tectonic,
        boundary,
    }
}

fn warp_point(seed: u32, x: f64, y: f64, strength: f64) -> (f64, f64) {
    // Two octaves of value noise for large coastal meanders
    let n1x = hash_noise(seed.wrapping_add(6000), x * 0.008, y * 0.008);
    let n1y = hash_noise(seed.wrapping_add(6001), x * 0.008 + 40.0, y * 0.008 + 17.0);
    let n2x = hash_noise(seed.wrapping_add(6002), x * 0.02, y * 0.02);
    let n2y = hash_noise(seed.wrapping_add(6003), x * 0.02 + 11.0, y * 0.02 + 29.0);
    let wx = (n1x * 0.7 + n2x * 0.3) * strength;
    let wy = (n1y * 0.7 + n2y * 0.3) * strength;
    (x + wx, y + wy)
}

fn soft_continentalness(
    plates: &[TectonicPlate],
    x: f64,
    y: f64,
    power: f64,
    seed: u32,
    fragmentation: f64,
) -> f64 {
    // Inverse-distance blend across all plates (smooth ownership)
    let mut sum_w = 0.0;
    let mut sum = 0.0;
    let mut best_w = 0.0;
    let mut second_w = 0.0;
    for p in plates {
        let d = (p.center_x - x).hypot(p.center_y - y).max(1.0);
        let w = 1.0 / d.powf(power);
        sum += p.base_elevation * w;
        sum_w += w;
        if w > best_w {
            second_w = best_w;
            best_w = w;
        } else if w > second_w {
            second_w = w;
        }
    }
    let base = sum / sum_w.max(1e-9);

    // How close we are to a plate–plate seam (1 = on the seam)
    let seam = if best_w + second_w > 1e-9 {
        1.0 - ((best_w - second_w) / (best_w + second_w)).abs().clamp(0.0, 1.0)
    } else {
        0.0
    };
    let seam = seam.powf(0.65);

    let frag = hash_noise(seed.wrapping_add(4000), x * 0.004, y * 0.004) * fragmentation * 0.22;
    let micro = hash_noise(seed.wrapping_add(4100), x * 0.018, y * 0.018) * fragmentation * 0.10;
    let mid = hash_noise(seed.wrapping_add(4200), x * 0.01, y * 0.01) * 0.07;
    // Extra displacement exactly on plate seams so midlines are not straight
    let seam_noise = hash_noise(seed.wrapping_add(4400), x * 0.015, y * 0.015) * seam * 0.14
        + hash_noise(seed.wrapping_add(4500), x * 0.028, y * 0.028) * seam * 0.08;

    let mut v = base + frag + micro + mid + seam_noise;
    let coast_band = 1.0 - ((v - 0.28).abs() / 0.14).clamp(0.0, 1.0);
    v += hash_noise(seed.wrapping_add(4300), x * 0.012, y * 0.012) * coast_band * 0.10;
    v.clamp(0.0, 1.0)
}

fn box_blur(src: &[f64], width: u32, height: u32, radius: i32) -> Vec<f64> {
    if radius <= 0 {
        return src.to_vec();
    }
    let size = src.len();
    let mut tmp = vec![0.0f64; size];
    let mut out = vec![0.0f64; size];
    let w = width as i32;
    let h = height as i32;

    for y in 0..h {
        for x in 0..w {
            let mut sum = 0.0;
            let mut count = 0.0;
            for dx in -radius..=radius {
                let nx = (x + dx).clamp(0, w - 1);
                sum += src[(y * w + nx) as usize];
                count += 1.0;
            }
            tmp[(y * w + x) as usize] = sum / count;
        }
    }
    for y in 0..h {
        for x in 0..w {
            let mut sum = 0.0;
            let mut count = 0.0;
            for dy in -radius..=radius {
                let ny = (y + dy).clamp(0, h - 1);
                sum += tmp[(ny * w + x) as usize];
                count += 1.0;
            }
            out[(y * w + x) as usize] = sum / count;
        }
    }
    out
}

fn create_plates(
    rng: &mut StdRng,
    n: usize,
    w: f64,
    h: f64,
    world_shape: WorldShape,
    continent_count: u32,
    plate_motion: f64,
) -> Vec<TectonicPlate> {
    let centers = match world_shape {
        WorldShape::Pangaea | WorldShape::Supercontinent => {
            clustered_centers(rng, n, w, h, 0.35)
        }
        WorldShape::Continent => clustered_centers(rng, n, w, h, 0.5),
        WorldShape::InlandSea => clustered_centers(rng, n, w, h, 0.45),
        WorldShape::Archipelago => scattered_centers(rng, n, w, h),
        WorldShape::Random => scattered_centers(rng, n, w, h),
    };

    let continental_flags =
        assign_continental(rng, n, world_shape, continent_count as usize, &centers, w, h);

    centers
        .into_iter()
        .enumerate()
        .map(|(id, (cx, cy))| {
            let angle = rng.gen_range(0.0..std::f64::consts::TAU);
            let speed = rng.gen_range(0.4..1.0) * plate_motion;
            let continental = continental_flags[id];
            let base_elevation = if continental {
                rng.gen_range(0.42..0.62)
            } else {
                rng.gen_range(0.05..0.18)
            };
            TectonicPlate {
                id,
                center_x: cx,
                center_y: cy,
                velocity_x: angle.cos() * speed,
                velocity_y: angle.sin() * speed,
                continental,
                base_elevation,
            }
        })
        .collect()
}

fn scattered_centers(rng: &mut StdRng, n: usize, w: f64, h: f64) -> Vec<(f64, f64)> {
    (0..n)
        .map(|_| (rng.gen_range(0.0..w), rng.gen_range(0.0..h)))
        .collect()
}

fn clustered_centers(rng: &mut StdRng, n: usize, w: f64, h: f64, spread: f64) -> Vec<(f64, f64)> {
    let cluster_cx = rng.gen_range(w * 0.3..w * 0.7);
    let cluster_cy = rng.gen_range(h * 0.3..h * 0.7);
    let radius = w.min(h) * spread;
    (0..n)
        .map(|i| {
            if i == 0 {
                (cluster_cx, cluster_cy)
            } else if i < n / 2 + 1 {
                let a = rng.gen_range(0.0..std::f64::consts::TAU);
                let r = rng.gen_range(0.0..radius);
                (
                    (cluster_cx + a.cos() * r).clamp(0.0, w - 1.0),
                    (cluster_cy + a.sin() * r).clamp(0.0, h - 1.0),
                )
            } else {
                (rng.gen_range(0.0..w), rng.gen_range(0.0..h))
            }
        })
        .collect()
}

fn assign_continental(
    rng: &mut StdRng,
    n: usize,
    world_shape: WorldShape,
    continent_count: usize,
    centers: &[(f64, f64)],
    w: f64,
    h: f64,
) -> Vec<bool> {
    let mut flags = vec![false; n];
    let count = match world_shape {
        WorldShape::Pangaea | WorldShape::Supercontinent => (n * 2 / 3).max(2).min(n),
        WorldShape::Continent => continent_count.max(1).min(n / 2 + 1).min(n),
        WorldShape::Archipelago => (n / 2).max(3).min(n),
        WorldShape::InlandSea => (n * 2 / 3).max(3).min(n),
        WorldShape::Random => continent_count.max(1).min(n),
    };

    match world_shape {
        WorldShape::Pangaea | WorldShape::Supercontinent | WorldShape::Continent => {
            // Prefer plates near the cluster centroid of first half
            let mut order: Vec<usize> = (0..n).collect();
            let cx = w * 0.5;
            let cy = h * 0.5;
            order.sort_by(|&a, &b| {
                let da = (centers[a].0 - cx).powi(2) + (centers[a].1 - cy).powi(2);
                let db = (centers[b].0 - cx).powi(2) + (centers[b].1 - cy).powi(2);
                da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
            });
            for &i in order.iter().take(count) {
                flags[i] = true;
            }
        }
        WorldShape::InlandSea => {
            let mut order: Vec<usize> = (0..n).collect();
            let cx = w * 0.5;
            let cy = h * 0.5;
            order.sort_by(|&a, &b| {
                let da = (centers[a].0 - cx).powi(2) + (centers[a].1 - cy).powi(2);
                let db = (centers[b].0 - cx).powi(2) + (centers[b].1 - cy).powi(2);
                da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
            });
            // Outer ring continental, innermost oceanic (inland sea)
            for &i in order.iter().take(count).skip(1) {
                flags[i] = true;
            }
            if count > 1 {
                // also mark a few mid-distance as continental
                for &i in order.iter().skip(count).take(count / 2) {
                    flags[i] = true;
                }
            }
            flags[order[0]] = false; // center oceanic
        }
        WorldShape::Archipelago | WorldShape::Random => {
            let mut indices: Vec<usize> = (0..n).collect();
            for i in (1..n).rev() {
                let j = rng.gen_range(0..=i);
                indices.swap(i, j);
            }
            for &i in indices.iter().take(count) {
                flags[i] = true;
            }
        }
    }

    if !flags.iter().any(|&c| c) {
        flags[0] = true;
    }
    flags
}

fn nearest_two(plates: &[TectonicPlate], x: f64, y: f64) -> (usize, usize, f64, f64) {
    let mut best = 0usize;
    let mut second = 0usize;
    let mut d1 = f64::INFINITY;
    let mut d2 = f64::INFINITY;

    for (i, p) in plates.iter().enumerate() {
        let d = (p.center_x - x).hypot(p.center_y - y);
        if d < d1 {
            d2 = d1;
            second = best;
            d1 = d;
            best = i;
        } else if d < d2 {
            d2 = d;
            second = i;
        }
    }
    if second == best && plates.len() > 1 {
        second = if best == 0 { 1 } else { 0 };
        d2 = (plates[second].center_x - x).hypot(plates[second].center_y - y);
    }
    (best, second, d1, d2)
}

fn classify_boundary(
    a: &TectonicPlate,
    b: &TectonicPlate,
    mountain_strength: f64,
    rift_strength: f64,
    volcanic_activity: f64,
) -> (BoundaryType, f64) {
    let mut nx = b.center_x - a.center_x;
    let mut ny = b.center_y - a.center_y;
    let len = (nx * nx + ny * ny).sqrt().max(1e-6);
    nx /= len;
    ny /= len;

    // Relative velocity of a toward b along the normal (positive = approaching)
    let rel_x = a.velocity_x - b.velocity_x;
    let rel_y = a.velocity_y - b.velocity_y;
    let approach = -(rel_x * nx + rel_y * ny);
    let tangential = (-rel_x * ny + rel_y * nx).abs();

    let threshold = 0.15;
    if approach > threshold {
        let strength = approach.clamp(0.0, 2.0);
        let uplift = match (a.continental, b.continental) {
            (true, true) => mountain_strength * 0.55 * strength,
            (true, false) | (false, true) => {
                mountain_strength * 0.35 * strength + volcanic_activity * 0.15
            }
            (false, false) => volcanic_activity * 0.25 * strength + 0.05,
        };
        (BoundaryType::Convergent, uplift)
    } else if approach < -threshold {
        let strength = (-approach).clamp(0.0, 2.0);
        let uplift = match (a.continental, b.continental) {
            (true, true) => -rift_strength * 0.35 * strength,
            (false, false) => rift_strength * 0.12 * strength, // ocean ridge rises a bit
            _ => -rift_strength * 0.2 * strength,
        };
        (BoundaryType::Divergent, uplift)
    } else if tangential > threshold {
        (BoundaryType::Transform, -0.04 * tangential.min(1.5))
    } else {
        (BoundaryType::None, 0.0)
    }
}

fn hash_noise(seed: u32, x: f64, y: f64) -> f64 {
    // Lightweight value noise in [-1, 1] without allocating Fbm
    let ix = x.floor() as i32;
    let iy = y.floor() as i32;
    let fx = x - ix as f64;
    let fy = y - iy as f64;
    let sx = fx * fx * (3.0 - 2.0 * fx);
    let sy = fy * fy * (3.0 - 2.0 * fy);

    let n00 = hash2(seed, ix, iy);
    let n10 = hash2(seed, ix + 1, iy);
    let n01 = hash2(seed, ix, iy + 1);
    let n11 = hash2(seed, ix + 1, iy + 1);

    let nx0 = n00 + (n10 - n00) * sx;
    let nx1 = n01 + (n11 - n01) * sx;
    nx0 + (nx1 - nx0) * sy
}

fn hash2(seed: u32, x: i32, y: i32) -> f64 {
    let mut n = seed
        .wrapping_mul(374761393)
        .wrapping_add((x as u32).wrapping_mul(668265263))
        .wrapping_add((y as u32).wrapping_mul(2147483647));
    n = (n ^ (n >> 13)).wrapping_mul(1274126177);
    n = n ^ (n >> 16);
    (n as f64 / u32::MAX as f64) * 2.0 - 1.0
}

pub fn boundary_type_from_u8(v: u8) -> BoundaryType {
    match v {
        BOUNDARY_CONVERGENT => BoundaryType::Convergent,
        BOUNDARY_DIVERGENT => BoundaryType::Divergent,
        BOUNDARY_TRANSFORM => BoundaryType::Transform,
        _ => BoundaryType::None,
    }
}
