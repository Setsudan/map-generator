use crate::biome::biome_rgb_advanced;
use crate::cli::Args;
use crate::render::post::{apply_depth_of_field, apply_dithering};
use crate::world::World;
use image::codecs::gif::GifEncoder;
use image::{Delay, Frame, Rgba, RgbaImage};
use noise::{NoiseFn, Perlin};
use std::cmp::{max, min};
use std::f64::consts::PI;
use std::fs::File;

struct RenderPixel {
    color: Rgba<u8>,
    height: f64,
    #[allow(dead_code)]
    is_water: bool,
    #[allow(dead_code)]
    depth: f64,
}

pub fn render_isometric_static(world: &World, args: &Args) {
    println!("Rendering isometric static frame...");
    let mut img = render_frame(world, 0.0, 0.8, 0.0, args.z_scale);

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

pub fn render_gif(world: &World, args: &Args) {
    println!("Rendering isometric GIF ({} frames)...", args.frames);
    let file_out = File::create(&args.output).unwrap();
    let mut encoder = GifEncoder::new(file_out);
    encoder
        .set_repeat(image::codecs::gif::Repeat::Infinite)
        .unwrap();

    for f in 0..args.frames {
        let progress = f as f64 / args.frames as f64;
        let sun_azimuth = progress * 2.0 * PI;
        let sun_elevation = (progress * PI).sin().max(0.0) * 0.6 + 0.2;

        let mut img = render_frame(world, sun_azimuth, sun_elevation, progress, args.z_scale);

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

        if f % 10 == 0 {
            println!("Frame {}/{}", f, args.frames);
        }
    }
    println!("Saved to '{}'", args.output);
}

fn render_frame(
    world: &World,
    sun_angle: f64,
    sun_elevation: f64,
    time: f64,
    z_scale: f64,
) -> RgbaImage {
    let width = world.width;
    let height = world.height;
    let sea_level = world.sea_level;

    let water_noise = Perlin::new(100);
    let terrain_noise = Perlin::new(200);

    let light_dir_x = -sun_angle.cos();
    let light_dir_y = -sun_angle.sin();
    let height_step = sun_elevation.tan();

    let sun_color = [1.2, 1.1, 1.0];
    let ambient_sky = [0.2, 0.25, 0.4];
    let sun_intensity = sun_elevation.clamp(0.0, 1.0);

    let mut grid: Vec<RenderPixel> = Vec::with_capacity((width * height) as usize);

    for y in 0..height {
        for x in 0..width {
            let h_base = world.get_elevation(x, y);
            let tex_val = terrain_noise.get([x as f64 * 0.1, y as f64 * 0.1]) * 0.05;
            let i = world.idx(x, y);

            let (final_h, is_water, base_rgb) = if h_base < sea_level {
                let water_height = world.get_water_level(x, y);
                let wave_phase = time * 4.0;
                let wave = water_noise.get([x as f64 * 0.1, y as f64 * 0.1, wave_phase]);
                let wave_h = wave * 0.02;

                let depth = (sea_level + water_height) - h_base;
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

                (sea_level + wave_h, true, [r, g, b])
            } else if world.lake_mask[i] || world.river_mask[i] {
                let c = if world.lake_mask[i] {
                    [0.15, 0.4, 0.7]
                } else {
                    [0.2, 0.45, 0.75]
                };
                (h_base.max(sea_level) + 0.01, true, c)
            } else {
                let temp = world.get_temp(x, y);
                let humid = world.get_humidity(x, y);
                let h = h_base + tex_val;
                let c = biome_rgb_advanced(h_base, temp, humid, sea_level);
                let noise_tint = 1.0 + (tex_val * 2.0);
                (
                    h,
                    false,
                    [c[0] * noise_tint, c[1] * noise_tint, c[2] * noise_tint],
                )
            };

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

    let sky_r = (30.0 * (1.0 - sun_elevation) + 135.0 * sun_elevation).clamp(0.0, 255.0) as u8;
    let sky_g = (30.0 * (1.0 - sun_elevation) + 206.0 * sun_elevation).clamp(0.0, 255.0) as u8;
    let sky_b = (60.0 * (1.0 - sun_elevation) + 235.0 * sun_elevation).clamp(0.0, 255.0) as u8;
    let sky_color = Rgba([sky_r, sky_g, sky_b, 255]);

    let project = |x: i32, y: i32, z: f64| -> (i32, i32) {
        let iso_x = x - y;
        let iso_y = ((x + y) as f64 * 0.5 - z) as i32;
        (iso_x, iso_y)
    };

    let mut min_x = i32::MAX;
    let mut max_x = i32::MIN;
    let mut min_y = i32::MAX;
    let mut max_y = i32::MIN;
    let corners = [
        (0, 0),
        (width as i32, 0),
        (0, height as i32),
        (width as i32, height as i32),
    ];
    for (cx, cy) in corners {
        for &cz in &[0.0, z_scale] {
            let (px, py) = project(cx, cy, cz);
            min_x = min(min_x, px);
            max_x = max(max_x, px);
            min_y = min(min_y, py);
            max_y = max(max_y, py);
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

            let mut in_shadow = false;
            let mut ray_h = z_real;
            let mut ray_x = x as f64;
            let mut ray_y = y as f64;
            for _ in 0..40 {
                ray_x -= light_dir_x;
                ray_y -= light_dir_y;
                ray_h += height_step;
                if let Some(obs) = get_pix(ray_x as i32, ray_y as i32) {
                    if obs.height * z_scale > ray_h {
                        in_shadow = true;
                        break;
                    }
                } else {
                    break;
                }
            }

            let mut ao_darkening: f64 = 0.0;
            let neighbors = [(-1, 0), (1, 0), (0, -1), (0, 1)];
            for (dx, dy) in neighbors {
                if let Some(n) = get_pix(x as i32 + dx, y as i32 + dy) {
                    if n.height > px_data.height {
                        ao_darkening += 0.15;
                    }
                }
            }
            ao_darkening = ao_darkening.min(0.6);

            let shadow_mult = if in_shadow { 0.4 } else { 1.0 };
            let total_light = (sun_intensity * shadow_mult) * (1.0 - ao_darkening);

            let r = (px_data.color[0] as f64
                * (total_light * sun_color[0] + ambient_sky[0] * 0.4))
                .clamp(0.0, 255.0) as u8;
            let g = (px_data.color[1] as f64
                * (total_light * sun_color[1] + ambient_sky[1] * 0.4))
                .clamp(0.0, 255.0) as u8;
            let b = (px_data.color[2] as f64
                * (total_light * sun_color[2] + ambient_sky[2] * 0.4))
                .clamp(0.0, 255.0) as u8;

            let final_col = Rgba([r, g, b, 255]);

            let (iso_x, iso_y) = project(x as i32, y as i32, z_real);
            let draw_x = iso_x + off_x;
            let draw_y = iso_y + off_y;

            if draw_x >= 0 && draw_x < out_w as i32 && draw_y >= 0 && draw_y < out_h as i32 {
                img.put_pixel(draw_x as u32, draw_y as u32, final_col);
            }

            let wall_col = Rgba([
                (r as f64 * 0.7) as u8,
                (g as f64 * 0.7) as u8,
                (b as f64 * 0.7) as u8,
                255,
            ]);

            let mut draw_wall = |nx: i32, ny: i32| {
                let neighbor_z = if let Some(n) = get_pix(nx, ny) {
                    n.height * z_scale
                } else {
                    0.0
                };
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
