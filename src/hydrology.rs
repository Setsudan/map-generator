use std::cmp::Ordering;
use std::collections::BinaryHeap;

#[derive(Clone, Debug)]
pub struct HydrologyResult {
    pub flow_direction: Vec<u8>,
    pub flow_accumulation: Vec<f64>,
    pub river_mask: Vec<bool>,
    pub lake_mask: Vec<bool>,
    /// Long downhill paths from cold high ground (drawn on terrain)
    pub meltwater_mask: Vec<bool>,
}

/// D8 neighbor offsets; flow_direction 0-7, 255 = sink/ocean/lake
const D8: [(i32, i32); 8] = [
    (1, 0),
    (1, 1),
    (0, 1),
    (-1, 1),
    (-1, 0),
    (-1, -1),
    (0, -1),
    (1, -1),
];

#[derive(Copy, Clone)]
struct FloodNode {
    elev: f64,
    idx: usize,
}

impl PartialEq for FloodNode {
    fn eq(&self, other: &Self) -> bool {
        self.idx == other.idx
    }
}
impl Eq for FloodNode {}
impl PartialOrd for FloodNode {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for FloodNode {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .elev
            .partial_cmp(&self.elev)
            .unwrap_or(Ordering::Equal)
            .then_with(|| self.idx.cmp(&other.idx))
    }
}

pub fn generate_hydrology(
    elevation: &mut [f64],
    width: u32,
    height: u32,
    sea_level: f64,
    river_threshold: f64,
    lake_threshold: f64,
    river_erosion: f64,
    temperature: Option<&[f64]>,
) -> HydrologyResult {
    let size = (width * height) as usize;
    let original = elevation.to_vec();
    let mut filled = elevation.to_vec();

    priority_flood(&mut filled, width, height, sea_level);

    let mut lake_mask = vec![false; size];
    for i in 0..size {
        let depth = filled[i] - original[i];
        if original[i] >= sea_level && depth > lake_threshold {
            lake_mask[i] = true;
        }
    }

    let flow_direction = compute_flow_direction(&filled, width, height, sea_level, &lake_mask);
    let mut flow_accumulation = compute_flow_accumulation(&flow_direction, width, height);

    // Meltwater: cold high ground with a smooth downhill path seeds rivers
    if let Some(temp) = temperature {
        seed_meltwater_rivers(
            &mut flow_accumulation,
            &flow_direction,
            &original,
            temp,
            width,
            height,
            sea_level,
            &lake_mask,
        );
    }

    let mut river_mask = vec![false; size];
    for i in 0..size {
        if original[i] >= sea_level && !lake_mask[i] && flow_accumulation[i] >= river_threshold {
            river_mask[i] = true;
        }
    }

    let mut meltwater_mask = vec![false; size];
    if let Some(temp) = temperature {
        mark_meltwater_paths(
            &mut meltwater_mask,
            &flow_direction,
            &original,
            temp,
            &flow_accumulation,
            width,
            height,
            sea_level,
            &lake_mask,
            river_threshold * 0.25,
        );
        // Meltwater rivers are also rivers
        for i in 0..size {
            if meltwater_mask[i] {
                river_mask[i] = true;
            }
        }
    }

    river_mask = prune_river_mask(&river_mask, &flow_accumulation, width, height, river_threshold);

    if river_erosion > 0.0 {
        for i in 0..size {
            if river_mask[i] {
                let deepen = river_erosion * (1.0 + flow_accumulation[i]).ln() * 0.01;
                elevation[i] = (elevation[i] - deepen).max(sea_level);
            }
        }
    }

    HydrologyResult {
        flow_direction,
        flow_accumulation,
        river_mask,
        lake_mask,
        meltwater_mask,
    }
}

