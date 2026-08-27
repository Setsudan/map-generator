use crate::cli::{Args, WorldShape};
use crate::climate;
use crate::erosion;
use crate::hydrology::{self, HydrologyResult};
use crate::biome;
use crate::tectonics::{self, TectonicPlate};
use crate::terrain;

#[derive(Clone, Debug)]
pub struct WorldConfig {
    pub width: u32,
    pub height: u32,
    pub seed: u32,
    pub scale: f64,
    pub sea_level: f64,
    pub domain_warp: bool,
    pub world_shape: WorldShape,
    pub continent_count: u32,
    pub fragmentation: f64,
    pub plates: u32,
    pub tectonics: bool,
    pub plate_motion: f64,
    pub mountain_strength: f64,
    pub rift_strength: f64,
    pub volcanic_activity: f64,
    pub erosion_cycles: u32,
    pub thermal_erosion_cycles: u32,
    pub terracing_levels: u32,
    pub fluid_dynamics: bool,
    pub rivers: bool,
    pub river_threshold: f64,
    pub lake_threshold: f64,
    pub river_erosion: f64,
    pub equator_position: f64,
    pub temperature_noise: f64,
    pub lapse_rate: f64,
    pub biome_blending: f64,
}

impl WorldConfig {
    pub fn from_args(args: &Args) -> Self {
        Self {
            width: args.width,
            height: args.height,
            seed: args.seed,
            scale: args.scale,
            sea_level: args.sea_level,
            domain_warp: args.domain_warp,
            world_shape: args.world_shape,
            continent_count: args.continent_count,
            fragmentation: args.fragmentation,
            plates: args.plates,
            tectonics: args.tectonics,
            plate_motion: args.plate_motion,
            mountain_strength: args.mountain_strength,
            rift_strength: args.rift_strength,
            volcanic_activity: args.volcanic_activity,
            erosion_cycles: args.erosion_cycles,
            thermal_erosion_cycles: args.thermal_erosion_cycles,
            terracing_levels: args.terracing_levels,
            fluid_dynamics: args.fluid_dynamics,
            rivers: args.rivers,
            river_threshold: args.river_threshold,
            lake_threshold: args.lake_threshold,
            river_erosion: args.river_erosion,
            equator_position: args.equator_position,
            temperature_noise: args.temperature_noise,
            lapse_rate: args.lapse_rate,
            biome_blending: args.biome_blending,
        }
    }
}

pub struct World {
    pub width: u32,
    pub height: u32,
    pub sea_level: f64,
    pub plates: Vec<TectonicPlate>,
    pub plate_id: Vec<u16>,
    #[allow(dead_code)]
    pub continentalness: Vec<f64>,
    #[allow(dead_code)]
    pub tectonic: Vec<f64>,
    pub boundary: Vec<u8>,
    pub elevation: Vec<f64>,
    pub temperature: Vec<f64>,
    pub humidity: Vec<f64>,
    pub water_level: Vec<f64>,
    #[allow(dead_code)]
    pub flow_direction: Vec<u8>,
    pub flow_accumulation: Vec<f64>,
    pub river_mask: Vec<bool>,
    pub lake_mask: Vec<bool>,
    pub meltwater_mask: Vec<bool>,
    pub biome: Vec<u8>,
    pub biome_color: Vec<[f64; 3]>,
}

