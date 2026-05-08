use clap::Parser;
use image::{Frame, Rgba, RgbaImage, Delay};
use image::codecs::gif::GifEncoder;
use noise::{Fbm, MultiFractal, NoiseFn, Perlin};
use std::fs::File;
use std::f64::consts::PI;
use std::path::Path;
use std::cmp::{min, max};
use rand::{Rng, SeedableRng};
use rand::rngs::StdRng;

// --- CLI ARGUMENTS ---
#[derive(Parser, Debug)]
#[command(author, version, about = "Generates high-fidelity voxel fantasy maps with advanced terrain")]
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

    #[arg(long, default_value_t = 100)]
    thermal_erosion_cycles: u32,

    #[arg(long, default_value_t = 0)]
    terracing_levels: u32,

    #[arg(long, action)]
    domain_warp: bool,

    #[arg(long, action)]
    use_voronoi: bool,

    #[arg(long, action)]
    fluid_dynamics: bool,

    #[arg(long, action)]
    dither: bool,

    #[arg(long, default_value_t = 1.0)]
    dof_strength: f64,
}

const SEA_LEVEL: f64 = 0.25;

// --- VORONOI BIOME STRUCTURES ---
#[derive(Clone, Debug)]
struct VoronoiBiome {
    center_x: f64,
    center_y: f64,
    biome_type: BiomeType,
    noise_seed: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub enum BiomeType {
    Desert,
    Grassland,
    Forest,
    Mountain,
    Tundra,
    Jungle,
    Badlands,
}

impl BiomeType {
    fn noise_params(&self) -> (f64, f64, f64) {
        // (octaves_bonus, lacunarity_mod, roughness)
        match self {
            BiomeType::Desert => (0.0, 1.8, 0.3),
            BiomeType::Grassland => (1.0, 2.0, 0.5),
            BiomeType::Forest => (2.0, 2.2, 0.6),
            BiomeType::Mountain => (3.0, 2.5, 0.7),
            BiomeType::Tundra => (0.5, 1.9, 0.4),
            BiomeType::Jungle => (2.5, 2.3, 0.65),
            BiomeType::Badlands => (1.5, 2.1, 0.55),
        }
    }
}

// --- GEOMETRY STRUCTS ---
struct HeightMap {
    data: Vec<f64>,
    temperature: Vec<f64>,
    humidity: Vec<f64>,
    water_level: Vec<f64>,
    width: u32,
    height: u32,
    voronoi_biomes: Vec<VoronoiBiome>,
}

impl HeightMap {
    fn new(w: u32, h: u32, seed: u32, scale: f64, use_voronoi: bool, domain_warp: bool) -> Self {
        let fbm = Fbm::<Perlin>::new(seed)
            .set_octaves(6)
            .set_lacunarity(2.0)
            .set_persistence(0.5);

        // Domain warping layer
        let fbm_warp = if domain_warp {
            Some(Fbm::<Perlin>::new(seed.wrapping_add(1000))
                .set_octaves(4)
                .set_lacunarity(2.1)
                .set_persistence(0.4))
        } else {
            None
        };

        // Temperature noise
        let temp_noise = Perlin::new(seed.wrapping_add(2000));
        
        // Humidity noise
        let humidity_noise = Perlin::new(seed.wrapping_add(3000));

        // Generate Voronoi biome centers
        let voronoi_biomes = if use_voronoi {
            Self::generate_voronoi_biomes(w, h, seed)
        } else {
            Vec::new()
        };

        let mut data = Vec::with_capacity((w * h) as usize);
        let mut temperature = Vec::with_capacity((w * h) as usize);
        let mut humidity = Vec::with_capacity((w * h) as usize);

        for y in 0..h {
            for x in 0..w {
                let nx = x as f64 / scale;
                let ny = y as f64 / scale;

                // Apply domain warping if enabled
                let (sample_x, sample_y) = if let Some(ref warp) = fbm_warp {
                    let warp_amount = 25.0;
                    let warp_x = warp.get([nx * 0.5, ny * 0.5]) * warp_amount;
                    let warp_y = warp.get([nx * 0.5 + 0.5, ny * 0.5 + 0.5]) * warp_amount;
                    (nx + warp_x, ny + warp_y)
                } else {
                    (nx, ny)
                };

                // Get biome-specific noise parameters if using Voronoi
                let raw = if use_voronoi && !voronoi_biomes.is_empty() {
                    let nearest_biome = Self::find_nearest_biome(x as f64, y as f64, &voronoi_biomes);
                    let (octave_bonus, lacunarity_mod, _) = nearest_biome.biome_type.noise_params();
                    
                    let biome_fbm = Fbm::<Perlin>::new(nearest_biome.noise_seed)
                        .set_octaves((6.0_f64 + octave_bonus).max(2.0) as usize)
                        .set_lacunarity(2.0 * lacunarity_mod)
                        .set_persistence(0.5);
                    
                    biome_fbm.get([sample_x, sample_y])
                } else {
                    fbm.get([sample_x, sample_y])
                };

                let mut height = 1.0 - raw.abs();
                height = height.powi(2);

                // Island mask
                let cx = w as f64 / 2.0;
                let cy = h as f64 / 2.0;
                let dist = ((x as f64 - cx).powi(2) + (y as f64 - cy).powi(2)).sqrt();
                let max_dist = cx.min(cy);
                let mask = (1.0 - (dist / max_dist).powi(2)).clamp(0.0, 1.0);
                
                height *= mask;

                // Temperature map (latitude-based with noise)
                let latitude_temp = 1.0 - (y as f64 / h as f64);
                let temp_noise_val = temp_noise.get([nx * 0.3, ny * 0.3]) * 0.3;
                let temp = (latitude_temp * 0.7 + 0.5 + temp_noise_val).clamp(0.0, 1.0);

                // Humidity map (noise-based)
                let humid_noise_val = humidity_noise.get([nx * 0.2, ny * 0.2]);
                let humid = (humid_noise_val * 0.5 + 0.5).clamp(0.0, 1.0);

                data.push(height.clamp(0.0, 1.0));
                temperature.push(temp);
                humidity.push(humid);
            }
        }

        HeightMap { 
            data, 
            temperature, 
            humidity, 
            water_level: vec![0.0; (w * h) as usize],
            width: w, 
            height: h,
            voronoi_biomes,
        }
    }

