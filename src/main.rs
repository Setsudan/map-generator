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
#[command(author, version, about = "Generates high-fidelity voxel fantasy maps")]
struct Args {
    #[arg(short = 'W', long, default_value_t = 400)]
    width: u32,

    #[arg(short = 'H', long, default_value_t = 400)]
    height: u32,

    #[arg(short, long, default_value = "output.gif")]
    output: String,

    #[arg(short, long, default_value_t = 42)]
    seed: u32,

    #[arg(long, default_value_t = 150.0)]
    scale: f64,

    #[arg(long, default_value_t = 60.0)]
    z_scale: f64,

    #[arg(long, default_value_t = 60)]
    frames: u32,

    #[arg(long, action)]
    isometric: bool,

    #[arg(long, default_value_t = 0)]
    erosion_cycles: u32,
}

const SEA_LEVEL: f64 = 0.25;

// --- GEOMETRY STRUCTS ---
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

                // Island Mask
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

    fn erode(&mut self, cycles: u32) {
        let mut rng = rand::thread_rng();
        let w = self.width as i32;
        let h = self.height as i32;

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
                let curr_h = self.get(ix as u32, iy as u32);
                
                let grad_x = self.get((ix - 1) as u32, iy as u32) - self.get((ix + 1) as u32, iy as u32);
                let grad_y = self.get(ix as u32, (iy - 1) as u32) - self.get(ix as u32, (iy + 1) as u32);

                dir_x = dir_x * 0.05 - grad_x * 0.95;
                dir_y = dir_y * 0.05 - grad_y * 0.95;
                
                let len = (dir_x * dir_x + dir_y * dir_y).sqrt();
                if len != 0.0 { dir_x /= len; dir_y /= len; }

                x += dir_x;
                y += dir_y;

                if x < 1.0 || x >= (w - 1) as f64 || y < 1.0 || y >= (h - 1) as f64 { break; }

                let diff = self.get(x as i32 as u32, y as i32 as u32) - curr_h;
                let max_sediment = water * 4.0 * speed.min(1.0);

                if diff > 0.0 {
                    let amount = sediment.min(diff);
                    self.set(ix as u32, iy as u32, curr_h + amount);
                    sediment -= amount;
                } else {
                    if sediment > max_sediment {
                        let amount = (sediment - max_sediment) * 0.3;
                        self.set(ix as u32, iy as u32, curr_h + amount);
                        sediment -= amount;
                    } else {
                        let amount = min_f64((max_sediment - sediment) * 0.3, -diff);
                        self.set(ix as u32, iy as u32, curr_h - amount);
                        sediment += amount;
                    }
                }
                speed = (speed * speed + diff * 4.0).sqrt();
                water *= 0.98;
                if water < 0.01 { break; }
            }
        }
    }
}

fn min_f64(a: f64, b: f64) -> f64 { if a < b { a } else { b } }

// --- MAIN PIPELINE ---

fn main() {
    let args = Args::parse();
    println!("Generating Voxel Map: {}x{} | Seed: {}", args.width, args.height, args.seed);

    let mut map = HeightMap::new(args.width, args.height, args.seed, args.scale);
    
    if args.erosion_cycles > 0 {
        println!("Running erosion simulation...");
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
    println!("Rendering high-fidelity static frame...");
    let img = render_frame(map, 0.0, 0.8, 0.0, args.z_scale, args.isometric);
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
        // Keep sun relatively high to avoid super long ugly shadows in isometric
        let sun_elevation = (progress * PI).sin().max(0.0) * 0.6 + 0.2;
        
        let img = render_frame(map, sun_azimuth, sun_elevation, progress, args.z_scale, args.isometric);
        
        let frame = Frame::from_parts(img, 0, 0, Delay::from_numer_denom_ms(50, 1));
        encoder.encode_frame(frame).unwrap();

        if f % 10 == 0 { println!("Frame {}/{}", f, args.frames); }
    }
}

// --- RENDERING CORE ---

struct RenderPixel {
    color: Rgba<u8>,
    height: f64,
    is_water: bool,
}

