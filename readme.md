# Fantasy Map Generator

Procedural world generator in Rust. Tectonic plates and continental crust define large-scale geography; noise adds detail; hydrology carves rivers; climate and biomes color the map.

Default output is a **2D cartographic PNG**. Pass `--isometric` for the voxel renderer (PNG or animated GIF).

## Quick Start (Docker)

```bash
docker build -t fantasy-map-generator .
mkdir output
```

**Windows (PowerShell):**

```powershell
docker run --rm -v ${PWD}/output:/app/output fantasy-map-generator ./target/release/fantasy_map --output ./output/map.png
```

**Mac/Linux:**

```bash
docker run --rm -v "$(pwd)/output":/app/output fantasy-map-generator ./target/release/fantasy_map --output ./output/map.png
```

## Local Development

```bash
cargo run --release -- --help
cargo run --release -- -W 800 -H 800 --seed 12345 --output map.png
```

## Example Commands

```bash
# Default 2D terrain with rivers and coastlines
cargo run --release -- -W 1024 -H 1024 -s 42 --output terrain.png

# Debug layers
cargo run --release -- --layer plates --output plates.png
cargo run --release -- --layer tectonics --output tectonics.png
cargo run --release -- --layer elevation --contours --output elev.png

# World shapes
cargo run --release -- --world-shape pangaea --output pangaea.png
cargo run --release -- --world-shape archipelago --output archipelago.png

# Noise-only comparison
cargo run --release -- --no-tectonics --output noise.png

# Isometric voxel / day-night GIF
cargo run --release -- --isometric --output iso.png
cargo run --release -- --isometric --frames 60 --output daynight.gif
```

## Configuration

| Flag | Default | Description |
| --- | --- | --- |
| `-W` / `--width` | `400` | Map width in pixels |
| `-H` / `--height` | `400` | Map height in pixels |
| `-o` / `--output` | `output.png` | `.png` for static; `.gif` requires `--isometric` |
| `-s` / `--seed` | `42` | World seed |
| `--scale` | `150.0` | Noise zoom (higher = larger features) |
| `--z-scale` | `60.0` | Vertical exaggeration (isometric) |
| `--frames` | `60` | GIF frame count (isometric) |
| `--isometric` | off | Voxel isometric renderer |
| `--sea-level` | `0.25` | Ocean threshold |
| `--equator-position` | `0.5` | Latitude of hottest band (0=north, 1=south) |
| `--temperature-noise` | `0.08` | Small heat-map variation (not random biomes) |
| `--lapse-rate` | `0.35` | Elevation cooling (mountains colder) |
| `--biome-blending` | `0.45` | Soft color blend at biome edges |
| `--world-shape` | `random` | `random`, `continent`, `archipelago`, `supercontinent`, `pangaea`, `inland-sea` |
| `--continent-count` | `5` | Target continental plates (shape-dependent) |
| `--fragmentation` | `0.35` | Continental crust noise |
| `--plates` | `12` | Tectonic plate count |
| `--tectonics` / `--no-tectonics` | on | Plate-driven elevation |
| `--plate-motion` | `1.0` | Plate velocity scale |
| `--mountain-strength` | `0.8` | Convergent uplift |
| `--rift-strength` | `0.5` | Divergent rift / ridge |
| `--volcanic-activity` | `0.3` | Oceanic arcs / island chains |
| `--rivers` / `--no-rivers` | on | Drainage network |
| `--river-threshold` | `250` | Flow accumulation for rivers |
| `--river-width` | `1.5` | Drawn river width scale |
| `--lake-threshold` | `0.02` | Depression fill depth for lakes |
| `--river-erosion` | `0.0` | Carve rivers into elevation |
| `--domain-warp` | off | Domain-warped terrain detail |
| `--erosion-cycles` | `0` | Hydraulic erosion droplets |
| `--thermal-erosion-cycles` | `0` | Thermal (talus) erosion |
| `--terracing-levels` | `0` | Height quantization |
| `--fluid-dynamics` | off | Visual puddle fill |
| `--layer` | `terrain` | `terrain`, `elevation`, `biome`, `temperature`, `humidity`, `plates`, `tectonics`, `rivers`, `contours` |
| `--contours` | off | Contour overlay |
| `--contour-interval` | `0.05` | Contour spacing |
| `--contour-index-every` | `5` | Index contour emphasis |
| `--show-coastlines` | off | Coastline overlay (on by default for terrain layer) |
| `--show-rivers` | off | River overlay on non-terrain layers |
| `--show-plate-boundaries` | off | Plate edge overlay |
| `--dither` | off | Pico-8 palette dither |
| `--dof-strength` | `1.0` | Depth of field (isometric) |

## Pipeline

```text
plates -> continentalness + tectonic uplift -> terrain detail
  -> erosion -> hydrology -> climate -> biomes -> render
```

## License

MIT
