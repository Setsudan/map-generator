use noise::{Fbm, MultiFractal, NoiseFn, Perlin};

/// Multi-scale noise elevation detail. Plates set the base; noise shapes hills and valleys.
pub fn generate_detail(
    width: u32,
    height: u32,
    seed: u32,
    scale: f64,
    domain_warp: bool,
) -> (Vec<f64>, Vec<f64>) {
    let size = (width * height) as usize;
    let mut mountain = vec![0.0f64; size];
    let mut local = vec![0.0f64; size];

    let macro_fbm = Fbm::<Perlin>::new(seed.wrapping_add(7100))
        .set_octaves(3)
        .set_lacunarity(2.0)
        .set_persistence(0.55);

    let mid_fbm = Fbm::<Perlin>::new(seed)
        .set_octaves(5)
        .set_lacunarity(2.0)
        .set_persistence(0.5);

    let mountain_fbm = Fbm::<Perlin>::new(seed.wrapping_add(7000))
        .set_octaves(4)
        .set_lacunarity(2.2)
        .set_persistence(0.45);

    let fine_fbm = Fbm::<Perlin>::new(seed.wrapping_add(7200))
        .set_octaves(4)
        .set_lacunarity(2.3)
        .set_persistence(0.4);

    let fbm_warp = if domain_warp {
        Some(
            Fbm::<Perlin>::new(seed.wrapping_add(1000))
                .set_octaves(4)
                .set_lacunarity(2.1)
                .set_persistence(0.4),
        )
    } else {
        None
    };

    for y in 0..height {
        for x in 0..width {
            let idx = (y * width + x) as usize;
            let nx = x as f64 / scale;
            let ny = y as f64 / scale;

            let (sx, sy) = if let Some(ref warp) = fbm_warp {
                let warp_amount = 25.0;
                let wx = warp.get([nx * 0.5, ny * 0.5]) * warp_amount;
                let wy = warp.get([nx * 0.5 + 0.5, ny * 0.5 + 0.5]) * warp_amount;
                (nx + wx, ny + wy)
            } else {
                (nx, ny)
            };

            // Low-frequency rolling hills / basins
            let macro_n = macro_fbm.get([sx * 0.35, sy * 0.35]);
            // Mid-frequency terrain
            let mid_n = mid_fbm.get([sx, sy]);
            // Fine bumps
            let fine_n = fine_fbm.get([sx * 2.2, sy * 2.2]);

            local[idx] = macro_n * 0.12 + mid_n * 0.10 + fine_n * 0.04;

            // Ridged mountain noise (peaks)
            let m = mountain_fbm.get([sx * 0.55, sy * 0.55]);
            let ridged = (1.0 - m.abs()).powi(2);
            mountain[idx] = ridged * 0.22;
        }
    }

    (mountain, local)
}

/// Noise-only elevation fallback when tectonics are disabled.
pub fn generate_noise_only_elevation(
    width: u32,
    height: u32,
    seed: u32,
    scale: f64,
    domain_warp: bool,
) -> Vec<f64> {
    let fbm = Fbm::<Perlin>::new(seed)
        .set_octaves(6)
        .set_lacunarity(2.0)
        .set_persistence(0.5);

    let low_fbm = Fbm::<Perlin>::new(seed.wrapping_add(9000))
        .set_octaves(3)
        .set_lacunarity(2.0)
        .set_persistence(0.5);

    let fbm_warp = if domain_warp {
        Some(
            Fbm::<Perlin>::new(seed.wrapping_add(1000))
                .set_octaves(4)
                .set_lacunarity(2.1)
                .set_persistence(0.4),
        )
    } else {
        None
    };

    let mut data = Vec::with_capacity((width * height) as usize);
    for y in 0..height {
        for x in 0..width {
            let nx = x as f64 / scale;
            let ny = y as f64 / scale;
            let (sx, sy) = if let Some(ref warp) = fbm_warp {
                let warp_amount = 25.0;
                let wx = warp.get([nx * 0.5, ny * 0.5]) * warp_amount;
                let wy = warp.get([nx * 0.5 + 0.5, ny * 0.5 + 0.5]) * warp_amount;
                (nx + wx, ny + wy)
            } else {
                (nx, ny)
            };
            let raw = fbm.get([sx, sy]);
            let mut height_v = 1.0 - raw.abs();
            height_v = height_v.powi(2);
            let low = low_fbm.get([nx * 0.15, ny * 0.15]);
            height_v = (height_v * 0.55 + (low * 0.5 + 0.5) * 0.45).clamp(0.0, 1.0);
            data.push(height_v);
        }
    }
    data
}