    fn generate_voronoi_biomes(w: u32, h: u32, seed: u32) -> Vec<VoronoiBiome> {
        let mut rng = StdRng::seed_from_u64(seed as u64);
        
        let cell_size = ((w as f64).max(h as f64) / 6.0) as u32;
        let mut biomes = Vec::new();

        for cell_x in 0..(w / cell_size + 1) {
            for cell_y in 0..(h / cell_size + 1) {
                let jitter_x = rng.gen_range(-(cell_size as i32 / 2)..cell_size as i32 / 2) as f64;
                let jitter_y = rng.gen_range(-(cell_size as i32 / 2)..cell_size as i32 / 2) as f64;

                let center_x = (cell_x as f64 * cell_size as f64) + jitter_x;
                let center_y = (cell_y as f64 * cell_size as f64) + jitter_y;

                let biome_type = [
                    BiomeType::Desert, BiomeType::Grassland, BiomeType::Forest,
                    BiomeType::Mountain, BiomeType::Tundra, BiomeType::Jungle, BiomeType::Badlands,
                ][rng.gen_range(0..7)].clone();

                biomes.push(VoronoiBiome {
                    center_x,
                    center_y,
                    biome_type,
                    noise_seed: seed.wrapping_add(rng.r#gen::<u32>()),
                });
            }
        }
        biomes
    }

    fn find_nearest_biome(x: f64, y: f64, biomes: &[VoronoiBiome]) -> &VoronoiBiome {
        biomes
            .iter()
            .min_by(|a, b| {
                let dist_a = (a.center_x - x).powi(2) + (a.center_y - y).powi(2);
                let dist_b = (b.center_x - x).powi(2) + (b.center_y - y).powi(2);
                dist_a.partial_cmp(&dist_b).unwrap_or(std::cmp::Ordering::Equal)
            })
            .unwrap_or(&biomes[0])
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

    fn get_temp(&self, x: u32, y: u32) -> f64 {
        if x >= self.width || y >= self.height { return 0.5; }
        self.temperature[(y * self.width + x) as usize]
    }

    fn get_humidity(&self, x: u32, y: u32) -> f64 {
        if x >= self.width || y >= self.height { return 0.5; }
        self.humidity[(y * self.width + x) as usize]
    }

    fn get_water_level(&self, x: u32, y: u32) -> f64 {
        if x >= self.width || y >= self.height { return 0.0; }
        self.water_level[(y * self.width + x) as usize]
    }

    fn set_water_level(&mut self, x: u32, y: u32, val: f64) {
        if x < self.width && y < self.height {
            self.water_level[(y * self.width + x) as usize] = val;
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

    // THERMAL EROSION: Talus generation
    fn erode_thermal(&mut self, cycles: u32, angle_of_repose: f64) {
        let mut rng = rand::thread_rng();
        let w = self.width as i32;
        let h = self.height as i32;
        
        for _ in 0..cycles {
            let x = rng.gen_range(1..w - 1);
            let y = rng.gen_range(1..h - 1);
            
            let center_h = self.get(x as u32, y as u32);
            
            // Check all 8 neighbors
            let neighbors = [
                (x - 1, y - 1), (x, y - 1), (x + 1, y - 1),
                (x - 1, y), (x + 1, y),
                (x - 1, y + 1), (x, y + 1), (x + 1, y + 1),
            ];
            
            for (nx, ny) in neighbors {
                if nx < 0 || nx >= w || ny < 0 || ny >= h { continue; }
                
                let neighbor_h = self.get(nx as u32, ny as u32);
                let height_diff = center_h - neighbor_h;
                
                if height_diff > angle_of_repose {
                    // Transfer material downslope
                    let transfer = (height_diff - angle_of_repose) * 0.5;
                    self.set(x as u32, y as u32, center_h - transfer);
                    self.set(nx as u32, ny as u32, neighbor_h + transfer);
                }
            }
        }
    }

    // TERRACING: Quantize heights into distinct levels
    fn apply_terracing(&mut self, levels: u32) {
        if levels == 0 { return; }
        
        let inv_levels = 1.0 / levels as f64;
        for h in self.data.iter_mut() {
            *h = (*h * levels as f64).floor() * inv_levels;
        }
    }

    // FLUID DYNAMICS: Water fills depressions
    fn simulate_fluid(&mut self, iterations: u32) {
        for _ in 0..iterations {
            let w = self.width as i32;
            let h = self.height as i32;
            
            for y in 1..h - 1 {
                for x in 1..w - 1 {
                    let center_h = self.get(x as u32, y as u32);
                    let center_water = self.get_water_level(x as u32, y as u32);
                    let center_total = center_h + center_water;
                    
                    // Check neighbors
                    let neighbors = [(x - 1, y), (x + 1, y), (x, y - 1), (x, y + 1)];
                    let mut avg_level = center_total;
                    let mut count = 1.0;
                    
                    for (nx, ny) in neighbors {
                        let neighbor_h = self.get(nx as u32, ny as u32);
                        let neighbor_water = self.get_water_level(nx as u32, ny as u32);
                        avg_level += neighbor_h + neighbor_water;
                        count += 1.0;
                    }
                    
                    avg_level /= count;
                    
                    // Distribute water
                    let new_water = (avg_level - center_h).max(0.0);
                    self.set_water_level(x as u32, y as u32, new_water * 0.95);
                }
            }
        }
    }
}

fn min_f64(a: f64, b: f64) -> f64 { if a < b { a } else { b } }

// --- MAIN PIPELINE ---

fn main() {
    let args = Args::parse();
    println!("Generating Voxel Map: {}x{} | Seed: {}", args.width, args.height, args.seed);
    println!("Features: Domain Warp={}, Voronoi={}, Fluid={}, Terracing Levels={}, Dither={}, DoF={}",
        args.domain_warp, args.use_voronoi, args.fluid_dynamics, args.terracing_levels, args.dither, args.dof_strength);

    let mut map = HeightMap::new(args.width, args.height, args.seed, args.scale, args.use_voronoi, args.domain_warp);
    
    if args.erosion_cycles > 0 {
        println!("Running hydraulic erosion simulation ({} cycles)...", args.erosion_cycles);
        map.erode(args.erosion_cycles);
    }

    if args.thermal_erosion_cycles > 0 {
        println!("Running thermal erosion simulation ({} cycles)...", args.thermal_erosion_cycles);
        map.erode_thermal(args.thermal_erosion_cycles, 0.08);
    }

    if args.terracing_levels > 0 {
        println!("Applying terracing ({} levels)...", args.terracing_levels);
        map.apply_terracing(args.terracing_levels);
    }

    if args.fluid_dynamics {
        println!("Simulating fluid dynamics...");
        map.simulate_fluid(20);
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
    let mut img = render_frame(map, 0.0, 0.8, 0.0, args.z_scale, args.isometric);
    
    if args.dof_strength > 0.0 {
        img = apply_depth_of_field(&img, args.dof_strength);
    }
    
    let final_img = if args.dither {
        apply_dithering(&img)
    } else {
        img
    };
    
    final_img.save(&args.output).unwrap();
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
        let sun_elevation = (progress * PI).sin().max(0.0) * 0.6 + 0.2;
        
        let mut img = render_frame(map, sun_azimuth, sun_elevation, progress, args.z_scale, args.isometric);
        
        if args.dof_strength > 0.0 {
            img = apply_depth_of_field(&img, args.dof_strength);
        }
        
        let final_img = if args.dither {
            apply_dithering(&img)
        } else {
            img
        };
        
        let frame = Frame::from_parts(final_img, 0, 0, Delay::from_numer_denom_ms(50, 1));
        encoder.encode_frame(frame).unwrap();

        if f % 10 == 0 { println!("Frame {}/{}", f, args.frames); }
    }
}

// --- RENDERING CORE ---

struct RenderPixel {
    color: Rgba<u8>,
    height: f64,
    is_water: bool,
    depth: f64,
}

fn render_frame(map: &HeightMap, sun_angle: f64, sun_elevation: f64, time: f64, z_scale: f64, _isometric: bool) -> RgbaImage {
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
    let sun_color = [1.2, 1.1, 1.0];
    let ambient_sky = [0.2, 0.25, 0.4];
    let sun_intensity = sun_elevation.clamp(0.0, 1.0);

    // --- STEP 1: CALCULATE SURFACE DATA ---
    let mut grid: Vec<RenderPixel> = Vec::with_capacity((width * height) as usize);

    for y in 0..height {
        for x in 0..width {
            let h_base = map.get(x, y);
            
            // Texture Noise
            let tex_val = terrain_noise.get([x as f64 * 0.1, y as f64 * 0.1]) * 0.05;

            // Water Calculation with fluid dynamics
            let (final_h, is_water, base_rgb) = if h_base < SEA_LEVEL {
                let water_height = map.get_water_level(x, y);
                let wave_phase = time * 4.0;
                let wave = water_noise.get([x as f64 * 0.1, y as f64 * 0.1, wave_phase]);
                let wave_h = wave * 0.02;
                
                let depth = (SEA_LEVEL + water_height) - h_base;
                let deep_col = [0.1, 0.3, 0.6];
                let shallow_col = [0.2, 0.5, 0.8];
                let foam_col = [0.9, 0.95, 1.0];
                
                let t = (depth * 10.0).clamp(0.0, 1.0);
                let mut r = deep_col[0] * t + shallow_col[0] * (1.0 - t);
                let mut g = deep_col[1] * t + shallow_col[1] * (1.0 - t);
                let mut b = deep_col[2] * t + shallow_col[2] * (1.0 - t);

                let foam_mask = (wave - 0.5).max(0.0) + (1.0 - (depth / 0.05).clamp(0.0, 1.0));
                if foam_mask > 0.5 {
                    r = r * 0.5 + foam_col[0] * 0.5;
                    g = g * 0.5 + foam_col[1] * 0.5;
                    b = b * 0.5 + foam_col[2] * 0.5;
                }

                (SEA_LEVEL + wave_h, true, [r, g, b])
            } else {
                // Land Biomes with Temperature + Humidity
                let temp = map.get_temp(x, y);
                let humid = map.get_humidity(x, y);
                
                let h = h_base + tex_val;
                let c = get_biome_rgb_advanced(h_base, temp, humid);
                let noise_tint = 1.0 + (tex_val * 2.0); 
                (h, false, [c[0] * noise_tint, c[1] * noise_tint, c[2] * noise_tint])
            };

            // Clamp colors to valid range
            let r = (base_rgb[0] * 255.0).clamp(0.0, 255.0) as u8;
            let g = (base_rgb[1] * 255.0).clamp(0.0, 255.0) as u8;
            let b = (base_rgb[2] * 255.0).clamp(0.0, 255.0) as u8;

            let z_real = final_h * z_scale;
            let depth = z_real / z_scale;

            grid.push(RenderPixel { 
                color: Rgba([r, g, b, 255]),
                height: final_h,
                is_water,
                depth,
            });
        }
    }

    let get_pix = |nx: i32, ny: i32| -> Option<&RenderPixel> {
        if nx >= 0 && nx < width as i32 && ny >= 0 && ny < height as i32 {
            Some(&grid[(ny * width as i32 + nx) as usize])
        } else {
            None
        }
    };

    // --- STEP 2: PROJECT & LIGHTING ---
    let sky_r = (30.0 * (1.0 - sun_elevation) + 135.0 * sun_elevation).clamp(0.0, 255.0) as u8;
    let sky_g = (30.0 * (1.0 - sun_elevation) + 206.0 * sun_elevation).clamp(0.0, 255.0) as u8;
    let sky_b = (60.0 * (1.0 - sun_elevation) + 235.0 * sun_elevation).clamp(0.0, 255.0) as u8;
    let sky_color = Rgba([sky_r, sky_g, sky_b, 255]);

    let project = |x: i32, y: i32, z: f64| -> (i32, i32) {
        let iso_x = x - y;
        let iso_y = ((x + y) as f64 * 0.5 - z) as i32;
        (iso_x, iso_y)
    };
    
    // Auto-centering
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
    
    for y in 0..height {
        for x in 0..width {
            let px_data = &grid[(y * width + x) as usize];
            let z_real = px_data.height * z_scale;
            
            // --- LIGHTING CALCULATION ---
            
            // 1. Shadow raycast
            let mut in_shadow = false;
            let mut ray_h = z_real;
            let mut ray_x = x as f64;
            let mut ray_y = y as f64;
            for _ in 0..40 {
                ray_x -= light_dir_x;
                ray_y -= light_dir_y;
                ray_h += height_step;
                if let Some(obs) = get_pix(ray_x as i32, ray_y as i32) {
                    if obs.height * z_scale > ray_h { in_shadow = true; break; }
                } else { break; }
            }

            // 2. Ambient Occlusion
            let mut ao_darkening: f64 = 0.0;
            let neighbors = [(-1,0), (1,0), (0,-1), (0,1)];
            for (dx, dy) in neighbors {
                if let Some(n) = get_pix(x as i32 + dx, y as i32 + dy) {
                     if n.height > px_data.height {
                         ao_darkening += 0.15;
                     }
                }
            }
            ao_darkening = ao_darkening.min(0.6);

            // 3. Combine Light
            let shadow_mult = if in_shadow { 0.4 } else { 1.0 };
            let total_light = (sun_intensity * shadow_mult) * (1.0 - ao_darkening);
            
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

            // Draw Walls
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

            draw_wall(x as i32, y as i32 + 1);
            draw_wall(x as i32 + 1, y as i32);
        }
    }
    
    img
}

// Advanced biome colors based on Temperature + Humidity
fn get_biome_rgb_advanced(h: f64, temp: f64, humid: f64) -> [f64; 3] {
    if h < SEA_LEVEL { return [0.0, 0.0, 0.0]; }

    let c = match (temp, humid) {
        // Hot + Dry = Desert
        (t, h) if t > 0.65 && h < 0.4 => [0.86, 0.80, 0.55],
        // Hot + Wet = Jungle
        (t, h) if t > 0.65 && h > 0.6 => [0.1, 0.5, 0.1],
        // Cold + Wet = Taiga
        (t, h) if t < 0.35 && h > 0.6 => [0.25, 0.45, 0.25],
        // Cold + Dry = Tundra
        (t, h) if t < 0.35 && h < 0.4 => [0.85, 0.85, 0.85],
        // Temperate Grassland
        (t, h) if t >= 0.35 && t <= 0.65 && h < 0.5 => [0.3, 0.6, 0.2],
        // Temperate Forest
        (t, h) if t >= 0.35 && t <= 0.65 && h >= 0.5 => [0.15, 0.45, 0.15],
        // Default fallback
        _ => [0.4, 0.4, 0.4],
    };

    // Altitude modifier
    if h < 0.45 {
        c
    } else if h < 0.65 {
        // Mix forest/rock
        let mix = (h - 0.45) / 0.2;
        [
            c[0] * (1.0 - mix) + 0.5 * mix,
            c[1] * (1.0 - mix) + 0.45 * mix,
            c[2] * (1.0 - mix) + 0.4 * mix,
        ]
    } else if h < 0.85 {
        // Rock
        [0.5, 0.45, 0.4]
    } else {
        // Snow
        [0.95, 0.95, 1.0]
    }
}

// DEPTH OF FIELD: Blur based on distance
fn apply_depth_of_field(img: &RgbaImage, strength: f64) -> RgbaImage {
    let (width, height) = img.dimensions();
    let mut result = img.clone();
    
    let focal_distance = 0.5;
    let blur_radius = (strength as u32).clamp(1, 10);
    
    for y in blur_radius..(height - blur_radius) {
        for x in blur_radius..(width - blur_radius) {
            let pix = img.get_pixel(x, y);
            
            let cx = width as f64 / 2.0;
            let cy = height as f64 / 2.0;
            let dist_from_center = (((x as f64 - cx).powi(2) + (y as f64 - cy).powi(2)).sqrt()) 
                / ((cx * cx + cy * cy).sqrt());
            
            let blur_amount = ((dist_from_center - focal_distance).abs() * strength * 5.0).clamp(0.0, 1.0);
            
            if blur_amount > 0.1 {
                let mut r_sum = 0u64;
                let mut g_sum = 0u64;
                let mut b_sum = 0u64;
                let mut count = 0u32;
                
                for dy in -(blur_radius as i32)..=(blur_radius as i32) {
                    for dx in -(blur_radius as i32)..=(blur_radius as i32) {
                        let nx = (x as i32 + dx).clamp(0, width as i32 - 1) as u32;
                        let ny = (y as i32 + dy).clamp(0, height as i32 - 1) as u32;
                        let p = img.get_pixel(nx, ny);
                        r_sum += p[0] as u64;
                        g_sum += p[1] as u64;
                        b_sum += p[2] as u64;
                        count += 1;
                    }
                }
                
                let blurred = Rgba([
                    ((r_sum / count as u64) as u8),
                    ((g_sum / count as u64) as u8),
                    ((b_sum / count as u64) as u8),
                    255,
                ]);
                
                let r = (pix[0] as f64 * (1.0 - blur_amount) + blurred[0] as f64 * blur_amount) as u8;
                let g = (pix[1] as f64 * (1.0 - blur_amount) + blurred[1] as f64 * blur_amount) as u8;
                let b = (pix[2] as f64 * (1.0 - blur_amount) + blurred[2] as f64 * blur_amount) as u8;
                
                result.put_pixel(x, y, Rgba([r, g, b, 255]));
            }
        }
    }
    
    result
}

// PALETTE DITHERING: Reduce to Pico-8 palette
fn apply_dithering(img: &RgbaImage) -> RgbaImage {
    let pico8_palette = vec![
        [0x00, 0x00, 0x00],
        [0x1d, 0x2b, 0x53],
        [0x7e, 0x25, 0x53],
        [0x00, 0x87, 0x51],
        [0xab, 0x52, 0x36],
        [0x5f, 0x57, 0x4f],
        [0xc2, 0xc3, 0xc7],
        [0xff, 0xf1, 0xe8],
        [0xff, 0x00, 0x4d],
        [0xff, 0xa3, 0x00],
        [0xff, 0xec, 0x27],
        [0x00, 0xe4, 0x36],
        [0x29, 0xad, 0xff],
        [0x83, 0x76, 0x9c],
        [0xff, 0x77, 0xa8],
        [0xff, 0xcc, 0xaa],
    ];

    let (width, height) = img.dimensions();
    let mut result = img.clone();
    let mut error_buffer = vec![vec![[0.0; 3]; width as usize]; height as usize];

    for y in 0..height {
        for x in 0..width {
            let pix = img.get_pixel(x, y);
            let original = [pix[0] as f64, pix[1] as f64, pix[2] as f64];
            let adjusted = [
                (original[0] + error_buffer[y as usize][x as usize][0]).clamp(0.0, 255.0),
                (original[1] + error_buffer[y as usize][x as usize][1]).clamp(0.0, 255.0),
                (original[2] + error_buffer[y as usize][x as usize][2]).clamp(0.0, 255.0),
            ];

            let mut nearest = 0;
            let mut min_dist = f64::INFINITY;
            for (i, &pal) in pico8_palette.iter().enumerate() {
                let dist = (adjusted[0] - pal[0] as f64).powi(2)
                    + (adjusted[1] - pal[1] as f64).powi(2)
                    + (adjusted[2] - pal[2] as f64).powi(2);
                if dist < min_dist {
                    min_dist = dist;
                    nearest = i;
                }
            }

            let quantized = pico8_palette[nearest];
            result.put_pixel(x, y, Rgba([quantized[0], quantized[1], quantized[2], 255]));

            let error = [
                adjusted[0] - quantized[0] as f64,
                adjusted[1] - quantized[1] as f64,
                adjusted[2] - quantized[2] as f64,
            ];

            if x + 1 < width {
                error_buffer[y as usize][(x + 1) as usize][0] += error[0] * 7.0 / 16.0;
                error_buffer[y as usize][(x + 1) as usize][1] += error[1] * 7.0 / 16.0;
                error_buffer[y as usize][(x + 1) as usize][2] += error[2] * 7.0 / 16.0;
            }
            if y + 1 < height {
                if x > 0 {
                    error_buffer[(y + 1) as usize][(x - 1) as usize][0] += error[0] * 3.0 / 16.0;
                    error_buffer[(y + 1) as usize][(x - 1) as usize][1] += error[1] * 3.0 / 16.0;
                    error_buffer[(y + 1) as usize][(x - 1) as usize][2] += error[2] * 3.0 / 16.0;
                }
                error_buffer[(y + 1) as usize][x as usize][0] += error[0] * 5.0 / 16.0;
                error_buffer[(y + 1) as usize][x as usize][1] += error[1] * 5.0 / 16.0;
                error_buffer[(y + 1) as usize][x as usize][2] += error[2] * 5.0 / 16.0;
                if x + 1 < width {
                    error_buffer[(y + 1) as usize][(x + 1) as usize][0] += error[0] * 1.0 / 16.0;
                    error_buffer[(y + 1) as usize][(x + 1) as usize][1] += error[1] * 1.0 / 16.0;
                    error_buffer[(y + 1) as usize][(x + 1) as usize][2] += error[2] * 1.0 / 16.0;
                }
            }
        }
    }

    result
}
