use image::{GenericImageView, RgbaImage};
use sandbox_core::diff::{diff_image, diff_images, DiffOptions};

/// Create an in-memory PNG from a solid-color RgbaImage
fn make_solid_png(width: u32, height: u32, r: u8, g: u8, b: u8, a: u8) -> Vec<u8> {
    let mut img = RgbaImage::new(width, height);
    for y in 0..height {
        for x in 0..width {
            img.put_pixel(x, y, image::Rgba([r, g, b, a]));
        }
    }
    let mut cursor = std::io::Cursor::new(Vec::new());
    img.write_to(&mut cursor, image::ImageFormat::Png).unwrap();
    cursor.into_inner()
}

// ── diff_images ──────────────────────────────────────────────

#[test]
fn identical_images_are_detected() {
    let png = make_solid_png(10, 10, 255, 0, 0, 255);
    let result = diff_images(&png, &png, &DiffOptions::default()).unwrap();
    assert!(result.identical);
    assert_eq!(result.diff_percentage, 0.0);
    assert_eq!(result.changed_pixels, 0);
    assert_eq!(result.total_pixels, 100);
}

#[test]
fn completely_different_images_detected() {
    let red = make_solid_png(10, 10, 255, 0, 0, 255);
    let green = make_solid_png(10, 10, 0, 255, 0, 255);
    let result = diff_images(&red, &green, &DiffOptions::default()).unwrap();
    assert!(!result.identical);
    assert_eq!(result.diff_percentage, 100.0);
    assert_eq!(result.changed_pixels, 100);
}

#[test]
fn size_mismatch_returns_full_diff() {
    let small = make_solid_png(10, 10, 255, 0, 0, 255);
    let large = make_solid_png(20, 20, 255, 0, 0, 255);
    let result = diff_images(&small, &large, &DiffOptions::default()).unwrap();
    assert!(!result.identical);
    assert_eq!(result.diff_percentage, 100.0);
}

#[test]
fn tolerance_respects_threshold() {
    let mut img1 = RgbaImage::new(10, 10);
    let mut img2 = RgbaImage::new(10, 10);
    for y in 0..10 {
        for x in 0..10 {
            img1.put_pixel(x, y, image::Rgba([100, 100, 100, 255]));
            img2.put_pixel(x, y, image::Rgba([105, 105, 105, 255])); // diff = 5 per channel
        }
    }
    let mut cursor = std::io::Cursor::new(Vec::new());
    img1.write_to(&mut cursor, image::ImageFormat::Png).unwrap();
    let png1 = cursor.into_inner();
    let mut cursor = std::io::Cursor::new(Vec::new());
    img2.write_to(&mut cursor, image::ImageFormat::Png).unwrap();
    let png2 = cursor.into_inner();

    // threshold=1: difference of 5 is detected → all pixels differ
    let opts = DiffOptions {
        threshold: 1,
        ..Default::default()
    };
    let result = diff_images(&png1, &png2, &opts).unwrap();
    assert!(!result.identical);
    assert_eq!(result.changed_pixels, 100);

    // threshold=10: difference of 5 is within tolerance → identical
    let opts = DiffOptions {
        threshold: 10,
        ..Default::default()
    };
    let result = diff_images(&png1, &png2, &opts).unwrap();
    assert!(result.identical);
    assert_eq!(result.changed_pixels, 0);
}

#[test]
fn max_diff_percentage_allows_minor_changes() {
    let red = make_solid_png(10, 10, 255, 0, 0, 255);
    let green = make_solid_png(10, 10, 0, 255, 0, 255);

    // max_diff_percentage = 100 means even fully different is "identical"
    let opts = DiffOptions {
        max_diff_percentage: 100.0,
        ..Default::default()
    };
    let result = diff_images(&red, &green, &opts).unwrap();
    assert!(result.identical);

    // max_diff_percentage = 0 means any diff fails
    let opts = DiffOptions {
        max_diff_percentage: 0.0,
        ..Default::default()
    };
    let result = diff_images(&red, &green, &opts).unwrap();
    assert!(!result.identical);
}

#[test]
fn ignore_border_excludes_edge_pixels() {
    // 10x10 red image compared with green image — but border is ignored
    let red = make_solid_png(10, 10, 255, 0, 0, 255);
    let green = make_solid_png(10, 10, 0, 255, 0, 255);

    let opts = DiffOptions {
        ignore_border: 2,
        ..Default::default()
    };
    let result = diff_images(&red, &green, &opts).unwrap();
    // Border of 2px on each side reduces from 10x10=100 to 6x6=36 pixels checked
    assert_eq!(result.total_pixels, 100);
    assert_eq!(result.changed_pixels, 36); // only inner 6x6
}

#[test]
fn partial_diff_calculated_correctly() {
    let mut img1 = RgbaImage::new(10, 10);
    let mut img2 = RgbaImage::new(10, 10);
    for y in 0..10 {
        for x in 0..10 {
            if x < 5 {
                img1.put_pixel(x, y, image::Rgba([255, 0, 0, 255]));
                img2.put_pixel(x, y, image::Rgba([255, 0, 0, 255]));
            } else {
                img1.put_pixel(x, y, image::Rgba([255, 0, 0, 255]));
                img2.put_pixel(x, y, image::Rgba([0, 255, 0, 255]));
            }
        }
    }
    let mut cursor = std::io::Cursor::new(Vec::new());
    img1.write_to(&mut cursor, image::ImageFormat::Png).unwrap();
    let png1 = cursor.into_inner();
    let mut cursor = std::io::Cursor::new(Vec::new());
    img2.write_to(&mut cursor, image::ImageFormat::Png).unwrap();
    let png2 = cursor.into_inner();

    let result = diff_images(&png1, &png2, &DiffOptions::default()).unwrap();
    assert!(!result.identical);
    assert_eq!(result.changed_pixels, 50); // half the image
    assert!((result.diff_percentage - 50.0).abs() < 0.01);
}

// ── diff_image (visual diff) ─────────────────────────────────

#[test]
fn diff_image_generates_valid_png() {
    let red = make_solid_png(10, 10, 255, 0, 0, 255);
    let green = make_solid_png(10, 10, 0, 255, 0, 255);
    let diff_png = diff_image(&red, &green, &DiffOptions::default()).unwrap();
    assert!(!diff_png.is_empty());
    // Should be parseable as PNG
    let img = image::load_from_memory(&diff_png).unwrap();
    assert_eq!(img.dimensions(), (10, 10));
}

#[test]
fn diff_image_identical_produces_dimmed_original() {
    let red = make_solid_png(10, 10, 255, 0, 0, 255);
    let diff_png = diff_image(&red, &red, &DiffOptions::default()).unwrap();
    let img = image::load_from_memory(&diff_png).unwrap();
    // No red pixels in the diff (no changes highlighted)
    for y in 0..10 {
        for x in 0..10 {
            let p = img.get_pixel(x, y);
            // All pixels should be non-highlight (not pure red)
            assert!(p.0[0] < 255 || p.0[1] > 0);
        }
    }
}

// ── Default options ──────────────────────────────────────────

#[test]
fn default_diff_options() {
    let opts = DiffOptions::default();
    assert_eq!(opts.threshold, 10);
    assert_eq!(opts.max_diff_percentage, 0.0);
    assert_eq!(opts.ignore_border, 0);
}
