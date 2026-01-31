use clap::Parser;
use image::{Frame, Rgba, RgbaImage, Delay};
use image::codecs::gif::GifEncoder;
use noise::{Fbm, MultiFractal, NoiseFn, Perlin, Seedable};
use std::fs::File;
use std::f64::consts::PI;
use std::path::Path;

// --- CLI ARGUMENTS STRUCT ---
#[derive(Parser, Debug)]
#[command(author, version, about = "Generates ray-traced fantasy maps")]
struct Args {
    /// Width of the map
    #[arg(short = 'W', long, default_value_t = 400)]
    width: u32,

    /// Height of the map
    #[arg(short = 'H', long, default_value_t = 400)]
    height: u32,

    /// Output filename (e.g., map.png, cycle.gif)
    #[arg(short, long, default_value = "output.gif")]
    output: String,

    /// Seed for random generation
    #[arg(short, long, default_value_t = 999)]
    seed: u32,

    /// Scale of terrain features (Zoom level)
    #[arg(long, default_value_t = 150.0)]
    scale: f64,

    /// Height multiplier for mountains (Z-scale)
    #[arg(long, default_value_t = 70.0)]
    z_scale: f64,

    /// Number of frames (only for GIF)
    #[arg(long, default_value_t = 60)]
    frames: u32,
}

// Constants that don't need configuration
const SEA_LEVEL: f64 = 0.25;

struct HeightMap {
    data: Vec<f64>,
    width: u32,
    height: u32,
    scale: f64,
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
                
                // Ridged Noise
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
        HeightMap { data, width: w, height: h, scale }
    }

    fn get(&self, x: u32, y: u32) -> f64 {
        if x >= self.width || y >= self.height { return 0.0; }
        self.data[(y * self.width + x) as usize]
    }
}

fn main() {
    // 1. Parse Arguments
    let args = Args::parse();
    
    println!("Generating map: {}x{} | Seed: {}", args.width, args.height, args.seed);

    let map = HeightMap::new(args.width, args.height, args.seed, args.scale);
    let path = Path::new(&args.output);
    let extension = path.extension().and_then(|s| s.to_str()).unwrap_or("gif");

    // 2. Decide Mode based on Extension
    if extension.to_lowercase() == "gif" {
        render_gif(&map, &args);
    } else {
        render_static(&map, &args);
    }
}

fn render_static(map: &HeightMap, args: &Args) {
    println!("Rendering single static frame (Noon)...");
    
    // Noon settings: Azimuth 0, Elevation High (0.8)
    let img = render_frame(map, 0.0, 0.8, args.z_scale);
    
    img.save(&args.output).unwrap();
    println!("Saved static map to '{}'", args.output);
}

fn render_gif(map: &HeightMap, args: &Args) {
    println!("Rendering animated GIF ({} frames)...", args.frames);
    let file_out = File::create(&args.output).unwrap();
    let mut encoder = GifEncoder::new(file_out);
    encoder.set_repeat(image::codecs::gif::Repeat::Infinite).unwrap();

    for f in 0..args.frames {
        let progress = f as f64 / args.frames as f64;
        
        let sun_azimuth = progress * 2.0 * PI; 
        let sun_elevation = (progress * PI).sin().max(0.0) * 0.8 + 0.05;

        let frame_img = render_frame(map, sun_azimuth, sun_elevation, args.z_scale);
        
        let frame = Frame::from_parts(frame_img, 0, 0, Delay::from_numer_denom_ms(40, 1));
        encoder.encode_frame(frame).unwrap();
        
        if f % 10 == 0 { println!("Rendered frame {}/{}", f, args.frames); }
    }
    println!("Saved animation to '{}'", args.output);
}

fn render_frame(map: &HeightMap, sun_angle: f64, sun_elevation: f64, z_scale: f64) -> RgbaImage {
    let width = map.width;
    let height = map.height;
    let mut img = RgbaImage::new(width, height);
    
    let light_dir_x = -sun_angle.cos();
    let light_dir_y = -sun_angle.sin();
    let step_dist = 1.0; 
    let height_step = sun_elevation.tan() * step_dist; 

    // Light Color logic
    let is_night = sun_elevation < 0.1;
    let (light_r, light_g, light_b) = if is_night {
        (0.1, 0.1, 0.3) 
    } else if sun_elevation < 0.3 {
        (1.0, 0.5, 0.2) 
    } else {
        (1.0, 1.0, 0.9) 
    };

    for y in 0..height {
        for x in 0..width {
            let h_val = map.get(x, y);
            
            // Flat water physics
            let effective_height = if h_val < SEA_LEVEL { SEA_LEVEL } else { h_val };
            let pixel_h_real = effective_height * z_scale; 
            
            // Raymarching
            let mut in_shadow = false;
            
            if !is_night {
                let mut ray_h = pixel_h_real;
                let mut ray_x = x as f64;
                let mut ray_y = y as f64;
                
                for _ in 0..200 {
                    ray_x -= light_dir_x * step_dist;
                    ray_y -= light_dir_y * step_dist;
                    ray_h += height_step; 
                    
                    if ray_x < 0.0 || ray_x >= width as f64 || ray_y < 0.0 || ray_y >= height as f64 { break; }
                    if ray_h > z_scale { break; }
                    
                    let terrain_raw = map.get(ray_x as u32, ray_y as u32);
                    
                    if terrain_raw < SEA_LEVEL { continue; }

                    let terrain_h = terrain_raw * z_scale;
                    if terrain_h > ray_h {
                        in_shadow = true;
                        break; 
                    }
                }
            }

            // Coloring
            let base_color = get_biome_color(h_val);
            let ambient = 0.3;
            let shadow_factor = if in_shadow { ambient } else { 1.0 };
            
            let mut specular = 0.0;
            if h_val < SEA_LEVEL && !in_shadow && !is_night {
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
    if h < SEA_LEVEL - 0.1 { Rgba([10, 30, 80, 255]) }
    else if h < SEA_LEVEL { Rgba([30, 80, 180, 255]) }
    else if h < 0.30 { Rgba([210, 200, 120, 255]) }
    else if h < 0.55 { Rgba([50, 140, 50, 255]) }
    else if h < 0.75 { Rgba([30, 90, 30, 255]) }
    else if h < 0.85 { Rgba([100, 100, 100, 255]) }
    else { Rgba([240, 240, 255, 255]) }
}