pub fn combine_elevation(
    continentalness: &[f64],
    tectonic: &[f64],
    mountain: &[f64],
    local: &[f64],
) -> Vec<f64> {
    // Continental crust + tectonics define landmasses; noise drives elevation variety
    continentalness
        .iter()
        .zip(tectonic.iter())
        .zip(mountain.iter())
        .zip(local.iter())
        .map(|(((c, t), m), l)| {
            // Noise modulates continental plate height so plains/hills vary
            let noise_shaped = c * (0.82 + l.max(-0.15) * 0.9) + t + m + l * 0.35;
            noise_shaped.clamp(0.0, 1.0)
        })
        .collect()
}

/// Light blur + shoreline noise so coasts are not polygonal.
pub fn soften_elevation(
    elevation: &[f64],
    width: u32,
    height: u32,
    seed: u32,
    sea_level: f64,
) -> Vec<f64> {
    let blurred = soft_box_blur(elevation, width, height, 2);
    let mut out = blurred;
    for y in 0..height {
        for x in 0..width {
            let i = (y * width + x) as usize;
            let h = out[i];
            let coast = 1.0 - ((h - sea_level).abs() / 0.08).clamp(0.0, 1.0);
            if coast > 0.0 {
                let n = shoreline_noise(seed, x as f64, y as f64);
                out[i] = (h + n * coast * 0.045).clamp(0.0, 1.0);
            }
        }
    }
    soft_box_blur(&out, width, height, 1)
}

fn shoreline_noise(seed: u32, x: f64, y: f64) -> f64 {
    let mut n = seed
        .wrapping_mul(374761393)
        .wrapping_add((x as i32 as u32).wrapping_mul(668265263))
        .wrapping_add((y as i32 as u32).wrapping_mul(2147483647));
    n = (n ^ (n >> 13)).wrapping_mul(1274126177);
    n ^= n >> 16;
    let a = (n as f64 / u32::MAX as f64) * 2.0 - 1.0;

    let fx = x * 0.04;
    let fy = y * 0.04;
    let ix = fx.floor() as i32;
    let iy = fy.floor() as i32;
    let tx = fx - ix as f64;
    let ty = fy - iy as f64;
    let sx = tx * tx * (3.0 - 2.0 * tx);
    let sy = ty * ty * (3.0 - 2.0 * ty);
    let h = |x: i32, y: i32| {
        let mut n = seed
            .wrapping_add(777)
            .wrapping_mul(374761393)
            .wrapping_add((x as u32).wrapping_mul(668265263))
            .wrapping_add((y as u32).wrapping_mul(2147483647));
        n = (n ^ (n >> 13)).wrapping_mul(1274126177);
        n ^= n >> 16;
        (n as f64 / u32::MAX as f64) * 2.0 - 1.0
    };
    let n00 = h(ix, iy);
    let n10 = h(ix + 1, iy);
    let n01 = h(ix, iy + 1);
    let n11 = h(ix + 1, iy + 1);
    let nx0 = n00 + (n10 - n00) * sx;
    let nx1 = n01 + (n11 - n01) * sx;
    let b = nx0 + (nx1 - nx0) * sy;
    a * 0.25 + b * 0.75
}

fn soft_box_blur(src: &[f64], width: u32, height: u32, radius: i32) -> Vec<f64> {
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
