use clap::Parser;
use image::{Frame, Rgba, RgbaImage, Delay};
use image::codecs::gif::GifEncoder;
use noise::{Fbm, MultiFractal, NoiseFn, Perlin, Seedable};
use std::fs::File;
use std::f64::consts::PI;
use std::path::Path;
use std::cmp::{min, max};
use rand::Rng; 

// --- CLI ARGUMENTS ---
#[derive(Parser, Debug)]
#[command(author, version, about = "Generates ray-traced fantasy maps")]
struct Args {
    #[arg(short = 'W', long, default_value_t = 400)]
    width: u32,

    #[arg(short = 'H', long, default_value_t = 400)]
    height: u32,

    #[arg(short, long, default_value = "output.gif")]
    output: String,

    #[arg(short, long, default_value_t = 999)]
    seed: u32,

    #[arg(long, default_value_t = 150.0)]
    scale: f64,

    #[arg(long, default_value_t = 70.0)]
    z_scale: f64,

    #[arg(long, default_value_t = 60)]
    frames: u32,

    #[arg(long, action)]
    isometric: bool,

    #[arg(long, default_value_t = 0)]
    erosion_cycles: u32,
}

const SEA_LEVEL: f64 = 0.25;

struct HeightMap {
    data: Vec<f64>,
    width: u32,
    height: u32,
}

impl HeightMap {
    fn new(w: u32, h: u32, seed: u32, scale: f64) -> Self {
        let fbm = Fbm::<Perlin>::new(seed)
            .set_octaves(6)
            .set_lacunarity(2.0)
            .set_persistence(0.5);

        let mut data = Vec::with_capacity((w * h) as usize);

        for y in 0..h {
            for x in 0..w {
                let nx = x as f64 / scale;
                let ny = y as f64 / scale;
                let raw = fbm.get([nx, ny]);
                
                let mut height = 1.0 - raw.abs(); 
                height = height.powi(2); 

                let cx = w as f64 / 2.0;
                let cy = h as f64 / 2.0;
                let dist = ((x as f64 - cx).powi(2) + (y as f64 - cy).powi(2)).sqrt();
                let max_dist = cx.min(cy);
                let mask = (1.0 - (dist / max_dist).powi(2)).clamp(0.0, 1.0);
                
                height *= mask;
                data.push(height.clamp(0.0, 1.0));
            }
        }
        HeightMap { data, width: w, height: h }
    }

    fn get(&self, x: u32, y: u32) -> f64 {
        if x >= self.width || y >= self.height { return 0.0; }
        self.data[(y * self.width + x) as usize]
    }
    
    fn set(&mut self, x: u32, y: u32, val: f64) {
        if x < self.width && y < self.height {
            self.data[(y * self.width + x) as usize] = val;
        }
    }

    // --- HYDRAULIC EROSION ALGORITHM ---
    fn erode(&mut self, cycles: u32) {
        let mut rng = rand::thread_rng();
        let w = self.width as i32;
        let h = self.height as i32;

        for _ in 0..cycles {
            let mut x = rng.gen_range(1..w - 1) as f64;
            let mut y = rng.gen_range(1..h - 1) as f64;
            
            // Explicit f64 types to fix compiler errors
            let mut dir_x: f64 = 0.0;
            let mut dir_y: f64 = 0.0;
            let mut speed: f64 = 1.0;
            let mut water: f64 = 1.0;
            let mut sediment: f64 = 0.0;

            let inertia = 0.05;
            let capacity = 4.0;
            let deposition = 0.3;
            let erosion = 0.3;
            let evaporation = 0.02;
            let gravity = 4.0;

            for _ in 0..30 {
                let ix = x as i32;
                let iy = y as i32;
                
                let curr_h = self.get(ix as u32, iy as u32);
                let h_xp = self.get((ix + 1) as u32, iy as u32);
                let h_xm = self.get((ix - 1) as u32, iy as u32);
                let h_yp = self.get(ix as u32, (iy + 1) as u32);
                let h_ym = self.get(ix as u32, (iy - 1) as u32);

                let grad_x = h_xm - h_xp;
                let grad_y = h_ym - h_yp;

                // Fixed: Removed unnecessary parentheses
                dir_x = dir_x * inertia - grad_x * (1.0 - inertia);
                dir_y = dir_y * inertia - grad_y * (1.0 - inertia);
                
                let len = (dir_x * dir_x + dir_y * dir_y).sqrt();
                if len != 0.0 {
                    dir_x /= len;
                    dir_y /= len;
                }

                x += dir_x;
                y += dir_y;

                if x < 1.0 || x >= (w - 1) as f64 || y < 1.0 || y >= (h - 1) as f64 { break; }

                let new_ix = x as i32;
                let new_iy = y as i32;
                let new_h = self.get(new_ix as u32, new_iy as u32);
                let diff = new_h - curr_h;

                let max_sediment = water * capacity * speed.min(1.0);

                if diff > 0.0 {
                    let amount = sediment.min(diff);
                    self.set(ix as u32, iy as u32, curr_h + amount);
                    sediment -= amount;
                } else {
                    if sediment > max_sediment {
                        let amount = (sediment - max_sediment) * deposition;
                        self.set(ix as u32, iy as u32, curr_h + amount);
                        sediment -= amount;
                    } else {
                        let amount = min_f64((max_sediment - sediment) * erosion, -diff);
                        self.set(ix as u32, iy as u32, curr_h - amount);
                        sediment += amount;
                    }
                }

                speed = (speed * speed + diff * gravity).sqrt();
                // Fixed: Removed unnecessary parentheses
                water *= 1.0 - evaporation;
                
                if water < 0.01 { break; }
            }
        }
    }
}