fn render_frame(map: &HeightMap, sun_angle: f64, sun_elevation: f64, time: f64, z_scale: f64, isometric: bool) -> RgbaImage {
    let width = map.width;
    let height = map.height;
    
    // Noise generators for texture detail
    let water_noise = Perlin::new(100);
    let terrain_noise = Perlin::new(200);

    // Light Setup
    let light_dir_x = -sun_angle.cos();
    let light_dir_y = -sun_angle.sin();
    let height_step = sun_elevation.tan();
    
    // Ambient Light Colors
    let sun_color = [1.2, 1.1, 1.0]; // Warm sunlight
    let ambient_sky = [0.2, 0.25, 0.4]; // Cool shadows
    let sun_intensity = sun_elevation.clamp(0.0, 1.0);

    // --- STEP 1: CALCULATE SURFACE DATA ---
    // We compute the exact height and color for every grid point first
    let mut grid: Vec<RenderPixel> = Vec::with_capacity((width * height) as usize);

    for y in 0..height {
        for x in 0..width {
            let h_base = map.get(x, y);
            
            // Texture Noise (Subtle variation on land)
            let tex_val = terrain_noise.get([x as f64 * 0.1, y as f64 * 0.1]) * 0.05;

            // Water Calculation
            let (final_h, is_water, mut base_rgb) = if h_base < SEA_LEVEL {
                // Animated Water
                let wave_phase = time * 4.0;
                let wave = water_noise.get([x as f64 * 0.1, y as f64 * 0.1, wave_phase]);
                let wave_h = wave * 0.02; // Small vertical displacement
                
                let depth = SEA_LEVEL - h_base;
                let deep_col = [0.1, 0.3, 0.6];
                let shallow_col = [0.2, 0.5, 0.8];
                let foam_col = [0.9, 0.95, 1.0];
                
                // Mix colors based on depth
                let t = (depth * 10.0).clamp(0.0, 1.0);
                let mut r = deep_col[0] * t + shallow_col[0] * (1.0 - t);
                let mut g = deep_col[1] * t + shallow_col[1] * (1.0 - t);
                let mut b = deep_col[2] * t + shallow_col[2] * (1.0 - t);

                // Foam at shoreline or wave crests
                let foam_mask = (wave - 0.5).max(0.0) + (1.0 - (depth/0.05).clamp(0.0, 1.0));
                if foam_mask > 0.5 {
                    r = r * 0.5 + foam_col[0] * 0.5;
                    g = g * 0.5 + foam_col[1] * 0.5;
                    b = b * 0.5 + foam_col[2] * 0.5;
                }

                (SEA_LEVEL + wave_h, true, [r, g, b])
            } else {
                // Land Biomes
                let h = h_base + tex_val; // Apply texture height noise
                let c = get_biome_rgb(h_base);
                // Apply subtle noise to color
                let noise_tint = 1.0 + (tex_val * 2.0); 
                (h, false, [c[0] * noise_tint, c[1] * noise_tint, c[2] * noise_tint])
            };

            // Convert to 0-255 later, keep as float 0-1 for lighting math
            grid.push(RenderPixel { 
                color: Rgba([(base_rgb[0] * 255.0) as u8, (base_rgb[1] * 255.0) as u8, (base_rgb[2] * 255.0) as u8, 255]),
                height: final_h,
                is_water
            });
        }
    }

    // Helper to get grid data safely
    let get_pix = |nx: i32, ny: i32| -> Option<&RenderPixel> {
        if nx >= 0 && nx < width as i32 && ny >= 0 && ny < height as i32 {
            Some(&grid[(ny * width as i32 + nx) as usize])
        } else {
            None
        }
    };

    // --- STEP 2: PROJECT & LIGHTING ---
    // We render directly to the output image using painter's algorithm
    // Background
    let sky_r = (30.0 * (1.0 - sun_elevation) + 135.0 * sun_elevation).clamp(0.0, 255.0) as u8;
    let sky_g = (30.0 * (1.0 - sun_elevation) + 206.0 * sun_elevation).clamp(0.0, 255.0) as u8;
    let sky_b = (60.0 * (1.0 - sun_elevation) + 235.0 * sun_elevation).clamp(0.0, 255.0) as u8;
    let sky_color = Rgba([sky_r, sky_g, sky_b, 255]);

    // Setup Isometric bounds
    let project = |x: i32, y: i32, z: f64| -> (i32, i32) {
        let iso_x = x - y;
        let iso_y = ((x + y) as f64 * 0.5 - z) as i32;
        (iso_x, iso_y)
    };
    
    // Auto-centering logic
    let mut min_x = i32::MAX; let mut max_x = i32::MIN;
    let mut min_y = i32::MAX; let mut max_y = i32::MIN;
    let corners = [(0,0), (width as i32, 0), (0, height as i32), (width as i32, height as i32)];
    for (cx, cy) in corners {
        for &cz in &[0.0, z_scale] {
             let (px, py) = project(cx, cy, cz);
             min_x = min(min_x, px); max_x = max(max_x, px);
             min_y = min(min_y, py); max_y = max(max_y, py);
        }
    }
    let pad = 20;
    let out_w = (max_x - min_x + pad * 2) as u32;
    let out_h = (max_y - min_y + pad * 2) as u32;
    let off_x = -min_x + pad;
    let off_y = -min_y + pad;

    let mut img = RgbaImage::from_pixel(out_w, out_h, sky_color);
    
    // Z-Buffer for correct depth sorting (simplistic, just y-order is mostly fine for iso, 
    // but strict painter's algo works better here)
    
    for y in 0..height {
        for x in 0..width {
            let px_data = &grid[(y * width + x) as usize];
            let z_real = px_data.height * z_scale;
            
            // --- LIGHTING CALCULATION ---
            
            // 1. Raycast Shadow
            let mut in_shadow = false;
            let mut ray_h = z_real;
            let mut ray_x = x as f64;
            let mut ray_y = y as f64;
            for _ in 0..40 { // Short range shadows are enough for voxel look
                ray_x -= light_dir_x;
                ray_y -= light_dir_y;
                ray_h += height_step;
                if let Some(obs) = get_pix(ray_x as i32, ray_y as i32) {
                    if obs.height * z_scale > ray_h { in_shadow = true; break; }
                } else { break; }
            }

            // 2. Ambient Occlusion (AO)
            // Check neighbors. If neighbor is higher, darken this pixel.
            let mut ao_darkening: f64 = 0.0;
            let neighbors = [(-1,0), (1,0), (0,-1), (0,1)];
            for (dx, dy) in neighbors {
                if let Some(n) = get_pix(x as i32 + dx, y as i32 + dy) {
                     if n.height > px_data.height {
                         ao_darkening += 0.15; // Accumulate shadow in corners
                     }
                }
            }
            ao_darkening = ao_darkening.min(0.6); // Cap AO

            // 3. Combine Light
            let shadow_mult = if in_shadow { 0.4 } else { 1.0 };
            let total_light = (sun_intensity * shadow_mult) * (1.0 - ao_darkening);
            
            // Apply light to base color
            let r = (px_data.color[0] as f64 * (total_light * sun_color[0] + ambient_sky[0] * 0.4)).clamp(0.0, 255.0) as u8;
            let g = (px_data.color[1] as f64 * (total_light * sun_color[1] + ambient_sky[1] * 0.4)).clamp(0.0, 255.0) as u8;
            let b = (px_data.color[2] as f64 * (total_light * sun_color[2] + ambient_sky[2] * 0.4)).clamp(0.0, 255.0) as u8;
            
            let final_col = Rgba([r, g, b, 255]);

            // --- DRAWING VOXEL ---
            let (iso_x, iso_y) = project(x as i32, y as i32, z_real);
            let draw_x = iso_x + off_x;
            let draw_y = iso_y + off_y;

            if draw_x >= 0 && draw_x < out_w as i32 && draw_y >= 0 && draw_y < out_h as i32 {
                img.put_pixel(draw_x as u32, draw_y as u32, final_col);
            }

            // Draw Walls (The dirt underneath)
            // We darken walls to fake directionality
            let wall_col = Rgba([(r as f64 * 0.7) as u8, (g as f64 * 0.7) as u8, (b as f64 * 0.7) as u8, 255]);
            
            let mut draw_wall = |nx: i32, ny: i32| {
                let neighbor_z = if let Some(n) = get_pix(nx, ny) { n.height * z_scale } else { 0.0 };
                if z_real > neighbor_z {
                     let (_, n_iso_y) = project(x as i32, y as i32, neighbor_z);
                     let wall_bot = n_iso_y + off_y;
                     for wy in (draw_y + 1)..=wall_bot {
                        if draw_x >= 0 && draw_x < out_w as i32 && wy >= 0 && wy < out_h as i32 {
                            img.put_pixel(draw_x as u32, wy as u32, wall_col);
                        }
                     }
                }
            };

            // Only draw walls for front-facing sides in painter's algorithm
            // (Standard loop order: Back-to-Front Y, Left-to-Right X)
            draw_wall(x as i32, y as i32 + 1); // Front Left
            draw_wall(x as i32 + 1, y as i32); // Front Right
        }
    }
    
    img
}

// Helper: Biome Colors (0.0 - 1.0 RGB)
fn get_biome_rgb(h: f64) -> [f64; 3] {
    if h < SEA_LEVEL { [0.0, 0.0, 0.0] } // Water handled separately
    else if h < 0.28 { [0.86, 0.80, 0.55] } // Sand (Beige)
    else if h < 0.45 { [0.3, 0.6, 0.2] }   // Grass (Green)
    else if h < 0.65 { [0.15, 0.45, 0.15] } // Forest (Dark Green)
    else if h < 0.80 { [0.5, 0.45, 0.4] }   // Rock (Brown/Grey)
    else { [0.95, 0.95, 1.0] }              // Snow (White)
}