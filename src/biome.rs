#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum BiomeId {
    DeepOcean = 0,
    Ocean = 1,
    ShallowOcean = 2,
    Beach = 3,
    Cliff = 4,
    Desert = 5,
    Jungle = 6,
    Taiga = 7,
    Tundra = 8,
    Grassland = 9,
    Forest = 10,
    Rock = 11,
    Snow = 12,
    Scrub = 13,
    Heath = 14,
    /// Middle between lowland and rock/snow
    Foothill = 15,
    /// Cold high middle (between forest/taiga and snow)
    Alpine = 16,
    /// Wet middle near inland water / river valleys
    Wetland = 17,
}

impl BiomeId {
    pub fn from_u8(v: u8) -> Self {
        match v {
            1 => BiomeId::Ocean,
            2 => BiomeId::ShallowOcean,
            3 => BiomeId::Beach,
            4 => BiomeId::Cliff,
            5 => BiomeId::Desert,
            6 => BiomeId::Jungle,
            7 => BiomeId::Taiga,
            8 => BiomeId::Tundra,
            9 => BiomeId::Grassland,
            10 => BiomeId::Forest,
            11 => BiomeId::Rock,
            12 => BiomeId::Snow,
            13 => BiomeId::Scrub,
            14 => BiomeId::Heath,
            15 => BiomeId::Foothill,
            16 => BiomeId::Alpine,
            17 => BiomeId::Wetland,
            _ => BiomeId::DeepOcean,
        }
    }

    pub fn palette_rgb(self) -> [u8; 3] {
        let c = self.rgb01();
        [
            (c[0] * 255.0) as u8,
            (c[1] * 255.0) as u8,
            (c[2] * 255.0) as u8,
        ]
    }

    pub fn rgb01(self) -> [f64; 3] {
        match self {
            BiomeId::DeepOcean => [0.05, 0.15, 0.40],
            BiomeId::Ocean => [0.10, 0.30, 0.60],
            BiomeId::ShallowOcean => [0.20, 0.50, 0.75],
            BiomeId::Beach => [0.90, 0.84, 0.62],
            BiomeId::Cliff => [0.42, 0.40, 0.38],
            BiomeId::Desert => [0.86, 0.78, 0.50],
            BiomeId::Jungle => [0.10, 0.48, 0.12],
            BiomeId::Taiga => [0.22, 0.40, 0.25],
            BiomeId::Tundra => [0.78, 0.82, 0.80],
            BiomeId::Grassland => [0.35, 0.62, 0.25],
            BiomeId::Forest => [0.15, 0.42, 0.15],
            BiomeId::Rock => [0.50, 0.45, 0.40],
            BiomeId::Snow => [0.95, 0.95, 1.0],
            BiomeId::Scrub => [0.70, 0.68, 0.40],
            BiomeId::Heath => [0.55, 0.60, 0.45],
            BiomeId::Foothill => [0.40, 0.48, 0.32],
            BiomeId::Alpine => [0.62, 0.66, 0.58],
            BiomeId::Wetland => [0.25, 0.45, 0.35],
        }
    }
}

fn smoothstep(edge0: f64, edge1: f64, x: f64) -> f64 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn lerp3(a: [f64; 3], b: [f64; 3], t: f64) -> [f64; 3] {
    [
        a[0] * (1.0 - t) + b[0] * t,
        a[1] * (1.0 - t) + b[1] * t,
        a[2] * (1.0 - t) + b[2] * t,
    ]
}

