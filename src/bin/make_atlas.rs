use bevy::asset::RenderAssetUsages;
use bevy::image::{CompressedImageFormats, Image, ImageSampler, ImageType};
#[path = "../material_catalog.rs"]
mod material_catalog;

use material_catalog::{
    TextureId, atlas_texture_order, source_base_filename, source_overlay_filename,
};
use png::{BitDepth, ColorType, Encoder};
use std::env;
use std::fs;
use std::io::BufWriter;
use std::path::{Path, PathBuf};

/// RGBA pixel stride in bytes.
const RGBA_STRIDE: usize = 4;

/// Print CLI usage.
fn print_usage(program: &str) {
    eprintln!(
        "Usage: {program} --source-dir <dir> [--output <path>]\n\
         Default output: assets/textures/atlas.png\n\
         Required files in <dir> are defined by shared material_catalog."
    );
}

/// Parse simple CLI args for source dir and output path.
fn parse_args() -> Result<(PathBuf, PathBuf), String> {
    let mut source_dir: Option<PathBuf> = None;
    let mut output = PathBuf::from("assets/textures/atlas.png");

    let mut it = env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--source-dir" => {
                let value = it
                    .next()
                    .ok_or_else(|| "--source-dir requires a value".to_string())?;
                source_dir = Some(PathBuf::from(value));
            }
            "--output" => {
                let value = it
                    .next()
                    .ok_or_else(|| "--output requires a value".to_string())?;
                output = PathBuf::from(value);
            }
            "--help" | "-h" => {
                let program = env::args()
                    .next()
                    .unwrap_or_else(|| "make_atlas".to_string());
                print_usage(&program);
                std::process::exit(0);
            }
            _ => return Err(format!("Unknown argument: {arg}")),
        }
    }

    let source = source_dir.ok_or_else(|| "--source-dir is required".to_string())?;
    Ok((source, output))
}

/// Decoded RGBA8 texture payload.
struct RgbaTexture {
    /// Image width in pixels.
    width: u32,
    /// Image height in pixels.
    height: u32,
    /// Raw RGBA8 bytes in row-major order.
    data: Vec<u8>,
}

/// Decode PNG into RGBA8 using Bevy image loader stack.
fn load_rgba8(path: &Path) -> Result<RgbaTexture, String> {
    let bytes = fs::read(path).map_err(|e| format!("Failed to read {}: {e}", path.display()))?;
    let image = Image::from_buffer(
        &bytes,
        ImageType::Extension("png"),
        CompressedImageFormats::NONE,
        true,
        ImageSampler::default(),
        RenderAssetUsages::default(),
    )
    .map_err(|e| format!("Failed to decode {}: {e}", path.display()))?;

    let width = image.width();
    let height = image.height();
    let data = image
        .data
        .ok_or_else(|| format!("Image {} has no pixel data", path.display()))?;
    let expected_len = width as usize * height as usize * RGBA_STRIDE;
    if data.len() != expected_len {
        return Err(format!(
            "Image {} is not RGBA8-compatible: got {} bytes, expected {}",
            path.display(),
            data.len(),
            expected_len
        ));
    }

    Ok(RgbaTexture {
        width,
        height,
        data,
    })
}

/// Verify all tile dimensions are equal.
fn ensure_same_size(images: &[(&str, &RgbaTexture)]) -> Result<(u32, u32), String> {
    let (name0, first) = images[0];
    let (w, h) = (first.width, first.height);
    for (name, img) in images.iter().skip(1) {
        if img.width != w || img.height != h {
            return Err(format!(
                "Tile size mismatch: {name0} is {w}x{h}, {name} is {}x{}",
                img.width, img.height
            ));
        }
    }
    Ok((w, h))
}

