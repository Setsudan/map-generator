use image::{Rgba, RgbaImage};

pub fn apply_depth_of_field(img: &RgbaImage, strength: f64) -> RgbaImage {
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

            let blur_amount =
                ((dist_from_center - focal_distance).abs() * strength * 5.0).clamp(0.0, 1.0);

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
                    (r_sum / count as u64) as u8,
                    (g_sum / count as u64) as u8,
                    (b_sum / count as u64) as u8,
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

pub fn apply_dithering(img: &RgbaImage) -> RgbaImage {
    let pico8_palette = [
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
