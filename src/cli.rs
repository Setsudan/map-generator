use clap::{Parser, ValueEnum};

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum WorldShape {
    Random,
    Continent,
    Archipelago,
    Supercontinent,
    Pangaea,
    InlandSea,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum MapLayer {
    Terrain,
    Elevation,
    Biome,
    Temperature,
    Humidity,
    Plates,
    Tectonics,
    Rivers,
    Contours,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ContourMode {
    Pixel,
}

#[derive(Parser, Debug)]
#[command(
    author,
    version,
    about = "Procedural fantasy map generator with tectonics, hydrology, and cartographic layers"
)]
pub struct Args {
    #[arg(short = 'W', long, default_value_t = 400)]
    pub width: u32,

    #[arg(short = 'H', long, default_value_t = 400)]
    pub height: u32,

    #[arg(short, long, default_value = "output.png")]
    pub output: String,

    #[arg(short, long, default_value_t = 42)]
    pub seed: u32,

    #[arg(long, default_value_t = 150.0)]
    pub scale: f64,

    #[arg(long, default_value_t = 60.0)]
    pub z_scale: f64,

    #[arg(long, default_value_t = 60)]
    pub frames: u32,

    #[arg(long, action)]
    pub isometric: bool,

    #[arg(long, default_value_t = 0)]
    pub erosion_cycles: u32,

    #[arg(long, default_value_t = 0)]
    pub thermal_erosion_cycles: u32,

    #[arg(long, default_value_t = 0)]
    pub terracing_levels: u32,

    #[arg(long, action)]
    pub domain_warp: bool,

    #[arg(long, action)]
    pub fluid_dynamics: bool,

    #[arg(long, action)]
    pub dither: bool,

    #[arg(long, default_value_t = 1.0)]
    pub dof_strength: f64,

    // --- World generation ---
    #[arg(long, value_enum, default_value_t = WorldShape::Random)]
    pub world_shape: WorldShape,

    #[arg(long, default_value_t = 5)]
    pub continent_count: u32,

    #[arg(long, default_value_t = 0.35)]
    pub fragmentation: f64,

    #[arg(long, default_value_t = 0.25)]
    pub sea_level: f64,

    // --- Climate ---
    #[arg(long, default_value_t = 0.5)]
    pub equator_position: f64,

    #[arg(long, default_value_t = 0.08)]
    pub temperature_noise: f64,

    #[arg(long, default_value_t = 0.35)]
    pub lapse_rate: f64,

    #[arg(long, default_value_t = 0.55)]
    pub biome_blending: f64,

    // --- Tectonics ---
    #[arg(long, default_value_t = 12)]
    pub plates: u32,

    #[arg(long, default_value_t = true, action = clap::ArgAction::SetTrue)]
    #[arg(long = "no-tectonics", action = clap::ArgAction::SetFalse)]
    pub tectonics: bool,

    #[arg(long, default_value_t = 1.0)]
    pub plate_motion: f64,

    #[arg(long, default_value_t = 0.8)]
    pub mountain_strength: f64,

    #[arg(long, default_value_t = 0.5)]
    pub rift_strength: f64,

    #[arg(long, default_value_t = 0.3)]
    pub volcanic_activity: f64,

    // --- Hydrology ---
    #[arg(long, default_value_t = true, action = clap::ArgAction::SetTrue)]
    #[arg(long = "no-rivers", action = clap::ArgAction::SetFalse)]
    pub rivers: bool,

    #[arg(long, default_value_t = 900.0)]
    pub river_threshold: f64,

    #[arg(long, default_value_t = 1.5)]
    pub river_width: f64,

    #[arg(long, default_value_t = 0.02)]
    pub lake_threshold: f64,

    #[arg(long, default_value_t = 0.0)]
    pub river_erosion: f64,

    // --- Rendering ---
    #[arg(long, value_enum, default_value_t = MapLayer::Terrain)]
    pub layer: MapLayer,

    #[arg(long, action)]
    pub contours: bool,

    #[arg(long, default_value_t = 0.05)]
    pub contour_interval: f64,

    #[arg(long, default_value_t = 5)]
    pub contour_index_every: u32,

    #[arg(long, value_enum, default_value_t = ContourMode::Pixel)]
    pub contour_mode: ContourMode,

    #[arg(long, action)]
    pub show_coastlines: bool,

    #[arg(long, action)]
    pub show_rivers: bool,

    #[arg(long, action)]
    pub show_plate_boundaries: bool,
}