/// Soft climate color with overlapping middle biomes.
fn climate_color(temp: f64, humid: f64) -> ([f64; 3], BiomeId) {
    let w_desert = smoothstep(0.55, 0.75, temp) * (1.0 - smoothstep(0.30, 0.50, humid));
    let w_scrub = smoothstep(0.45, 0.62, temp)
        * (1.0 - smoothstep(0.35, 0.55, humid))
        * (1.0 - w_desert * 0.5);
    let w_jungle = smoothstep(0.55, 0.72, temp) * smoothstep(0.45, 0.65, humid);
    let w_tundra = (1.0 - smoothstep(0.20, 0.38, temp)) * (1.0 - smoothstep(0.35, 0.55, humid));
    let w_taiga = (1.0 - smoothstep(0.22, 0.40, temp)) * smoothstep(0.35, 0.55, humid);
    let w_heath = (1.0 - smoothstep(0.30, 0.48, temp))
        * smoothstep(0.18, 0.35, temp)
        * (1.0 - w_tundra)
        * (1.0 - w_taiga);
    let w_grass = smoothstep(0.35, 0.52, temp)
        * (1.0 - smoothstep(0.58, 0.75, temp))
        * (1.0 - smoothstep(0.40, 0.58, humid));
    let w_forest = smoothstep(0.35, 0.52, temp)
        * (1.0 - smoothstep(0.62, 0.80, temp))
        * smoothstep(0.40, 0.58, humid);

    let candidates: [(BiomeId, f64); 8] = [
        (BiomeId::Desert, w_desert),
        (BiomeId::Scrub, w_scrub.max(0.05)),
        (BiomeId::Jungle, w_jungle),
        (BiomeId::Tundra, w_tundra),
        (BiomeId::Taiga, w_taiga),
        (BiomeId::Heath, w_heath.max(0.05)),
        (BiomeId::Grassland, w_grass.max(0.08)),
        (BiomeId::Forest, w_forest.max(0.08)),
    ];

    let mut sum_w = 0.0;
    let mut rgb = [0.0; 3];
    let mut best_id = BiomeId::Grassland;
    let mut best_w = -1.0;
    for (id, w) in candidates {
        if w <= 0.0 {
            continue;
        }
        let c = id.rgb01();
        rgb[0] += c[0] * w;
        rgb[1] += c[1] * w;
        rgb[2] += c[2] * w;
        sum_w += w;
        if w > best_w {
            best_w = w;
            best_id = id;
        }
    }
    if sum_w < 1e-6 {
        return (BiomeId::Grassland.rgb01(), BiomeId::Grassland);
    }
    (
        [rgb[0] / sum_w, rgb[1] / sum_w, rgb[2] / sum_w],
        best_id,
    )
}

/// Elevation middle biomes: foothill, alpine, rock, snow — soft blends.
fn altitude_mix(
    rgb: [f64; 3],
    base_id: BiomeId,
    elevation: f64,
    temp: f64,
    sea_level: f64,
) -> ([f64; 3], BiomeId) {
    let h = elevation;
    let _ = sea_level;

    if h > 0.88 {
        return (BiomeId::Snow.rgb01(), BiomeId::Snow);
    }

    // Alpine: cold + high, middle between taiga/heath and snow
    if h > 0.62 && temp < 0.45 {
        let t = smoothstep(0.62, 0.85, h);
        let alpine = BiomeId::Alpine.rgb01();
        let snow = BiomeId::Snow.rgb01();
        if t > 0.7 {
            return (lerp3(alpine, snow, (t - 0.7) / 0.3), BiomeId::Snow);
        }
        return (lerp3(rgb, alpine, t.min(1.0)), BiomeId::Alpine);
    }

    if h > 0.72 {
        let t = smoothstep(0.72, 0.88, h);
        return (
            lerp3(BiomeId::Rock.rgb01(), BiomeId::Snow.rgb01(), t),
            if t > 0.5 {
                BiomeId::Snow
            } else {
                BiomeId::Rock
            },
        );
    }

    // Foothill: middle between lowland biome and rock
    if h > 0.48 {
        let t = smoothstep(0.48, 0.72, h);
        let foothill = BiomeId::Foothill.rgb01();
        let rock = BiomeId::Rock.rgb01();
        if t > 0.65 {
            return (
                lerp3(foothill, rock, (t - 0.65) / 0.35),
                if t > 0.85 {
                    BiomeId::Rock
                } else {
                    BiomeId::Foothill
                },
            );
        }
        return (lerp3(rgb, foothill, t), BiomeId::Foothill);
    }

    (rgb, base_id)
}

#[allow(dead_code)]
pub fn classify_climate_biome(elevation: f64, temp: f64, humid: f64, sea_level: f64) -> BiomeId {
    if elevation < sea_level {
        return BiomeId::Ocean;
    }
    let (rgb, id) = climate_color(temp, humid);
    altitude_mix(rgb, id, elevation, temp, sea_level).1
}