impl World {
    pub fn generate(config: &WorldConfig) -> Self {
        let size = (config.width * config.height) as usize;
        println!(
            "Generating world: {}x{} | Seed: {} | Shape: {:?} | Tectonics: {}",
            config.width, config.height, config.seed, config.world_shape, config.tectonics
        );

        let (
            plates,
            plate_id,
            continentalness,
            tectonic,
            boundary,
            mut elevation,
        ) = if config.tectonics {
            println!(
                "Generating tectonic plates ({})...",
                config.plates
            );
            let tec = tectonics::generate_tectonics(
                config.width,
                config.height,
                config.seed,
                config.plates,
                config.world_shape,
                config.continent_count,
                config.fragmentation,
                config.plate_motion,
                config.mountain_strength,
                config.rift_strength,
                config.volcanic_activity,
            );
            println!("Adding terrain detail...");
            let (mountain, local) = terrain::generate_detail(
                config.width,
                config.height,
                config.seed,
                config.scale,
                config.domain_warp,
            );
            let elev = terrain::combine_elevation(
                &tec.continentalness,
                &tec.tectonic,
                &mountain,
                &local,
            );
            let elev = terrain::soften_elevation(
                &elev,
                config.width,
                config.height,
                config.seed,
                config.sea_level,
            );
            (
                tec.plates,
                tec.plate_id,
                tec.continentalness,
                tec.tectonic,
                tec.boundary,
                elev,
            )
        } else {
            println!("Generating noise-only elevation (--no-tectonics)...");
            let elev = terrain::generate_noise_only_elevation(
                config.width,
                config.height,
                config.seed,
                config.scale,
                config.domain_warp,
            );
            (
                Vec::new(),
                vec![0u16; size],
                elev.clone(),
                vec![0.0; size],
                vec![0u8; size],
                elev,
            )
        };

        if config.erosion_cycles > 0 {
            println!(
                "Running hydraulic erosion ({} cycles)...",
                config.erosion_cycles
            );
            erosion::erode_hydraulic(
                &mut elevation,
                config.width,
                config.height,
                config.erosion_cycles,
            );
        }

        if config.thermal_erosion_cycles > 0 {
            println!(
                "Running thermal erosion ({} cycles)...",
                config.thermal_erosion_cycles
            );
            erosion::erode_thermal(
                &mut elevation,
                config.width,
                config.height,
                config.thermal_erosion_cycles,
                0.08,
            );
        }

        if config.terracing_levels > 0 {
            println!("Applying terracing ({} levels)...", config.terracing_levels);
            erosion::apply_terracing(&mut elevation, config.terracing_levels);
        }

        let mut water_level = vec![0.0; size];
        if config.fluid_dynamics {
            println!("Simulating fluid dynamics...");
            erosion::simulate_fluid(
                &elevation,
                &mut water_level,
                config.width,
                config.height,
                20,
            );
        }

        println!("Generating climate (heat + moisture maps)...");
        let (temperature, humidity) = climate::generate_climate(
            config.width,
            config.height,
            config.seed,
            config.scale,
            &elevation,
            config.sea_level,
            config.equator_position,
            config.temperature_noise,
            config.lapse_rate,
        );

        let hydro = if config.rivers {
            println!("Generating hydrology (rivers / meltwater / lakes)...");
            hydrology::generate_hydrology(
                &mut elevation,
                config.width,
                config.height,
                config.sea_level,
                config.river_threshold,
                config.lake_threshold,
                config.river_erosion,
                Some(&temperature),
            )
        } else {
            HydrologyResult {
                flow_direction: vec![255u8; size],
                flow_accumulation: vec![0.0; size],
                river_mask: vec![false; size],
                lake_mask: vec![false; size],
                meltwater_mask: vec![false; size],
            }
        };

        println!("Classifying biomes (middle bands, cliffs, wetlands)...");
        let (biome, biome_color) = biome::generate_biomes(
            &elevation,
            &temperature,
            &humidity,
            config.width,
            config.height,
            config.sea_level,
            config.biome_blending,
            &hydro.river_mask,
            &hydro.lake_mask,
        );

        World {
            width: config.width,
            height: config.height,
            sea_level: config.sea_level,
            plates,
            plate_id,
            continentalness,
            tectonic,
            boundary,
            elevation,
            temperature,
            humidity,
            water_level,
            flow_direction: hydro.flow_direction,
            flow_accumulation: hydro.flow_accumulation,
            river_mask: hydro.river_mask,
            lake_mask: hydro.lake_mask,
            meltwater_mask: hydro.meltwater_mask,
            biome,
            biome_color,
        }
    }

    pub fn idx(&self, x: u32, y: u32) -> usize {
        (y * self.width + x) as usize
    }

    pub fn get_elevation(&self, x: u32, y: u32) -> f64 {
        if x >= self.width || y >= self.height {
            return 0.0;
        }
        self.elevation[self.idx(x, y)]
    }

    pub fn get_temp(&self, x: u32, y: u32) -> f64 {
        if x >= self.width || y >= self.height {
            return 0.5;
        }
        self.temperature[self.idx(x, y)]
    }

    pub fn get_humidity(&self, x: u32, y: u32) -> f64 {
        if x >= self.width || y >= self.height {
            return 0.5;
        }
        self.humidity[self.idx(x, y)]
    }

    pub fn get_water_level(&self, x: u32, y: u32) -> f64 {
        if x >= self.width || y >= self.height {
            return 0.0;
        }
        self.water_level[self.idx(x, y)]
    }
}