/// Cold + high cells inject meltwater down smooth downhill paths (not cliffs).
fn seed_meltwater_rivers(
    accumulation: &mut [f64],
    flow_direction: &[u8],
    elevation: &[f64],
    temperature: &[f64],
    width: u32,
    height: u32,
    sea_level: f64,
    lake_mask: &[bool],
) {
    let boost = 180.0;
    for y in 1..height.saturating_sub(1) {
        for x in 1..width.saturating_sub(1) {
            let i = (y * width + x) as usize;
            if elevation[i] < sea_level || lake_mask[i] {
                continue;
            }
            // High + cold = snow/ice melt source
            if elevation[i] < 0.62 || temperature[i] > 0.38 {
                continue;
            }
            // Prefer a smooth descent: max neighbor drop in a moderate range
            let mut max_drop = 0.0f64;
            for (dx, dy) in D8 {
                let nx = x as i32 + dx;
                let ny = y as i32 + dy;
                if nx < 0 || ny < 0 || nx >= width as i32 || ny >= height as i32 {
                    continue;
                }
                let ni = (ny as u32 * width + nx as u32) as usize;
                let drop = elevation[i] - elevation[ni];
                if drop > 0.0 {
                    max_drop = max_drop.max(drop);
                }
            }
            // Smooth ride: some downhill, but not a cliff
            if max_drop < 0.01 || max_drop > 0.11 {
                continue;
            }

            // Trace downhill and add meltwater along the path
            let mut idx = i;
            let mut steps = 0;
            let melt = boost * (1.0 - temperature[i]) * ((elevation[i] - 0.55).max(0.0) * 2.5);
            let mut path_ok = false;
            while steps < 500 {
                accumulation[idx] += melt * (1.0 - steps as f64 / 500.0);
                let d = flow_direction[idx];
                if d == 255 {
                    break;
                }
                let x0 = (idx as u32) % width;
                let y0 = (idx as u32) / width;
                let (dx, dy) = D8[d as usize];
                let nx = x0 as i32 + dx;
                let ny = y0 as i32 + dy;
                if nx < 0 || ny < 0 || nx >= width as i32 || ny >= height as i32 {
                    break;
                }
                let nidx = (ny as u32 * width + nx as u32) as usize;
                if elevation[nidx] < sea_level || lake_mask[nidx] {
                    accumulation[nidx] += melt * 0.5;
                    path_ok = steps >= 12;
                    break;
                }
                // Stop if next step is a cliff (sudden drop)
                if elevation[idx] - elevation[nidx] > 0.14 {
                    break;
                }
                // Reached meaningfully lower / warmer lowlands
                if elevation[nidx] < 0.45 && steps >= 12 {
                    path_ok = true;
                }
                idx = nidx;
                steps += 1;
            }
            let _ = path_ok;
        }
    }
}

fn mark_meltwater_paths(
    river_mask: &mut [bool],
    flow_direction: &[u8],
    elevation: &[f64],
    temperature: &[f64],
    accumulation: &[f64],
    width: u32,
    height: u32,
    sea_level: f64,
    lake_mask: &[bool],
    min_acc: f64,
) {
    for y in 1..height.saturating_sub(1) {
        for x in 1..width.saturating_sub(1) {
            let i = (y * width + x) as usize;
            if elevation[i] < 0.62 || temperature[i] > 0.38 {
                continue;
            }
            if elevation[i] < sea_level || lake_mask[i] {
                continue;
            }

            let mut idx = i;
            let mut steps = 0;
            let mut path: Vec<usize> = Vec::new();
            let start_h = elevation[i];
            while steps < 450 {
                path.push(idx);
                let d = flow_direction[idx];
                if d == 255 {
                    break;
                }
                let x0 = (idx as u32) % width;
                let y0 = (idx as u32) / width;
                let (dx, dy) = D8[d as usize];
                let nx = x0 as i32 + dx;
                let ny = y0 as i32 + dy;
                if nx < 0 || ny < 0 || nx >= width as i32 || ny >= height as i32 {
                    break;
                }
                let nidx = (ny as u32 * width + nx as u32) as usize;
                if elevation[nidx] < sea_level || lake_mask[nidx] {
                    break;
                }
                if elevation[idx] - elevation[nidx] > 0.14 {
                    break; // cliff — waterfall, stop river mark
                }
                idx = nidx;
                steps += 1;
            }

            // Need a real downhill journey from high cold to lower land
            if path.len() < 16 {
                continue;
            }
            let end_h = elevation[*path.last().unwrap()];
            if start_h - end_h < 0.08 {
                continue;
            }
            let peak_acc = path.iter().map(|&p| accumulation[p]).fold(0.0f64, f64::max);
            if peak_acc < min_acc {
                continue;
            }
            for &p in &path {
                if elevation[p] >= sea_level && !lake_mask[p] {
                    river_mask[p] = true;
                }
            }
        }
    }
}

fn prune_river_mask(
    mask: &[bool],
    accumulation: &[f64],
    width: u32,
    height: u32,
    threshold: f64,
) -> Vec<bool> {
    let mut out = mask.to_vec();
    let strong = threshold * 2.5;
    for y in 1..height.saturating_sub(1) {
        for x in 1..width.saturating_sub(1) {
            let i = (y * width + x) as usize;
            if !mask[i] {
                continue;
            }
            // Keep major channels always
            if accumulation[i] >= strong {
                continue;
            }
            let mut neighbors = 0;
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
                let nx = (x as i32 + dx) as u32;
                let ny = (y as i32 + dy) as u32;
                let ni = (ny * width + nx) as usize;
                if mask[ni] {
                    neighbors += 1;
                }
            }
            // Isolated or nearly-isolated cells become noise scratches
            if neighbors < 2 {
                out[i] = false;
            }
        }
    }
    out
}