// Helper for min float since f64 isn't Ord
fn min_f64(a: f64, b: f64) -> f64 { if a < b { a } else { b } }

fn main() {
    let args = Args::parse();
    println!("Generating map: {}x{} | Seed: {}", args.width, args.height, args.seed);

    let mut map = HeightMap::new(args.width, args.height, args.seed, args.scale);
    
    if args.erosion_cycles > 0 {
        println!("Simulating {} erosion cycles... (This might take a moment)", args.erosion_cycles);
        map.erode(args.erosion_cycles);
    }

    let path = Path::new(&args.output);
    let extension = path.extension().and_then(|s| s.to_str()).unwrap_or("gif");

    if extension.to_lowercase() == "gif" {
        render_gif(&map, &args);
    } else {
        render_static(&map, &args);
    }
}

fn render_static(map: &HeightMap, args: &Args) {
    println!("Rendering single static frame...");
    let img = render_frame(map, 0.0, 0.8, args.z_scale, args.isometric);
    img.save(&args.output).unwrap();
    println!("Saved to '{}'", args.output);
}

fn render_gif(map: &HeightMap, args: &Args) {
    println!("Rendering GIF ({} frames)...", args.frames);
    let file_out = File::create(&args.output).unwrap();
    let mut encoder = GifEncoder::new(file_out);
    encoder.set_repeat(image::codecs::gif::Repeat::Infinite).unwrap();

    for f in 0..args.frames {
        let progress = f as f64 / args.frames as f64;
        let sun_azimuth = progress * 2.0 * PI; 
        let sun_elevation = (progress * PI).sin().max(0.0) * 0.8 + 0.05;

        let frame_img = render_frame(map, sun_azimuth, sun_elevation, args.z_scale, args.isometric);
        let frame = Frame::from_parts(frame_img, 0, 0, Delay::from_numer_denom_ms(40, 1));
        encoder.encode_frame(frame).unwrap();

        if f % 10 == 0 { println!("Rendered frame {}/{}", f, args.frames); }
    }
}

