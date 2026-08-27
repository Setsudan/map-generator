use crate::biome::BiomeId;
use crate::cli::{Args, MapLayer};
use crate::tectonics::{boundary_type_from_u8, BoundaryType};
use crate::world::World;
use image::{Rgba, RgbaImage};

pub fn render_map2d(world: &World, args: &Args) -> RgbaImage {
    let mut img = match args.layer {
        MapLayer::Terrain => render_terrain(world),
        MapLayer::Elevation => render_elevation(world),
        MapLayer::Biome => render_biome_layer(world),
        MapLayer::Temperature => render_scalar(&world.temperature, world.width, world.height, heat_ramp),
        MapLayer::Humidity => render_scalar(&world.humidity, world.width, world.height, moisture_ramp),
        MapLayer::Plates => render_plates(world),
        MapLayer::Tectonics => render_tectonics(world),
        MapLayer::Rivers => render_rivers_layer(world),
        MapLayer::Contours => {
            let mut base = render_elevation(world);
            draw_contours(&mut base, world, args);
            base
        }
    };

    // Overlays (compose onto most layers)
    let overlay_ok = matches!(
        args.layer,
        MapLayer::Terrain
            | MapLayer::Elevation
            | MapLayer::Biome
            | MapLayer::Contours
            | MapLayer::Plates
            | MapLayer::Tectonics
    );

    if args.layer == MapLayer::Terrain {
        draw_lakes(&mut img, world);
        // Draw meltwater / major rivers (long downhill paths from cold high ground)
        if args.rivers || args.show_rivers {
            draw_rivers(&mut img, world, args.river_width);
        }
        if args.show_coastlines {
            draw_coastlines(&mut img, world);
        }
    } else if overlay_ok {
        if args.show_rivers {
            draw_rivers(&mut img, world, args.river_width);
        }
        if args.show_coastlines {
            draw_coastlines(&mut img, world);
        }
    }

    if args.contours && args.layer != MapLayer::Contours && overlay_ok {
        draw_contours(&mut img, world, args);
    }
    if args.show_plate_boundaries && overlay_ok {
        draw_plate_boundaries(&mut img, world);
    }

    img
}

fn render_terrain(world: &World) -> RgbaImage {
    let mut img = RgbaImage::new(world.width, world.height);

    for y in 0..world.height {
        for x in 0..world.width {
            let i = world.idx(x, y);
            let rgb = if world.lake_mask[i] {
                [40, 100, 170]
            } else {
                let c = world.biome_color[i];
                [
                    (c[0] * 255.0).clamp(0.0, 255.0) as u8,
                    (c[1] * 255.0).clamp(0.0, 255.0) as u8,
                    (c[2] * 255.0).clamp(0.0, 255.0) as u8,
                ]
            };
            img.put_pixel(x, y, Rgba([rgb[0], rgb[1], rgb[2], 255]));
        }
    }
    img
}

fn render_elevation(world: &World) -> RgbaImage {
    let mut img = RgbaImage::new(world.width, world.height);
    for y in 0..world.height {
        for x in 0..world.width {
            let h = world.elevation[world.idx(x, y)];
            let v = (h * 255.0).clamp(0.0, 255.0) as u8;
            if h < world.sea_level {
                img.put_pixel(x, y, Rgba([20, 40, 80, 255]));
            } else {
                img.put_pixel(x, y, Rgba([v, v, v, 255]));
            }
        }
    }
    img
}

fn render_biome_layer(world: &World) -> RgbaImage {
    let mut img = RgbaImage::new(world.width, world.height);
    for y in 0..world.height {
        for x in 0..world.width {
            let id = BiomeId::from_u8(world.biome[world.idx(x, y)]);
            let rgb = id.palette_rgb();
            img.put_pixel(x, y, Rgba([rgb[0], rgb[1], rgb[2], 255]));
        }
    }
    img
}

fn render_scalar(
    data: &[f64],
    width: u32,
    height: u32,
    ramp: fn(f64) -> [u8; 3],
) -> RgbaImage {
    let mut img = RgbaImage::new(width, height);
    for y in 0..height {
        for x in 0..width {
            let v = data[(y * width + x) as usize];
            let rgb = ramp(v);
            img.put_pixel(x, y, Rgba([rgb[0], rgb[1], rgb[2], 255]));
        }
    }
    img
}

fn heat_ramp(t: f64) -> [u8; 3] {
    let t = t.clamp(0.0, 1.0);
    [
        (t * 255.0) as u8,
        ((1.0 - (t - 0.5).abs() * 2.0) * 180.0) as u8,
        ((1.0 - t) * 255.0) as u8,
    ]
}

