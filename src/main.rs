mod biome;
mod climate;
mod cli;
mod erosion;
mod hydrology;
mod render;
mod tectonics;
mod terrain;
mod world;

use clap::Parser;
use cli::Args;
use std::path::Path;
use world::{World, WorldConfig};

fn main() {
    let args = Args::parse();
    let config = WorldConfig::from_args(&args);
    let world = World::generate(&config);

    let path = Path::new(&args.output);
    let extension = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("png")
        .to_lowercase();

    if args.isometric {
        if extension == "gif" {
            render::render_gif(&world, &args);
        } else {
            render::render_isometric_static(&world, &args);
        }
    } else {
        if extension == "gif" {
            eprintln!(
                "GIF output requires --isometric (day/night voxel animation). Use .png for 2D maps."
            );
            std::process::exit(1);
        }
        println!("Rendering 2D map layer {:?}...", args.layer);
        let img = render::render_map2d(&world, &args);
        let final_img = if args.dither {
            render::apply_dithering(&img)
        } else {
            img
        };
        final_img.save(&args.output).unwrap();
        println!("Saved to '{}'", args.output);
    }
}