fn priority_flood(filled: &mut [f64], width: u32, height: u32, sea_level: f64) {
    let size = filled.len();
    let mut closed = vec![false; size];
    let mut heap = BinaryHeap::new();

    for y in 0..height {
        for x in 0..width {
            let idx = (y * width + x) as usize;
            let on_border = x == 0 || y == 0 || x == width - 1 || y == height - 1;
            if filled[idx] < sea_level || on_border {
                if filled[idx] < sea_level {
                    filled[idx] = sea_level - 0.001;
                }
                heap.push(FloodNode {
                    elev: filled[idx],
                    idx,
                });
                closed[idx] = true;
            }
        }
    }

    while let Some(node) = heap.pop() {
        let y = (node.idx as u32) / width;
        let x = (node.idx as u32) % width;
        for (dx, dy) in D8 {
            let nx = x as i32 + dx;
            let ny = y as i32 + dy;
            if nx < 0 || ny < 0 || nx >= width as i32 || ny >= height as i32 {
                continue;
            }
            let nidx = (ny as u32 * width + nx as u32) as usize;
            if closed[nidx] {
                continue;
            }
            closed[nidx] = true;
            if filled[nidx] < node.elev {
                filled[nidx] = node.elev;
            }
            heap.push(FloodNode {
                elev: filled[nidx],
                idx: nidx,
            });
        }
    }
}

fn compute_flow_direction(
    filled: &[f64],
    width: u32,
    height: u32,
    sea_level: f64,
    lake_mask: &[bool],
) -> Vec<u8> {
    let size = filled.len();
    let mut dirs = vec![255u8; size];

    for y in 0..height {
        for x in 0..width {
            let idx = (y * width + x) as usize;
            if filled[idx] < sea_level || lake_mask[idx] {
                dirs[idx] = 255;
                continue;
            }

            let mut best_dir = 255u8;
            let mut best_drop = 0.0f64;
            let h0 = filled[idx];

            for (d, (dx, dy)) in D8.iter().enumerate() {
                let nx = x as i32 + dx;
                let ny = y as i32 + dy;
                if nx < 0 || ny < 0 || nx >= width as i32 || ny >= height as i32 {
                    continue;
                }
                let nidx = (ny as u32 * width + nx as u32) as usize;
                let drop = h0 - filled[nidx];
                let dist = if dx.abs() + dy.abs() == 2 {
                    std::f64::consts::SQRT_2
                } else {
                    1.0
                };
                let slope = drop / dist;
                if slope > best_drop {
                    best_drop = slope;
                    best_dir = d as u8;
                }
            }
            dirs[idx] = best_dir;
        }
    }
    dirs
}

fn compute_flow_accumulation(flow_direction: &[u8], width: u32, height: u32) -> Vec<f64> {
    let size = flow_direction.len();
    let mut acc = vec![1.0f64; size];
    let mut indegree = vec![0u32; size];

    for y in 0..height {
        for x in 0..width {
            let idx = (y * width + x) as usize;
            let d = flow_direction[idx];
            if d == 255 {
                continue;
            }
            let (dx, dy) = D8[d as usize];
            let nx = x as i32 + dx;
            let ny = y as i32 + dy;
            if nx < 0 || ny < 0 || nx >= width as i32 || ny >= height as i32 {
                continue;
            }
            let nidx = (ny as u32 * width + nx as u32) as usize;
            indegree[nidx] += 1;
        }
    }

    let mut queue: Vec<usize> = Vec::new();
    for i in 0..size {
        if indegree[i] == 0 {
            queue.push(i);
        }
    }

    let mut head = 0;
    while head < queue.len() {
        let idx = queue[head];
        head += 1;

        let d = flow_direction[idx];
        if d == 255 {
            continue;
        }
        let x = (idx as u32) % width;
        let y = (idx as u32) / width;
        let (dx, dy) = D8[d as usize];
        let nx = x as i32 + dx;
        let ny = y as i32 + dy;
        if nx < 0 || ny < 0 || nx >= width as i32 || ny >= height as i32 {
            continue;
        }
        let nidx = (ny as u32 * width + nx as u32) as usize;
        acc[nidx] += acc[idx];
        indegree[nidx] = indegree[nidx].saturating_sub(1);
        if indegree[nidx] == 0 {
            queue.push(nidx);
        }
    }

    acc
}