fn render_frame(map: &HeightMap, sun_angle: f64, sun_elevation: f64, z_scale: f64, isometric: bool) -> RgbaImage {
    let width = map.width;
    let height = map.height;

    // 1. SHADOW MAP PASS
    let mut color_grid: Vec<Rgba<u8>> = vec![Rgba([0,0,0,0]); (width * height) as usize];
    let light_dir_x = -sun_angle.cos();
    let light_dir_y = -sun_angle.sin();
    let height_step = sun_elevation.tan(); 
    let is_night = sun_elevation < 0.1;
    let (light_r, light_g, light_b) = if is_night { (0.1, 0.1, 0.3) } else if sun_elevation < 0.3 { (1.0, 0.5, 0.2) } else { (1.0, 1.0, 0.9) };

    for y in 0..height {
        for x in 0..width {
            let h_val = map.get(x, y);
            let effective_height = if h_val < SEA_LEVEL { SEA_LEVEL } else { h_val };
            let pixel_h_real = effective_height * z_scale; 
            
            let mut in_shadow = false;
            if !is_night {
                let mut ray_h = pixel_h_real;
                let mut ray_x = x as f64;
                let mut ray_y = y as f64;
                for _ in 0..150 {
                    ray_x -= light_dir_x;
                    ray_y -= light_dir_y;
                    ray_h += height_step; 
                    if ray_x < 0.0 || ray_x >= width as f64 || ray_y < 0.0 || ray_y >= height as f64 || ray_h > z_scale { break; }
                    let tr = map.get(ray_x as u32, ray_y as u32);
                    if tr < SEA_LEVEL { continue; }
                    if tr * z_scale > ray_h { in_shadow = true; break; }
                }
            }

            let base = get_biome_color(h_val);
            let shadow_fac = if in_shadow { 0.3 } else { 1.0 };
            let mut spec = 0.0;
            if h_val < SEA_LEVEL && !in_shadow && !is_night { spec = (sun_elevation * 0.5).powi(2); }

            let r = (base[0] as f64 * light_r * shadow_fac + spec * 255.0).clamp(0.0, 255.0) as u8;
            let g = (base[1] as f64 * light_g * shadow_fac + spec * 255.0).clamp(0.0, 255.0) as u8;
            let b = (base[2] as f64 * light_b * shadow_fac + spec * 255.0).clamp(0.0, 255.0) as u8;

            color_grid[(y * width + x) as usize] = Rgba([r, g, b, 255]);
        }
    }

    if !isometric {
        let mut img = RgbaImage::new(width, height);
        for y in 0..height {
            for x in 0..width {
                img.put_pixel(x, y, color_grid[(y * width + x) as usize]);
            }
        }
        return img;
    }

    // --- 2. ISOMETRIC RENDER WITH AUTO-SIZING & WALLS ---
    let mut min_iso_x = i32::MAX;
    let mut max_iso_x = i32::MIN;
    let mut min_iso_y = i32::MAX;
    let mut max_iso_y = i32::MIN;

    let project = |x: i32, y: i32, z: f64| -> (i32, i32) {
        let iso_x = x - y;
        let iso_y = ((x + y) as f64 * 0.5 - z) as i32;
        (iso_x, iso_y)
    };

    let corners = [(0,0), (width as i32, 0), (0, height as i32), (width as i32, height as i32)];
    for (cx, cy) in corners {
        for &cz in &[0.0, z_scale] {
             let (px, py) = project(cx, cy, cz);
             min_iso_x = min(min_iso_x, px);
             max_iso_x = max(max_iso_x, px);
             min_iso_y = min(min_iso_y, py);
             max_iso_y = max(max_iso_y, py);
        }
    }

    let padding = 20;
    let final_w = (max_iso_x - min_iso_x + padding * 2) as u32;
    let final_h = (max_iso_y - min_iso_y + padding * 2) as u32;
    let offset_x = -min_iso_x + padding;
    let offset_y = -min_iso_y + padding;

    let mut img = RgbaImage::new(final_w, final_h);
    
    for y in 0..height {
        for x in 0..width {
            let color = color_grid[(y * width + x) as usize];
            let h_val = map.get(x, y);
            let effective_height = if h_val < SEA_LEVEL { SEA_LEVEL } else { h_val };
            let z = effective_height * z_scale;

            let (px, py) = project(x as i32, y as i32, z);
            let final_x = px + offset_x;
            let final_y = py + offset_y;

            if final_x >= 0 && final_x < final_w as i32 && final_y >= 0 && final_y < final_h as i32 {
                img.put_pixel(final_x as u32, final_y as u32, color);
            }

            let mut draw_wall = |nx: u32, ny: u32, side_color: Rgba<u8>| {
                if nx < width && ny < height {
                    let n_h = map.get(nx, ny);
                    let n_eff_h = if n_h < SEA_LEVEL { SEA_LEVEL } else { n_h };
                    let n_z = n_eff_h * z_scale;

                    if z > n_z {
                        let (_, n_py) = project(x as i32, y as i32, n_z);
                        let bottom_y = n_py + offset_y;
                        for wy in (final_y + 1)..=bottom_y {
                            if final_x >= 0 && final_x < final_w as i32 && wy >= 0 && wy < final_h as i32 {
                                img.put_pixel(final_x as u32, wy as u32, side_color);
                            }
                        }
                    }
                } else {
                     let (_, n_py) = project(x as i32, y as i32, 0.0);
                     let bottom_y = n_py + offset_y;
                     for wy in (final_y + 1)..=bottom_y {
                        if final_x >= 0 && final_x < final_w as i32 && wy >= 0 && wy < final_h as i32 {
                            img.put_pixel(final_x as u32, wy as u32, side_color);
                        }
                    }
                }
            };

            let dark_color = Rgba([
                (color[0] as f64 * 0.7) as u8,
                (color[1] as f64 * 0.7) as u8,
                (color[2] as f64 * 0.7) as u8,
                255
            ]);
            let darker_color = Rgba([
                (color[0] as f64 * 0.5) as u8,
                (color[1] as f64 * 0.5) as u8,
                (color[2] as f64 * 0.5) as u8,
                255
            ]);

            draw_wall(x, y + 1, dark_color);
            draw_wall(x + 1, y, darker_color);
        }
    }

    img
}

fn get_biome_color(h: f64) -> Rgba<u8> {
    if h < SEA_LEVEL - 0.1 { Rgba([10, 30, 80, 255]) }
    else if h < SEA_LEVEL { Rgba([30, 80, 180, 255]) }
    else if h < 0.30 { Rgba([210, 200, 120, 255]) }
    else if h < 0.55 { Rgba([50, 140, 50, 255]) }
    else if h < 0.75 { Rgba([30, 90, 30, 255]) }
    else if h < 0.85 { Rgba([100, 100, 100, 255]) }
    else { Rgba([240, 240, 255, 255]) }
}