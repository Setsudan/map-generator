use noise::{NoiseFn, Perlin};

/// Climate driven by geography, not random noise.
/// Temperature: latitude heat map (hot equator, cold poles) + elevation cooling + tiny smooth noise.
/// Humidity: ocean proximity + latitude moisture band + tiny smooth noise.
pub fn generate_climate(
    width: u32,
    height: u32,
    seed: u32,
    scale: f64,
    elevation: &[f64],
    sea_level: f64,
    equator_position: f64,
    temperature_noise: f64,
    lapse_rate: f64,
) -> (Vec<f64>, Vec<f64>) {
    let size = (width * height) as usize;
    let temp_noise = Perlin::new(seed.wrapping_add(2000));
    let humid_noise = Perlin::new(seed.wrapping_add(3000));

    let ocean_dist = distance_to_ocean(elevation, width, height, sea_level);
    let max_dist = (width.max(height) as f64) * 0.35;
    // Blur elevation so lapse-rate cooling does not facet along plate edges
    let elev_smooth = box_blur(elevation, width, height, 3);

    let mut temperature = vec![0.0f64; size];
    let mut humidity = vec![0.0f64; size];

    let h_denom = (height.saturating_sub(1).max(1)) as f64;
    let equator = equator_position.clamp(0.05, 0.95);

    for y in 0..height {
        for x in 0..width {
            let i = (y * width + x) as usize;
            let nx = x as f64 / scale;
            let ny = y as f64 / scale;

            // 0 = north (top), 1 = south (bottom). Poles cold, equator hot.
            // Warp latitude slightly so climate bands are not ruler-straight.
            let lat_warp = temp_noise.get([nx * 0.12, ny * 0.12]) * 0.14;
            let lat = (y as f64 / h_denom + lat_warp).clamp(0.0, 1.0);
            let dist_from_eq = (lat - equator).abs();
            let latitude_heat = (1.0 - (dist_from_eq / equator.max(1.0 - equator)).min(1.0))
                .clamp(0.0, 1.0);
            let latitude_heat = latitude_heat.powf(0.85);

            let land_h = (elev_smooth[i] - sea_level).max(0.0);
            let elevation_cooling = land_h * lapse_rate;

            let t_noise = temp_noise.get([nx * 0.08, ny * 0.08]) * temperature_noise;
            let temp = (latitude_heat * 0.92 + 0.05 + t_noise - elevation_cooling).clamp(0.0, 1.0);
            temperature[i] = temp;

            let dist = ocean_dist[i];
            let coastal = (1.0 - (dist / max_dist).clamp(0.0, 1.0)).powf(0.65);
            let tropical_moisture = (latitude_heat * 0.35).clamp(0.0, 0.35);
            let h_noise = humid_noise.get([nx * 0.06, ny * 0.06]) * 0.12;
            let orographic_dry = (land_h * 0.25).min(0.2);

            let humid = if elevation[i] < sea_level {
                1.0
            } else {
                (0.15 + coastal * 0.55 + tropical_moisture + h_noise - orographic_dry)
                    .clamp(0.0, 1.0)
            };
            humidity[i] = humid;
        }
    }

    // Stronger smooth so climate bands are continuous heat maps
    let mut temperature = box_blur(&temperature, width, height, 6);
    let mut humidity = box_blur(&humidity, width, height, 4);

    // Re-clamp after blur
    for t in temperature.iter_mut() {
        *t = t.clamp(0.0, 1.0);
    }
    for h in humidity.iter_mut() {
        *h = h.clamp(0.0, 1.0);
    }

    (temperature, humidity)
}

/// Approximate Euclidean distance (in pixels) from each cell to nearest ocean.
fn distance_to_ocean(elevation: &[f64], width: u32, height: u32, sea_level: f64) -> Vec<f64> {
    let size = (width * height) as usize;
    let mut dist = vec![f64::INFINITY; size];
    let mut queue: Vec<(u32, u32)> = Vec::new();

    for y in 0..height {
        for x in 0..width {
            let i = (y * width + x) as usize;
            if elevation[i] < sea_level {
                dist[i] = 0.0;
                queue.push((x, y));
            }
        }
    }

    // 8-connected with Euclidean step lengths (avoids Manhattan diamond seams)
    let neighbors: [(i32, i32, f64); 8] = [
        (1, 0, 1.0),
        (-1, 0, 1.0),
        (0, 1, 1.0),
        (0, -1, 1.0),
        (1, 1, std::f64::consts::SQRT_2),
        (1, -1, std::f64::consts::SQRT_2),
        (-1, 1, std::f64::consts::SQRT_2),
        (-1, -1, std::f64::consts::SQRT_2),
    ];

    let mut head = 0;
    while head < queue.len() {
        let (x, y) = queue[head];
        head += 1;
        let i = (y * width + x) as usize;
        let d = dist[i];
        for (dx, dy, step) in neighbors {
            let nx = x as i32 + dx;
            let ny = y as i32 + dy;
            if nx < 0 || ny < 0 || nx >= width as i32 || ny >= height as i32 {
                continue;
            }
            let ni = (ny as u32 * width + nx as u32) as usize;
            let nd = d + step;
            if nd < dist[ni] {
                dist[ni] = nd;
                queue.push((nx as u32, ny as u32));
            }
        }
    }

    let fallback = width.max(height) as f64;
    for d in dist.iter_mut() {
        if !d.is_finite() {
            *d = fallback;
        }
    }
    // Blur distance so moisture fronts are soft curves, not faceted
    box_blur(&dist, width, height, 4)
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

    // Horizontal
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

    // Vertical
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
