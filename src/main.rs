use image::{Frame, Rgba, RgbaImage, Delay};
use image::codecs::gif::GifEncoder;
use noise::{Fbm, MultiFractal, NoiseFn, Perlin, Seedable};
use std::fs::File;
use std::f64::consts::PI;

// --- CONFIGURATION ---
const WIDTH: u32 = 400;
const HEIGHT: u32 = 400;
const NOISE_SCALE: f64 = 150.0; 
const Z_SCALE: f64 = 70.0;      // Taller mountains
const SEED: u32 = 999;          
const TOTAL_FRAMES: u32 = 60;   
const SEA_LEVEL: f64 = 0.25;    // Everything below this is water

struct HeightMap {
    data: Vec<f64>,
    width: u32,
    height: u32,
}

impl HeightMap {
    fn new(w: u32, h: u32) -> Self {
        let fbm = Fbm::<Perlin>::new(SEED)
            .set_octaves(6)
            .set_lacunarity(2.0)
            .set_persistence(0.5);

        let mut data = Vec::with_capacity((w * h) as usize);

        for y in 0..h {
            for x in 0..w {
                let nx = x as f64 / NOISE_SCALE;
                let ny = y as f64 / NOISE_SCALE;
                
                let raw = fbm.get([nx, ny]);
                
                // Ridged Noise (Sharp peaks)
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
}

fn main() {
    println!("Generating Map with Flat Oceans...");
    let map = HeightMap::new(WIDTH, HEIGHT);

    println!("Rendering {} frames...", TOTAL_FRAMES);
    let file_out = File::create("./output/cycle.gif").unwrap();
    let mut encoder = GifEncoder::new(file_out);
    encoder.set_repeat(image::codecs::gif::Repeat::Infinite).unwrap();

    for f in 0..TOTAL_FRAMES {
        let progress = f as f64 / TOTAL_FRAMES as f64;
        
        let sun_azimuth = progress * 2.0 * PI; 
        // Sun movement: Sunrise -> Noon -> Sunset
        let sun_elevation = (progress * PI).sin().max(0.0) * 0.8 + 0.05;

        let frame_img = render_frame(&map, sun_azimuth, sun_elevation);
        
        // 25 FPS
        let frame = Frame::from_parts(frame_img, 0, 0, Delay::from_numer_denom_ms(40, 1));
        encoder.encode_frame(frame).unwrap();
        
        if f % 10 == 0 { println!("Rendered frame {}/{}", f, TOTAL_FRAMES); }
    }
    
    println!("Done! Check output/cycle.gif");
}

fn render_frame(map: &HeightMap, sun_angle: f64, sun_elevation: f64) -> RgbaImage {
    let mut img = RgbaImage::new(WIDTH, HEIGHT);
    
    let light_dir_x = -sun_angle.cos();
    let light_dir_y = -sun_angle.sin();
    
    let step_dist = 1.0; 
    let height_step = sun_elevation.tan() * step_dist; 

    // Dynamic Lighting Colors
    let is_night = sun_elevation < 0.1;
    let (light_r, light_g, light_b) = if is_night {
        (0.1, 0.1, 0.3) 
    } else if sun_elevation < 0.3 {
        (1.0, 0.5, 0.2) // Orange
    } else {
        (1.0, 1.0, 0.9) // White-ish
    };

    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let h_val = map.get(x, y);
            
            // If this pixel is water, it should be visually flat (constant height)
            // But we keep h_val for coloring (deep vs shallow)
            let effective_height = if h_val < SEA_LEVEL { SEA_LEVEL } else { h_val };
            let pixel_h_real = effective_height * Z_SCALE; 
            
            // --- RAYMARCHING ---
            let mut in_shadow = false;
            
            if !is_night {
                let mut ray_h = pixel_h_real;
                let mut ray_x = x as f64;
                let mut ray_y = y as f64;
                
                // Shadow Ray Trace
                for _ in 0..150 {
                    ray_x -= light_dir_x * step_dist;
                    ray_y -= light_dir_y * step_dist;
                    ray_h += height_step; 
                    
                    if ray_x < 0.0 || ray_x >= WIDTH as f64 || ray_y < 0.0 || ray_y >= HEIGHT as f64 { break; }
                    if ray_h > Z_SCALE { break; }
                    
                    let terrain_raw = map.get(ray_x as u32, ray_y as u32);
                    
                    // CRITICAL FIX: Water cannot block light
                    // If the terrain we hit is below sea level, ignore it (it's flat water)
                    if terrain_raw < SEA_LEVEL {
                        continue; 
                    }

                    // Otherwise, check if the land blocks the light
                    let terrain_h = terrain_raw * Z_SCALE;
                    if terrain_h > ray_h {
                        in_shadow = true;
                        break; 
                    }
                }
            }

            // --- COLORING ---
            let base_color = get_biome_color(h_val);
            
            // Ambient light
            let ambient = 0.3;
            let shadow_factor = if in_shadow { ambient } else { 1.0 };
            
            // Specular Reflection (Sun glint on water)
            // If it's water, and NOT in shadow, and the sun is aligned...
            let mut specular = 0.0;
            if h_val < SEA_LEVEL && !in_shadow && !is_night {
                 // Simple specular approximation based on sun height
                 // (Glints more when sun is high)
                 specular = (sun_elevation * 0.5).powi(2);
            }

            let r = (base_color[0] as f64 * light_r * shadow_factor + specular * 255.0).clamp(0.0, 255.0) as u8;
            let g = (base_color[1] as f64 * light_g * shadow_factor + specular * 255.0).clamp(0.0, 255.0) as u8;
            let b = (base_color[2] as f64 * light_b * shadow_factor + specular * 255.0).clamp(0.0, 255.0) as u8;

            img.put_pixel(x, y, Rgba([r, g, b, 255]));
        }
    }
    img
}

fn get_biome_color(h: f64) -> Rgba<u8> {
    if h < SEA_LEVEL - 0.1 { Rgba([10, 30, 80, 255]) }       // Deep Water
    else if h < SEA_LEVEL { Rgba([30, 80, 180, 255]) }       // Shallow Water
    else if h < 0.30 { Rgba([210, 200, 120, 255]) }          // Sand
    else if h < 0.55 { Rgba([50, 140, 50, 255]) }            // Grass
    else if h < 0.75 { Rgba([30, 90, 30, 255]) }             // Forest
    else if h < 0.85 { Rgba([100, 100, 100, 255]) }          // Rock
    else { Rgba([240, 240, 255, 255]) }                      // Snow
}