/// Composite `top` over `base` using non-premultiplied alpha.
fn alpha_blend(base: &RgbaTexture, top: &RgbaTexture) -> Result<RgbaTexture, String> {
    if base.width != top.width || base.height != top.height {
        return Err(format!(
            "Overlay size mismatch: base {}x{}, overlay {}x{}",
            base.width, base.height, top.width, top.height
        ));
    }
    let mut out = base.data.clone();
    for (dst_px, src_px) in out
        .chunks_exact_mut(RGBA_STRIDE)
        .zip(top.data.chunks_exact(RGBA_STRIDE))
    {
        let dst_r = dst_px[0] as f32 / 255.0;
        let dst_g = dst_px[1] as f32 / 255.0;
        let dst_b = dst_px[2] as f32 / 255.0;
        let dst_a = dst_px[3] as f32 / 255.0;
        let src_r = src_px[0] as f32 / 255.0;
        let src_g = src_px[1] as f32 / 255.0;
        let src_b = src_px[2] as f32 / 255.0;
        let src_a = src_px[3] as f32 / 255.0;

        let out_a = src_a + dst_a * (1.0 - src_a);
        let (out_r, out_g, out_b) = if out_a > 0.0 {
            (
                (src_r * src_a + dst_r * dst_a * (1.0 - src_a)) / out_a,
                (src_g * src_a + dst_g * dst_a * (1.0 - src_a)) / out_a,
                (src_b * src_a + dst_b * dst_a * (1.0 - src_a)) / out_a,
            )
        } else {
            (0.0, 0.0, 0.0)
        };

        dst_px[0] = (out_r * 255.0).round().clamp(0.0, 255.0) as u8;
        dst_px[1] = (out_g * 255.0).round().clamp(0.0, 255.0) as u8;
        dst_px[2] = (out_b * 255.0).round().clamp(0.0, 255.0) as u8;
        dst_px[3] = (out_a * 255.0).round().clamp(0.0, 255.0) as u8;
    }

    Ok(RgbaTexture {
        width: base.width,
        height: base.height,
        data: out,
    })
}

/// Build atlas RGBA bytes in horizontal order from provided tile list.
fn build_atlas_data(tiles: &[RgbaTexture]) -> Vec<u8> {
    let tile_w = tiles[0].width as usize;
    let tile_h = tiles[0].height as usize;
    let atlas_w = tile_w * tiles.len();
    let row_bytes = atlas_w * RGBA_STRIDE;
    let tile_row_bytes = tile_w * RGBA_STRIDE;
    let mut out = vec![0_u8; row_bytes * tile_h];

    for y in 0..tile_h {
        let out_row = y * row_bytes;
        let src_row = y * tile_row_bytes;
        for (i, tile) in tiles.iter().enumerate() {
            let dst_start = out_row + i * tile_row_bytes;
            let dst_end = dst_start + tile_row_bytes;
            out[dst_start..dst_end].copy_from_slice(&tile.data[src_row..src_row + tile_row_bytes]);
        }
    }

    out
}

/// Ensure output parent directory exists.
fn ensure_parent_dir(path: &Path) -> Result<(), String> {
    let Some(parent) = path.parent() else {
        return Ok(());
    };
    fs::create_dir_all(parent).map_err(|e| {
        format!(
            "Failed to create output directory {}: {e}",
            parent.display()
        )
    })
}

/// Encode RGBA8 bytes to PNG file.
fn save_png_rgba8(path: &Path, width: u32, height: u32, data: &[u8]) -> Result<(), String> {
    let file = fs::File::create(path)
        .map_err(|e| format!("Failed to create output file {}: {e}", path.display()))?;
    let writer = BufWriter::new(file);
    let mut encoder = Encoder::new(writer, width, height);
    encoder.set_color(ColorType::Rgba);
    encoder.set_depth(BitDepth::Eight);
    let mut png_writer = encoder
        .write_header()
        .map_err(|e| format!("Failed to write PNG header {}: {e}", path.display()))?;
    png_writer
        .write_image_data(data)
        .map_err(|e| format!("Failed to write PNG data {}: {e}", path.display()))
}

fn main() -> Result<(), String> {
    let (source_dir, output) = parse_args()?;

    let mut tiles: Vec<(TextureId, String, RgbaTexture)> = Vec::new();
    for texture in atlas_texture_order() {
        let base_filename = source_base_filename(*texture);
        let base_path = source_dir.join(base_filename);
        let base = load_rgba8(&base_path)?;

        let final_tile = if let Some(overlay_filename) = source_overlay_filename(*texture) {
            let overlay_path = source_dir.join(overlay_filename);
            let overlay = load_rgba8(&overlay_path)?;
            alpha_blend(&base, &overlay)?
        } else {
            base
        };

        tiles.push((*texture, base_filename.to_string(), final_tile));
    }
    let refs: Vec<(&str, &RgbaTexture)> = tiles
        .iter()
        .map(|(_, filename, texture)| (filename.as_str(), texture))
        .collect();
    let (tile_w, tile_h) = ensure_same_size(&refs)?;
    let ordered_tiles: Vec<RgbaTexture> = tiles.into_iter().map(|(_, _, t)| t).collect();
    let atlas_data = build_atlas_data(&ordered_tiles);
    ensure_parent_dir(&output)?;
    save_png_rgba8(
        &output,
        tile_w * atlas_texture_order().len() as u32,
        tile_h,
        &atlas_data,
    )?;

    println!("Atlas generated: {}", output.display());
    Ok(())
}
