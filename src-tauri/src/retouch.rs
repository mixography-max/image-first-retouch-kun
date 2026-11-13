//! Core photo harmonization engine
//! Strictly preserves Munsell hue, color temperature, and chromatic coordinates (a*, b*)
//! while optimizing luminance (L*) and local contrast via adaptive gamma and CLAHE.

use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;

#[derive(Debug, Clone, serde::Serialize)]
pub struct RetouchStats {
    pub file_name: String,
    pub width: u32,
    pub height: u32,
    pub med_pre: f32,
    pub med_post: f32,
    pub std_pre: f32,
    pub std_post: f32,
    pub gamma: f32,
    pub clip_limit: f32,
}

/// Convert sRGB [0..255] to CIE L*a*b* (L* scaled to 0..255 for OpenCV compatibility)
#[inline(always)]
fn rgb_to_lab(r: u8, g: u8, b: u8) -> (f32, f32, f32) {
    // sRGB to linear RGB
    let to_linear = |c: u8| -> f32 {
        let v = c as f32 / 255.0;
        if v <= 0.04045 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };

    let r_lin = to_linear(r);
    let g_lin = to_linear(g);
    let b_lin = to_linear(b);

    // Linear RGB to CIE XYZ (D65)
    let x = r_lin * 0.4124564 + g_lin * 0.3575761 + b_lin * 0.1804375;
    let y = r_lin * 0.2126729 + g_lin * 0.7151522 + b_lin * 0.0721750;
    let z = r_lin * 0.0193339 + g_lin * 0.1191920 + b_lin * 0.9503041;

    // Normalize with D65 reference white
    let x_n = x / 0.95047;
    let y_n = y / 1.00000;
    let z_n = z / 1.08883;

    let f = |t: f32| -> f32 {
        let delta = 6.0 / 29.0;
        let delta_cb = delta * delta * delta;
        if t > delta_cb {
            t.cbrt()
        } else {
            t / (3.0 * delta * delta) + 4.0 / 29.0
        }
    };

    let fx = f(x_n);
    let fy = f(y_n);
    let fz = f(z_n);

    // Standard L* is 0..100. Scale to 0..255 to match OpenCV L channel in 8-bit space
    let l_star = (116.0 * fy - 16.0).clamp(0.0, 100.0) * (255.0 / 100.0);
    let a_star = 500.0 * (fx - fy);
    let b_star = 200.0 * (fy - fz);

    (l_star, a_star, b_star)
}

/// Convert CIE L*a*b* back to sRGB [0..255]
#[inline(always)]
fn lab_to_rgb(l_star_255: f32, a_star: f32, b_star: f32) -> (u8, u8, u8) {
    let l_star = l_star_255 * (100.0 / 255.0);
    let fy = (l_star + 16.0) / 116.0;
    let fx = a_star / 500.0 + fy;
    let fz = fy - b_star / 200.0;

    let inv_f = |t: f32| -> f32 {
        let delta = 6.0 / 29.0;
        if t > delta {
            t * t * t
        } else {
            3.0 * delta * delta * (t - 4.0 / 29.0)
        }
    };

    let x = 0.95047 * inv_f(fx);
    let y = 1.00000 * inv_f(fy);
    let z = 1.08883 * inv_f(fz);

    // CIE XYZ to Linear RGB
    let r_lin = x * 3.2404542 - y * 1.5371385 - z * 0.4985314;
    let g_lin = -x * 0.9692660 + y * 1.8760108 + z * 0.0415560;
    let b_lin = x * 0.0556434 - y * 0.2040259 + z * 1.0572252;

    // Linear RGB to sRGB
    let to_srgb = |v: f32| -> u8 {
        let clamped = v.clamp(0.0, 1.0);
        let s = if clamped <= 0.0031308 {
            12.92 * clamped
        } else {
            1.055 * clamped.powf(1.0 / 2.4) - 0.055
        };
        (s * 255.0).round().clamp(0.0, 255.0) as u8
    };

    (to_srgb(r_lin), to_srgb(g_lin), to_srgb(b_lin))
}