pub fn generate_biomes(
    elevation: &[f64],
    temperature: &[f64],
    humidity: &[f64],
    width: u32,
    height: u32,
    sea_level: f64,
    blend_strength: f64,
    river_mask: &[bool],
    lake_mask: &[bool],
) -> (Vec<u8>, Vec<[f64; 3]>) {
    let size = elevation.len();
    let mut ids = vec![0u8; size];
    let mut colors = vec![[0.0; 3]; size];

    let slope = compute_slope(elevation, width, height);
    let coastal = coastal_band(elevation, width, height, sea_level, 5);

    for i in 0..size {
        let h = elevation[i];
        if h < sea_level {
            let depth = sea_level - h;
            let (id, rgb) = if depth > 0.12 {
                (BiomeId::DeepOcean, BiomeId::DeepOcean.rgb01())
            } else if depth > 0.04 {
                let t = smoothstep(0.04, 0.12, depth);
                (
                    BiomeId::Ocean,
                    lerp3(BiomeId::Ocean.rgb01(), BiomeId::DeepOcean.rgb01(), t),
                )
            } else {
                (BiomeId::ShallowOcean, BiomeId::ShallowOcean.rgb01())
            };
            ids[i] = id as u8;
            colors[i] = rgb;
            continue;
        }

        let (rgb, id) = climate_color(temperature[i], humidity[i]);
        let (mixed, alt_id) = altitude_mix(rgb, id, h, temperature[i], sea_level);
        ids[i] = alt_id as u8;
        colors[i] = mixed;
    }

    // Cliffs where low elevation meets high elevation (steep relief)
    let cliff_threshold = 0.09;
    for y in 0..height {
        for x in 0..width {
            let i = (y * width + x) as usize;
            if elevation[i] < sea_level {
                continue;
            }
            let s = slope[i];
            if s < cliff_threshold {
                continue;
            }

            // Confirm a true high/low meet: neighbor much lower or higher
            let mut low_meet_high = false;
            for (dx, dy) in [(-1i32, 0), (1, 0), (0, -1), (0, 1)] {
                let nx = x as i32 + dx;
                let ny = y as i32 + dy;
                if nx < 0 || ny < 0 || nx >= width as i32 || ny >= height as i32 {
                    continue;
                }
                let ni = (ny as u32 * width + nx as u32) as usize;
                let dh = (elevation[i] - elevation[ni]).abs();
                if dh >= cliff_threshold
                    && (elevation[i].min(elevation[ni]) < 0.55
                        || elevation[i].max(elevation[ni]) > 0.50)
                {
                    low_meet_high = true;
                    break;
                }
            }
            if !low_meet_high {
                continue;
            }

            let cliff = BiomeId::Cliff.rgb01();
            let t = ((s - cliff_threshold) / 0.12).clamp(0.35, 0.9);
            ids[i] = BiomeId::Cliff as u8;
            colors[i] = lerp3(colors[i], cliff, t);
        }
    }

    // Coastal middle-men: beach / cliff / shallow
    for y in 0..height {
        for x in 0..width {
            let i = (y * width + x) as usize;
            let h = elevation[i];
            let s = slope[i];
            if !coastal[i] {
                continue;
            }

            if h < sea_level {
                let depth = sea_level - h;
                if depth < 0.07 {
                    ids[i] = BiomeId::ShallowOcean as u8;
                    let shallow = BiomeId::ShallowOcean.rgb01();
                    let t = (depth / 0.07).clamp(0.0, 1.0);
                    colors[i] = lerp3(shallow, colors[i], t);
                }
                continue;
            }

            if s > 0.10 || ids[i] == BiomeId::Cliff as u8 {
                ids[i] = BiomeId::Cliff as u8;
                colors[i] = lerp3(colors[i], BiomeId::Cliff.rgb01(), 0.7);
            } else {
                ids[i] = BiomeId::Beach as u8;
                colors[i] = lerp3(colors[i], BiomeId::Beach.rgb01(), 0.75);
            }
        }
    }

    // Wetland middle biome: low flat land near rivers/lakes
    for y in 0..height {
        for x in 0..width {
            let i = (y * width + x) as usize;
            if elevation[i] < sea_level || elevation[i] > sea_level + 0.12 {
                continue;
            }
            if slope[i] > 0.05 {
                continue;
            }
            let near_water = lake_mask[i]
                || river_mask[i]
                || coastal[i]
                || has_water_neighbor(elevation, river_mask, lake_mask, width, height, x, y, sea_level, 3);
            if near_water && humidity[i] > 0.45 {
                ids[i] = BiomeId::Wetland as u8;
                colors[i] = lerp3(colors[i], BiomeId::Wetland.rgb01(), 0.55);
            }
        }
    }

    let strength = blend_strength.clamp(0.0, 1.0);
    if strength > 0.0 {
        colors = blend_colors(&colors, &ids, width, height, strength);
        colors = blend_colors(&colors, &ids, width, height, strength * 0.65);
    }

    (ids, colors)
}