fn moisture_ramp(h: f64) -> [u8; 3] {
    let h = h.clamp(0.0, 1.0);
    [
        ((1.0 - h) * 200.0) as u8,
        (100.0 + h * 120.0) as u8,
        (80.0 + h * 175.0) as u8,
    ]
}

fn render_plates(world: &World) -> RgbaImage {
    let mut img = RgbaImage::new(world.width, world.height);
    let n = world.plates.len().max(1);
    for y in 0..world.height {
        for x in 0..world.width {
            let id = world.plate_id[world.idx(x, y)] as usize;
            let hue = (id as f64 / n as f64) * 360.0;
            let (r, g, b) = hsv_to_rgb(hue, 0.55, 0.85);
            // Darken oceanic plates
            let continental = world
                .plates
                .get(id)
                .map(|p| p.continental)
                .unwrap_or(true);
            let factor = if continental { 1.0 } else { 0.55 };
            img.put_pixel(
                x,
                y,
                Rgba([
                    (r as f64 * factor) as u8,
                    (g as f64 * factor) as u8,
                    (b as f64 * factor) as u8,
                    255,
                ]),
            );
        }
    }
    img
}

fn render_tectonics(world: &World) -> RgbaImage {
    let mut img = render_plates(world);
    let w = world.width as i32;
    let h = world.height as i32;
    for y in 0..world.height {
        for x in 0..world.width {
            let i = world.idx(x, y);
            let pid = world.plate_id[i];
            let mut edge = false;
            for (dx, dy) in [(1i32, 0), (0, 1), (-1, 0), (0, -1)] {
                let nx = x as i32 + dx;
                let ny = y as i32 + dy;
                if nx < 0 || ny < 0 || nx >= w || ny >= h {
                    continue;
                }
                if world.plate_id[world.idx(nx as u32, ny as u32)] != pid {
                    edge = true;
                    break;
                }
            }
            if !edge {
                continue;
            }
            let b = boundary_type_from_u8(world.boundary[i]);
            let color = match b {
                BoundaryType::Convergent => [220, 40, 40],
                BoundaryType::Divergent => [40, 180, 80],
                BoundaryType::Transform => [240, 200, 40],
                BoundaryType::None => [200, 200, 200],
            };
            img.put_pixel(x, y, Rgba([color[0], color[1], color[2], 255]));
        }
    }
    img
}

fn render_rivers_layer(world: &World) -> RgbaImage {
    let mut img = RgbaImage::from_pixel(world.width, world.height, Rgba([20, 20, 30, 255]));
    let max_acc = world
        .flow_accumulation
        .iter()
        .cloned()
        .fold(1.0f64, f64::max);
    for y in 0..world.height {
        for x in 0..world.width {
            let i = world.idx(x, y);
            if world.elevation[i] < world.sea_level {
                img.put_pixel(x, y, Rgba([15, 35, 70, 255]));
                continue;
            }
            let t = (world.flow_accumulation[i] / max_acc).sqrt().clamp(0.0, 1.0);
            let v = (t * 255.0) as u8;
            img.put_pixel(x, y, Rgba([v / 4, v / 2, v, 255]));
            if world.river_mask[i] {
                img.put_pixel(x, y, Rgba([80, 160, 255, 255]));
            }
            if world.lake_mask[i] {
                img.put_pixel(x, y, Rgba([40, 100, 200, 255]));
            }
        }
    }
    img
}

fn draw_contours(img: &mut RgbaImage, world: &World, args: &Args) {
    let interval = args.contour_interval.max(0.001);
    let index_every = args.contour_index_every.max(1);
    let w = world.width as i32;
    let h = world.height as i32;

    for y in 0..world.height {
        for x in 0..world.width {
            let i = world.idx(x, y);
            let elev = world.elevation[i];
            if elev < world.sea_level {
                continue;
            }
            let band = (elev / interval).floor() as i32;
            let mut is_contour = false;
            for (dx, dy) in [(1i32, 0), (0, 1)] {
                let nx = x as i32 + dx;
                let ny = y as i32 + dy;
                if nx < 0 || ny < 0 || nx >= w || ny >= h {
                    continue;
                }
                let n_elev = world.elevation[world.idx(nx as u32, ny as u32)];
                if n_elev < world.sea_level {
                    continue;
                }
                let n_band = (n_elev / interval).floor() as i32;
                if n_band != band {
                    is_contour = true;
                    break;
                }
            }
            if is_contour {
                let is_index = band.rem_euclid(index_every as i32) == 0;
                let c = if is_index {
                    Rgba([30, 30, 30, 255])
                } else {
                    Rgba([70, 70, 70, 255])
                };
                img.put_pixel(x, y, c);
            }
        }
    }
}