/// Pure Rust implementation of CLAHE (Contrast Limited Adaptive Histogram Equalization)
fn apply_clahe(l_channel: &[f32], width: usize, height: usize, clip_limit: f32) -> Vec<f32> {
    const GRID_X: usize = 8;
    const GRID_Y: usize = 8;

    let tile_w = width / GRID_X;
    let tile_h = height / GRID_Y;

    // Compute CDF for each tile
    let mut tile_cdfs = vec![vec![0.0f32; 256]; GRID_X * GRID_Y];

    for gy in 0..GRID_Y {
        let y_start = gy * tile_h;
        let y_end = if gy == GRID_Y - 1 { height } else { y_start + tile_h };
        let tile_actual_h = y_end - y_start;

        for gx in 0..GRID_X {
            let x_start = gx * tile_w;
            let x_end = if gx == GRID_X - 1 { width } else { x_start + tile_w };
            let tile_actual_w = x_end - x_start;
            let num_pixels = (tile_actual_w * tile_actual_h) as f32;

            let mut hist = [0.0f32; 256];
            for y in y_start..y_end {
                let row_offset = y * width;
                for x in x_start..x_end {
                    let val = l_channel[row_offset + x].clamp(0.0, 255.0) as usize;
                    hist[val] += 1.0;
                }
            }

            // Clip limit
            let clip_thresh = (clip_limit * num_pixels / 256.0).max(1.0);
            let mut excess = 0.0f32;
            for bin in hist.iter_mut() {
                if *bin > clip_thresh {
                    excess += *bin - clip_thresh;
                    *bin = clip_thresh;
                }
            }

            let bonus = excess / 256.0;
            let mut cum = 0.0f32;
            let tile_idx = gy * GRID_X + gx;
            for i in 0..256 {
                cum += hist[i] + bonus;
                tile_cdfs[tile_idx][i] = (cum / num_pixels * 255.0).clamp(0.0, 255.0);
            }
        }
    }

    // Bilinear interpolation across tiles
    let mut output = vec![0.0f32; width * height];

    for y in 0..height {
        // Tile coordinates in continuous float
        let y_pos = (y as f32 / tile_h as f32) - 0.5;
        let gy1 = y_pos.floor().clamp(0.0, (GRID_Y - 1) as f32) as usize;
        let gy2 = (gy1 + 1).min(GRID_Y - 1);
        let y_weight = (y_pos - gy1 as f32).clamp(0.0, 1.0);

        let row_offset = y * width;

        for x in 0..width {
            let x_pos = (x as f32 / tile_w as f32) - 0.5;
            let gx1 = x_pos.floor().clamp(0.0, (GRID_X - 1) as f32) as usize;
            let gx2 = (gx1 + 1).min(GRID_X - 1);
            let x_weight = (x_pos - gx1 as f32).clamp(0.0, 1.0);

            let val = l_channel[row_offset + x].clamp(0.0, 255.0) as usize;

            let c00 = tile_cdfs[gy1 * GRID_X + gx1][val];
            let c10 = tile_cdfs[gy1 * GRID_X + gx2][val];
            let c01 = tile_cdfs[gy2 * GRID_X + gx1][val];
            let c11 = tile_cdfs[gy2 * GRID_X + gx2][val];

            let top = c00 * (1.0 - x_weight) + c10 * x_weight;
            let bot = c01 * (1.0 - x_weight) + c11 * x_weight;
            output[row_offset + x] = top * (1.0 - y_weight) + bot * y_weight;
        }
    }

    output
}

/// Extract APP1 (Exif) marker bytes from source JPEG file
pub fn extract_exif_bytes(path: &Path) -> Option<Vec<u8>> {
    let mut file = File::open(path).ok()?;
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer).ok()?;

    if buffer.len() < 4 || buffer[0] != 0xFF || buffer[1] != 0xD8 {
        return None; // Not a valid JPEG
    }

    let mut idx = 2;
    while idx + 4 <= buffer.len() {
        if buffer[idx] != 0xFF {
            break;
        }
        let marker = buffer[idx + 1];
        let len = ((buffer[idx + 2] as usize) << 8) | (buffer[idx + 3] as usize);

        if marker == 0xE1 {
            // Found APP1
            let seg_end = idx + 2 + len;
            if seg_end <= buffer.len() {
                return Some(buffer[idx..seg_end].to_vec());
            }
        }
        idx += 2 + len;
    }
    None
}

