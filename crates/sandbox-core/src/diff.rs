use crate::error::{AppError, Result};
use image::{GenericImageView, Pixel, RgbaImage};
use serde::{Deserialize, Serialize};

/// Result of comparing two screenshots
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffResult {
    /// Whether the images are considered identical
    pub identical: bool,
    /// Percentage of pixels that differ (0.0 - 100.0)
    pub diff_percentage: f64,
    /// Total number of pixels compared
    pub total_pixels: u64,
    /// Number of pixels that differ
    pub changed_pixels: u64,
}

/// Options for screenshot comparison
#[derive(Debug, Clone)]
pub struct DiffOptions {
    /// Pixel difference threshold (0-255). Pixels with channel differences
    /// below this are considered identical. Default: 10.
    pub threshold: u8,
    /// Maximum diff percentage to consider images identical. Default: 0.0 (any diff = not identical).
    pub max_diff_percentage: f64,
    /// Ignore pixels within this border (in pixels). Default: 0.
    pub ignore_border: u32,
}

impl Default for DiffOptions {
    fn default() -> Self {
        Self {
            threshold: 10,
            max_diff_percentage: 0.0,
            ignore_border: 0,
        }
    }
}

/// Compare two PNG images and return a diff result
pub fn diff_images(expected: &[u8], actual: &[u8], options: &DiffOptions) -> Result<DiffResult> {
    let expected_img = image::load_from_memory(expected)
        .map_err(|e| AppError::Screenshot(format!("Failed to load expected image: {}", e)))?;
    let actual_img = image::load_from_memory(actual)
        .map_err(|e| AppError::Screenshot(format!("Failed to load actual image: {}", e)))?;

    let (ew, eh) = expected_img.dimensions();
    let (aw, ah) = actual_img.dimensions();

    if ew != aw || eh != ah {
        return Ok(DiffResult {
            identical: false,
            diff_percentage: 100.0,
            total_pixels: (ew as u64) * (eh as u64),
            changed_pixels: (ew as u64) * (eh as u64),
        });
    }

    let total_pixels = (ew as u64) * (eh as u64);
    let mut changed_pixels: u64 = 0;
    let threshold = options.threshold;

    let ib = options.ignore_border;
    for y in ib..(eh - ib) {
        for x in ib..(ew - ib) {
            let ep = expected_img.get_pixel(x, y);
            let ap = actual_img.get_pixel(x, y);
            let channels = ep.channels();
            let ach = ap.channels();

            let mut diff = false;
            for c in 0..channels.len() {
                let d = channels[c].abs_diff(ach[c]);
                if d > threshold {
                    diff = true;
                    break;
                }
            }
            if diff {
                changed_pixels += 1;
            }
        }
    }

    let diff_percentage = if total_pixels > 0 {
        (changed_pixels as f64) / (total_pixels as f64) * 100.0
    } else {
        0.0
    };

    Ok(DiffResult {
        identical: diff_percentage <= options.max_diff_percentage,
        diff_percentage,
        total_pixels,
        changed_pixels,
    })
}

/// Generate a diff image highlighting changed pixels in red
pub fn diff_image(expected: &[u8], actual: &[u8], options: &DiffOptions) -> Result<Vec<u8>> {
    let expected_img = image::load_from_memory(expected)
        .map_err(|e| AppError::Screenshot(format!("Failed to load expected image: {}", e)))?;
    let actual_img = image::load_from_memory(actual)
        .map_err(|e| AppError::Screenshot(format!("Failed to load actual image: {}", e)))?;

    let (ew, eh) = expected_img.dimensions();
    let (aw, ah) = actual_img.dimensions();

    let width = ew.min(aw);
    let height = eh.min(ah);

    let mut result = RgbaImage::new(width, height);

    for y in 0..height {
        for x in 0..width {
            let ep = expected_img.get_pixel(x, y);
            let ap = actual_img.get_pixel(x, y);
            let channels = ep.channels();
            let ach = ap.channels();

            let mut is_diff = false;
            for c in 0..channels.len().min(ach.len()) {
                let d = channels[c].abs_diff(ach[c]);
                if d > options.threshold {
                    is_diff = true;
                    break;
                }
            }

            if is_diff {
                // Highlight changed pixels in semi-transparent red
                let alpha = 180u8;
                result.put_pixel(x, y, image::Rgba([255, 0, 0, alpha]));
            } else {
                // Show actual image with reduced opacity for context
                let p = ap;
                let data = p.channels();
                result.put_pixel(
                    x,
                    y,
                    image::Rgba([
                        (data[0] as u16 * 3 / 4) as u8,
                        (data[1] as u16 * 3 / 4) as u8,
                        (data[2] as u16 * 3 / 4) as u8,
                        255,
                    ]),
                );
            }
        }
    }

    let mut buf = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(result)
        .write_to(&mut buf, image::ImageFormat::Png)
        .map_err(|e| AppError::Screenshot(format!("Failed to encode diff image: {}", e)))?;
    Ok(buf.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_identical_images() {
        // Create two identical 10x10 red images
        let mut img = RgbaImage::new(10, 10);
        for y in 0..10 {
            for x in 0..10 {
                img.put_pixel(x, y, image::Rgba([255, 0, 0, 255]));
            }
        }
        let mut buf1 = std::io::Cursor::new(Vec::new());
        img.write_to(&mut buf1, image::ImageFormat::Png).unwrap();
        let png1 = buf1.into_inner();

        let result = diff_images(&png1, &png1, &DiffOptions::default()).unwrap();
        assert!(result.identical);
        assert_eq!(result.diff_percentage, 0.0);
        assert_eq!(result.changed_pixels, 0);
    }

    #[test]
    fn test_different_images() {
        let mut img1 = RgbaImage::new(10, 10);
        let mut img2 = RgbaImage::new(10, 10);
        for y in 0..10 {
            for x in 0..10 {
                img1.put_pixel(x, y, image::Rgba([255, 0, 0, 255]));
                img2.put_pixel(x, y, image::Rgba([0, 255, 0, 255]));
            }
        }
        let mut buf1 = std::io::Cursor::new(Vec::new());
        let mut buf2 = std::io::Cursor::new(Vec::new());
        img1.write_to(&mut buf1, image::ImageFormat::Png).unwrap();
        img2.write_to(&mut buf2, image::ImageFormat::Png).unwrap();

        let result = diff_images(
            &buf1.into_inner(),
            &buf2.into_inner(),
            &DiffOptions::default(),
        )
        .unwrap();
        assert!(!result.identical);
        assert_eq!(result.changed_pixels, 100);
        assert_eq!(result.diff_percentage, 100.0);
    }

    #[test]
    fn test_size_mismatch() {
        let mut img1 = RgbaImage::new(10, 10);
        let mut img2 = RgbaImage::new(20, 20);
        for y in 0..10 {
            for x in 0..10 {
                img1.put_pixel(x, y, image::Rgba([255, 0, 0, 255]));
            }
        }
        for y in 0..20 {
            for x in 0..20 {
                img2.put_pixel(x, y, image::Rgba([0, 255, 0, 255]));
            }
        }
        let mut buf1 = std::io::Cursor::new(Vec::new());
        let mut buf2 = std::io::Cursor::new(Vec::new());
        img1.write_to(&mut buf1, image::ImageFormat::Png).unwrap();
        img2.write_to(&mut buf2, image::ImageFormat::Png).unwrap();

        let result = diff_images(
            &buf1.into_inner(),
            &buf2.into_inner(),
            &DiffOptions::default(),
        )
        .unwrap();
        assert!(!result.identical);
        assert_eq!(result.diff_percentage, 100.0);
    }
}