fn draw_coastlines(img: &mut RgbaImage, world: &World) {
    let w = world.width as i32;
    let h = world.height as i32;
    for y in 0..world.height {
        for x in 0..world.width {
            let land = world.elevation[world.idx(x, y)] >= world.sea_level;
            if !land {
                continue;
            }
            let mut coast = false;
            for (dx, dy) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
                let nx = x as i32 + dx;
                let ny = y as i32 + dy;
                if nx < 0 || ny < 0 || nx >= w || ny >= h {
                    coast = true;
                    break;
                }
                if world.elevation[world.idx(nx as u32, ny as u32)] < world.sea_level {
                    coast = true;
                    break;
                }
            }
            if coast {
                img.put_pixel(x, y, Rgba([20, 20, 20, 255]));
            }
        }
    }
}

fn draw_rivers(img: &mut RgbaImage, world: &World, river_width: f64) {
    let max_acc = world
        .flow_accumulation
        .iter()
        .cloned()
        .fold(1.0f64, f64::max);
    let major_floor = (max_acc * 0.025).max(500.0);
    let has_melt = world.meltwater_mask.len() == world.river_mask.len();

    for y in 0..world.height {
        for x in 0..world.width {
            let i = world.idx(x, y);
            let is_melt = has_melt && world.meltwater_mask[i];
            let is_major = world.river_mask[i] && world.flow_accumulation[i] >= major_floor;
            if !is_melt && !is_major {
                continue;
            }

            let acc = world.flow_accumulation[i].max(1.0);
            let width_factor = (acc.ln() / max_acc.ln().max(1.0)).clamp(0.0, 1.0);
            let radius = (river_width * (0.5 + width_factor)).round().max(1.0) as i32;
            let color = Rgba([35, 115, 195, 255]);

            for dy in -radius..=radius {
                for dx in -radius..=radius {
                    if dx * dx + dy * dy > radius * radius {
                        continue;
                    }
                    let nx = x as i32 + dx;
                    let ny = y as i32 + dy;
                    if nx < 0 || ny < 0 || nx >= world.width as i32 || ny >= world.height as i32 {
                        continue;
                    }
                    let ni = world.idx(nx as u32, ny as u32);
                    if world.elevation[ni] < world.sea_level {
                        continue;
                    }
                    img.put_pixel(nx as u32, ny as u32, color);
                }
            }
        }
    }
}

fn draw_lakes(img: &mut RgbaImage, world: &World) {
    for y in 0..world.height {
        for x in 0..world.width {
            let i = world.idx(x, y);
            if world.lake_mask[i] {
                img.put_pixel(x, y, Rgba([40, 100, 170, 255]));
            }
        }
    }
}

fn draw_plate_boundaries(img: &mut RgbaImage, world: &World) {
    let w = world.width as i32;
    let h = world.height as i32;
    for y in 0..world.height {
        for x in 0..world.width {
            let i = world.idx(x, y);
            let pid = world.plate_id[i];
            let mut edge = false;
            for (dx, dy) in [(1i32, 0), (0, 1)] {
                let nx = x as i32 + dx;
                let ny = y as i32 + dy;
                if nx < 0 || ny < 0 || nx >= w || ny >= h {
                    continue;
                }
                if world.plate_id[world.idx(nx as u32, ny as u32)] != pid {
                    edge = true;
                    break;
                }
            }
            if !edge {
                continue;
            }
            let b = boundary_type_from_u8(world.boundary[i]);
            let color = match b {
                BoundaryType::Convergent => [200, 50, 50],
                BoundaryType::Divergent => [50, 180, 80],
                BoundaryType::Transform => [220, 190, 40],
                BoundaryType::None => [180, 180, 180],
            };
            img.put_pixel(x, y, Rgba([color[0], color[1], color[2], 255]));
        }
    }
}

fn hsv_to_rgb(h: f64, s: f64, v: f64) -> (u8, u8, u8) {
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = v - c;
    let (r, g, b) = if h < 60.0 {
        (c, x, 0.0)
    } else if h < 120.0 {
        (x, c, 0.0)
    } else if h < 180.0 {
        (0.0, c, x)
    } else if h < 240.0 {
        (0.0, x, c)
    } else if h < 300.0 {
        (x, 0.0, c)
    } else {
        (c, 0.0, x)
    };
    (
        ((r + m) * 255.0) as u8,
        ((g + m) * 255.0) as u8,
        ((b + m) * 255.0) as u8,
    )
}