/// Insert APP1 (Exif) marker into target JPEG file.
/// Also patches the Orientation tag to 1 (Normal) to prevent double-rotation,
/// because `image::open()` already auto-applies the rotation from the original Exif.
pub fn insert_exif_bytes(path: &Path, exif_seg: &[u8]) -> std::io::Result<()> {
    let mut file = File::open(path)?;
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer)?;

    if buffer.len() < 2 || buffer[0] != 0xFF || buffer[1] != 0xD8 {
        return Ok(());
    }

    // Patch Orientation tag to 1 (Normal) in the Exif segment copy
    let mut patched_exif = exif_seg.to_vec();
    patch_exif_orientation(&mut patched_exif);

    // Write: SOI (0xFF, 0xD8) + patched_exif + rest of buffer (skipping original SOI)
    let mut out_file = File::create(path)?;
    out_file.write_all(&[0xFF, 0xD8])?;
    out_file.write_all(&patched_exif)?;
    out_file.write_all(&buffer[2..])?;
    Ok(())
}

/// Patch the Exif Orientation tag (0x0112) to 1 (Normal) in an APP1 segment.
/// This is needed because `image::open()` auto-applies rotation from the original Exif,
/// so we re-attach a modified Exif with Orientation=1 to avoid double rotation in browsers.
fn patch_exif_orientation(exif_seg: &mut [u8]) {
    // APP1 structure: 0xFF 0xE1 <len_hi> <len_lo> "Exif\0\0" <TIFF header>
    // Minimum size check: 2 (marker) + 2 (len) + 6 (Exif\0\0) + 8 (TIFF header) = 18 bytes
    if exif_seg.len() < 18 {
        return;
    }
    // Exif header starts at offset 4 (after FF E1 LEN_HI LEN_LO)
    let exif_header_start = 4;
    // Check "Exif\0\0"
    if &exif_seg[exif_header_start..exif_header_start + 6] != b"Exif\0\0" {
        return;
    }

    // TIFF data starts at offset 10
    let tiff_start = exif_header_start + 6; // = 10

    if exif_seg.len() < tiff_start + 8 {
        return;
    }

    // Determine byte order from TIFF header: "II" = little-endian, "MM" = big-endian
    let is_le = exif_seg[tiff_start] == b'I' && exif_seg[tiff_start + 1] == b'I';
    let is_be = exif_seg[tiff_start] == b'M' && exif_seg[tiff_start + 1] == b'M';
    if !is_le && !is_be {
        return;
    }

    let read_u16 = |seg: &[u8], off: usize| -> u16 {
        if is_le {
            u16::from_le_bytes([seg[off], seg[off + 1]])
        } else {
            u16::from_be_bytes([seg[off], seg[off + 1]])
        }
    };
    let read_u32 = |seg: &[u8], off: usize| -> u32 {
        if is_le {
            u32::from_le_bytes([seg[off], seg[off + 1], seg[off + 2], seg[off + 3]])
        } else {
            u32::from_be_bytes([seg[off], seg[off + 1], seg[off + 2], seg[off + 3]])
        }
    };

    // IFD0 offset (relative to tiff_start) is at tiff_start + 4
    let ifd0_offset = read_u32(exif_seg, tiff_start + 4) as usize;
    let ifd0_abs = tiff_start + ifd0_offset;

    if exif_seg.len() < ifd0_abs + 2 {
        return;
    }

    let num_entries = read_u16(exif_seg, ifd0_abs) as usize;

    for i in 0..num_entries {
        let entry_abs = ifd0_abs + 2 + i * 12;
        if exif_seg.len() < entry_abs + 12 {
            break;
        }
        let tag = read_u16(exif_seg, entry_abs);
        if tag == 0x0112 {
            // Orientation tag found — set value to 1 (Normal)
            // Value offset is at entry_abs + 8, type is SHORT (2 bytes)
            if is_le {
                exif_seg[entry_abs + 8] = 1;
                exif_seg[entry_abs + 9] = 0;
            } else {
                exif_seg[entry_abs + 8] = 0;
                exif_seg[entry_abs + 9] = 1;
            }
            // Clear remaining 2 bytes of the 4-byte value field
            exif_seg[entry_abs + 10] = 0;
            exif_seg[entry_abs + 11] = 0;
            return;
        }
    }
}

