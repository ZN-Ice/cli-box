use crate::error::{AppError, Result};
use image::{GenericImageView, Rgba};
use serde::Serialize;

/// Result of comparing two screenshots.
#[derive(Debug, Serialize)]
pub struct DiffResult {
    pub total_pixels: u32,
    pub different_pixels: u32,
    pub diff_percentage: f64,
    pub diff_image: Option<Vec<u8>>,
}

/// Compare two PNG images pixel-by-pixel.
pub fn diff_images(img_a: &[u8], img_b: &[u8], threshold: u8) -> Result<DiffResult> {
    let a = image::load_from_memory(img_a)
        .map_err(|e| AppError::Screenshot(format!("Failed to load image A: {e}")))?;
    let b = image::load_from_memory(img_b)
        .map_err(|e| AppError::Screenshot(format!("Failed to load image B: {e}")))?;

    if a.dimensions() != b.dimensions() {
        return Err(AppError::BadRequest(format!(
            "Image dimensions differ: {:?} vs {:?}",
            a.dimensions(),
            b.dimensions()
        )));
    }

    let (width, height) = a.dimensions();
    let total = width * height;
    let mut different: u32 = 0;
    let mut diff_buf = image::RgbaImage::new(width, height);

    for y in 0..height {
        for x in 0..width {
            let pa: Rgba<u8> = a.get_pixel(x, y);
            let pb: Rgba<u8> = b.get_pixel(x, y);
            let dr = pa[0].abs_diff(pb[0]);
            let dg = pa[1].abs_diff(pb[1]);
            let db = pa[2].abs_diff(pb[2]);
            if dr > threshold || dg > threshold || db > threshold {
                different += 1;
                diff_buf.put_pixel(x, y, Rgba([255, 0, 0, 255]));
            } else {
                diff_buf.put_pixel(x, y, pa);
            }
        }
    }

    let mut diff_png = Vec::new();
    let encoder = image::codecs::png::PngEncoder::new(&mut diff_png);
    use image::ImageEncoder;
    encoder
        .write_image(
            diff_buf.as_raw(),
            width,
            height,
            image::ExtendedColorType::Rgba8,
        )
        .map_err(|e| AppError::Screenshot(format!("Failed to encode diff: {e}")))?;

    Ok(DiffResult {
        total_pixels: total,
        different_pixels: different,
        diff_percentage: (different as f64 / total as f64) * 100.0,
        diff_image: Some(diff_png),
    })
}
