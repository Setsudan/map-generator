# 🗺️ Rust Ray-Traced Fantasy Map Generator

A high-performance procedural map generator written in Rust. It uses **Fractal Noise** to generate terrain and **Ray-Casting** to simulate realistic 3D lighting, shadows, and day/night cycles.

## Some results

![archipelago img](/imgs/archipelago.png)

![huge img](/imgs/huge.gif)

---

## ✨ Features
* **Procedural Terrain:** Infinite variations using Multi-Fractal Perlin Noise.
* **Ray-Traced Shadows:** Calculates real-time shadows based on sun position and mountain height.
* **Day/Night Cycles:** Generates animated GIFs showing the sun rising, rotating, and setting.
* **Water Physics:** Oceans are flattened (do not cast shadows) and feature specular sun glints.
* **Voxel/Pixel Art Style:** Distinct biome coloring (Deep Water, Sand, Forest, Snow).
* **CLI Support:** Customize resolution, seeds, zoom levels, and output formats.

## 🚀 Quick Start (Docker)
The easiest way to run this is using Docker, which handles all dependencies.

### 1. Build the Image
```bash
docker build -t fantasy-map-generator .
```

### 2. Generate a Map

**Windows (Command Prompt):**

```cmd
mkdir output
docker run --rm -v %cd%/output:/app/output fantasy-map-generator ./target/release/fantasy_map --output ./output/map.png
```

**Windows (PowerShell):**

```powershell
mkdir output
docker run --rm -v ${PWD}/output:/app/output fantasy-map-generator ./target/release/fantasy_map --output ./output/map.png
```

**Mac/Linux:**

```bash
mkdir output
docker run --rm -v "$(pwd)/output":/app/output fantasy-map-generator ./target/release/fantasy_map --output ./output/map.png
```

## 🛠️ Configuration Options

You can pass flags to customize the generation.

| Flag | Default | Description |
| --- | --- | --- |
| `-W` / `--width` | `400` | Width of the image in pixels. |
| `-H` / `--height` | `400` | Height of the image in pixels. |
| `--output` | `output.gif` | Filename. Ends in `.png` for static, `.gif` for animation. |
| `--seed` | `999` | Random seed. Change this to get a completely new world. |
| `--scale` | `150.0` | Zoom level. Higher = Larger continents. Lower = Many small islands. |
| `--z-scale` | `70.0` | Vertical exaggeration. Higher = Taller mountains/Longer shadows. |

## 📦 Local Development (Rust)

If you have Rust installed locally (with C++ Build Tools):

1. `cargo run --release -- --help`
2. `cargo run --release -- --width 800 --output test.png`

## 📝 License

MIT