/// Main entry point: harmonizes an image file
pub fn harmonize_file(
    in_path: &Path,
    out_path: &Path,
    target_l: f32,
) -> Result<RetouchStats, String> {
    let img = image::open(in_path).map_err(|e| format!("Failed to open image: {}", e))?;
    let rgb_img = img.to_rgb8();
    let (width, height) = rgb_img.dimensions();
    let total_pixels = (width * height) as usize;

    let mut l_channel = Vec::with_capacity(total_pixels);
    let mut a_channel = Vec::with_capacity(total_pixels);
    let mut b_channel = Vec::with_capacity(total_pixels);

    for pixel in rgb_img.pixels() {
        let (l, a, b) = rgb_to_lab(pixel[0], pixel[1], pixel[2]);
        l_channel.push(l);
        a_channel.push(a);
        b_channel.push(b);
    }

    // Soil / Subject mask: exclude overexposed highlights (> 240)
    let mut valid_samples: Vec<f32> = l_channel
        .iter()
        .copied()
        .filter(|&v| v < 240.0)
        .collect();

    if valid_samples.len() < total_pixels / 7 {
        valid_samples = l_channel.clone();
    }

    valid_samples.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let med_pre = valid_samples[valid_samples.len() / 2];

    let mean_pre = valid_samples.iter().sum::<f32>() / valid_samples.len() as f32;
    let var_pre = valid_samples
        .iter()
        .map(|v| (v - mean_pre) * (v - mean_pre))
        .sum::<f32>()
        / valid_samples.len() as f32;
    let std_pre = var_pre.sqrt();

    // Adaptive gamma
    let ratio = (med_pre / 255.0).clamp(0.15, 0.85);
    let target_ratio = (target_l / 255.0).clamp(0.20, 0.80);
    let gamma = (target_ratio.ln() / ratio.ln()).clamp(0.65, 1.35);

    // Apply gamma
    let mut l_gamma = Vec::with_capacity(total_pixels);
    for &l in &l_channel {
        let norm = (l / 255.0).max(1e-6);
        l_gamma.push(norm.powf(gamma) * 255.0);
    }

    // Adaptive CLAHE clip limit
    let clip_limit = if std_pre < 45.0 {
        2.0
    } else if std_pre > 65.0 {
        1.2
    } else {
        1.5
    };

    let l_clahe = apply_clahe(&l_gamma, width as usize, height as usize, clip_limit);

    // Blend: 75% CLAHE + 25% smooth gamma curve, with soft roll-off
    let mut l_final = Vec::with_capacity(total_pixels);
    for i in 0..total_pixels {
        let mut val = 0.75 * l_clahe[i] + 0.25 * l_gamma[i];

        // Soft highlight roll-off above 225
        if val > 225.0 {
            val = 225.0 + 30.0 * ((val - 225.0) / 30.0).tanh();
        }
        // Soft shadow roll-off below 12
        if val < 12.0 {
            val = 12.0 * (val / 12.0 * std::f32::consts::FRAC_PI_2).sin();
        }

        l_final.push(val.clamp(0.0, 255.0));
    }

    // Compute post statistics
    let mut sorted_post = l_final.clone();
    sorted_post.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let med_post = sorted_post[sorted_post.len() / 2];

    let mean_post = l_final.iter().sum::<f32>() / total_pixels as f32;
    let var_post = l_final
        .iter()
        .map(|v| (v - mean_post) * (v - mean_post))
        .sum::<f32>()
        / total_pixels as f32;
    let std_post = var_post.sqrt();

    // Reconstruct RGB with UNTOUCHED a_channel and b_channel (Zero Color Shift!)
    let mut out_img = image::RgbImage::new(width, height);
    for y in 0..height {
        let row_offset = (y * width) as usize;
        for x in 0..width {
            let idx = row_offset + x as usize;
            let (r, g, b) = lab_to_rgb(l_final[idx], a_channel[idx], b_channel[idx]);
            out_img.put_pixel(x, y, image::Rgb([r, g, b]));
        }
    }

    // Create parent directories if needed
    if let Some(parent) = out_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create output directory: {}", e))?;
    }

    // Save image with high quality
    out_img
        .save(out_path)
        .map_err(|e| format!("Failed to save image: {}", e))?;

    // Copy Exif APP1 segment if JPEG
    if let Some(exif_bytes) = extract_exif_bytes(in_path) {
        let _ = insert_exif_bytes(out_path, &exif_bytes);
    }

    Ok(RetouchStats {
        file_name: in_path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string(),
        width,
        height,
        med_pre,
        med_post,
        std_pre,
        std_post,
        gamma,
        clip_limit,
    })
}

