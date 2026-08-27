use rand::Rng;

fn min_f64(a: f64, b: f64) -> f64 {
    if a < b {
        a
    } else {
        b
    }
}

pub fn get(data: &[f64], width: u32, height: u32, x: u32, y: u32) -> f64 {
    if x >= width || y >= height {
        return 0.0;
    }
    data[(y * width + x) as usize]
}

pub fn set(data: &mut [f64], width: u32, height: u32, x: u32, y: u32, val: f64) {
    if x < width && y < height {
        data[(y * width + x) as usize] = val;
    }
}

pub fn erode_hydraulic(data: &mut [f64], width: u32, height: u32, cycles: u32) {
    let mut rng = rand::thread_rng();
    let w = width as i32;
    let h = height as i32;

    for _ in 0..cycles {
        let mut x = rng.gen_range(1..w - 1) as f64;
        let mut y = rng.gen_range(1..h - 1) as f64;
        let mut dir_x: f64 = 0.0;
        let mut dir_y: f64 = 0.0;
        let mut speed: f64 = 1.0;
        let mut water: f64 = 1.0;
        let mut sediment: f64 = 0.0;

        for _ in 0..30 {
            let ix = x as i32;
            let iy = y as i32;
            let curr_h = get(data, width, height, ix as u32, iy as u32);

            let grad_x = get(data, width, height, (ix - 1) as u32, iy as u32)
                - get(data, width, height, (ix + 1) as u32, iy as u32);
            let grad_y = get(data, width, height, ix as u32, (iy - 1) as u32)
                - get(data, width, height, ix as u32, (iy + 1) as u32);

            dir_x = dir_x * 0.05 - grad_x * 0.95;
            dir_y = dir_y * 0.05 - grad_y * 0.95;

            let len = (dir_x * dir_x + dir_y * dir_y).sqrt();
            if len != 0.0 {
                dir_x /= len;
                dir_y /= len;
            }

            x += dir_x;
            y += dir_y;

            if x < 1.0 || x >= (w - 1) as f64 || y < 1.0 || y >= (h - 1) as f64 {
                break;
            }

            let diff = get(data, width, height, x as i32 as u32, y as i32 as u32) - curr_h;
            let max_sediment = water * 4.0 * speed.min(1.0);

            if diff > 0.0 {
                let amount = sediment.min(diff);
                set(data, width, height, ix as u32, iy as u32, curr_h + amount);
                sediment -= amount;
            } else if sediment > max_sediment {
                let amount = (sediment - max_sediment) * 0.3;
                set(data, width, height, ix as u32, iy as u32, curr_h + amount);
                sediment -= amount;
            } else {
                let amount = min_f64((max_sediment - sediment) * 0.3, -diff);
                set(data, width, height, ix as u32, iy as u32, curr_h - amount);
                sediment += amount;
            }
            speed = (speed * speed + diff * 4.0).sqrt();
            water *= 0.98;
            if water < 0.01 {
                break;
            }
        }
    }
}

pub fn erode_thermal(data: &mut [f64], width: u32, height: u32, cycles: u32, angle_of_repose: f64) {
    let mut rng = rand::thread_rng();
    let w = width as i32;
    let h = height as i32;

    for _ in 0..cycles {
        let x = rng.gen_range(1..w - 1);
        let y = rng.gen_range(1..h - 1);

        let center_h = get(data, width, height, x as u32, y as u32);

        let neighbors = [
            (x - 1, y - 1),
            (x, y - 1),
            (x + 1, y - 1),
            (x - 1, y),
            (x + 1, y),
            (x - 1, y + 1),
            (x, y + 1),
            (x + 1, y + 1),
        ];

        for (nx, ny) in neighbors {
            if nx < 0 || nx >= w || ny < 0 || ny >= h {
                continue;
            }

            let neighbor_h = get(data, width, height, nx as u32, ny as u32);
            let height_diff = center_h - neighbor_h;

            if height_diff > angle_of_repose {
                let transfer = (height_diff - angle_of_repose) * 0.5;
                set(
                    data,
                    width,
                    height,
                    x as u32,
                    y as u32,
                    center_h - transfer,
                );
                set(
                    data,
                    width,
                    height,
                    nx as u32,
                    ny as u32,
                    neighbor_h + transfer,
                );
            }
        }
    }
}

pub fn apply_terracing(data: &mut [f64], levels: u32) {
    if levels == 0 {
        return;
    }
    let inv_levels = 1.0 / levels as f64;
    for h in data.iter_mut() {
        *h = (*h * levels as f64).floor() * inv_levels;
    }
}

pub fn simulate_fluid(
    elevation: &[f64],
    water_level: &mut [f64],
    width: u32,
    height: u32,
    iterations: u32,
) {
    for _ in 0..iterations {
        let w = width as i32;
        let h = height as i32;

        for y in 1..h - 1 {
            for x in 1..w - 1 {
                let idx = (y as u32 * width + x as u32) as usize;
                let center_h = elevation[idx];
                let center_water = water_level[idx];
                let center_total = center_h + center_water;

                let neighbors = [(x - 1, y), (x + 1, y), (x, y - 1), (x, y + 1)];
                let mut avg_level = center_total;
                let mut count = 1.0;

                for (nx, ny) in neighbors {
                    let nidx = (ny as u32 * width + nx as u32) as usize;
                    avg_level += elevation[nidx] + water_level[nidx];
                    count += 1.0;
                }

                avg_level /= count;
                let new_water = (avg_level - center_h).max(0.0);
                water_level[idx] = new_water * 0.95;
            }
        }
    }
}