fn has_water_neighbor(
    elevation: &[f64],
    river_mask: &[bool],
    lake_mask: &[bool],
    width: u32,
    height: u32,
    x: u32,
    y: u32,
    sea_level: f64,
    radius: i32,
) -> bool {
    for dy in -radius..=radius {
        for dx in -radius..=radius {
            let nx = x as i32 + dx;
            let ny = y as i32 + dy;
            if nx < 0 || ny < 0 || nx >= width as i32 || ny >= height as i32 {
                continue;
            }
            let ni = (ny as u32 * width + nx as u32) as usize;
            if elevation[ni] < sea_level || river_mask[ni] || lake_mask[ni] {
                return true;
            }
        }
    }
    false
}

fn compute_slope(elevation: &[f64], width: u32, height: u32) -> Vec<f64> {
    let mut slope = vec![0.0f64; elevation.len()];
    for y in 0..height {
        for x in 0..width {
            let i = (y * width + x) as usize;
            let h = elevation[i];
            let mut max_d = 0.0f64;
            for (dx, dy) in [(-1i32, 0), (1, 0), (0, -1), (0, 1)] {
                let nx = x as i32 + dx;
                let ny = y as i32 + dy;
                if nx < 0 || ny < 0 || nx >= width as i32 || ny >= height as i32 {
                    continue;
                }
                let nh = elevation[(ny as u32 * width + nx as u32) as usize];
                max_d = max_d.max((h - nh).abs());
            }
            slope[i] = max_d;
        }
    }
    slope
}

fn coastal_band(
    elevation: &[f64],
    width: u32,
    height: u32,
    sea_level: f64,
    radius: i32,
) -> Vec<bool> {
    let size = elevation.len();
    let mut coast = vec![false; size];
    for y in 0..height {
        for x in 0..width {
            let i = (y * width + x) as usize;
            let land = elevation[i] >= sea_level;
            let mut found = false;
            'search: for dy in -radius..=radius {
                for dx in -radius..=radius {
                    if dx == 0 && dy == 0 {
                        continue;
                    }
                    let nx = x as i32 + dx;
                    let ny = y as i32 + dy;
                    if nx < 0 || ny < 0 || nx >= width as i32 || ny >= height as i32 {
                        continue;
                    }
                    let n_land =
                        elevation[(ny as u32 * width + nx as u32) as usize] >= sea_level;
                    if n_land != land {
                        found = true;
                        break 'search;
                    }
                }
            }
            coast[i] = found;
        }
    }
    coast
}

fn blend_colors(
    colors: &[[f64; 3]],
    biome_ids: &[u8],
    width: u32,
    height: u32,
    strength: f64,
) -> Vec<[f64; 3]> {
    let strength = strength.clamp(0.0, 1.0);
    let mut out = colors.to_vec();
    for y in 0..height {
        for x in 0..width {
            let i = (y * width + x) as usize;
            let mut sum = colors[i];
            let mut w = 1.0;
            for (dx, dy) in [
                (-1i32, 0),
                (1, 0),
                (0, -1),
                (0, 1),
                (-1, -1),
                (1, -1),
                (-1, 1),
                (1, 1),
            ] {
                let nx = x as i32 + dx;
                let ny = y as i32 + dy;
                if nx < 0 || ny < 0 || nx >= width as i32 || ny >= height as i32 {
                    continue;
                }
                let ni = (ny as u32 * width + nx as u32) as usize;
                let weight = if biome_ids[ni] != biome_ids[i] {
                    1.0
                } else {
                    0.45
                };
                sum[0] += colors[ni][0] * weight;
                sum[1] += colors[ni][1] * weight;
                sum[2] += colors[ni][2] * weight;
                w += weight;
            }
            let blended = [sum[0] / w, sum[1] / w, sum[2] / w];
            out[i] = lerp3(colors[i], blended, strength);
        }
    }
    out
}

pub fn biome_rgb_advanced(h: f64, temp: f64, humid: f64, sea_level: f64) -> [f64; 3] {
    if h < sea_level {
        return BiomeId::Ocean.rgb01();
    }
    let (rgb, id) = climate_color(temp, humid);
    altitude_mix(rgb, id, h, temp, sea_level).0
}
