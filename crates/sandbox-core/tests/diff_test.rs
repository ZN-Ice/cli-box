use sandbox_core::diff::diff_images;

fn encode_png(img: &image::RgbaImage) -> Vec<u8> {
    let mut buf = Vec::new();
    let encoder = image::codecs::png::PngEncoder::new(&mut buf);
    use image::ImageEncoder;
    encoder
        .write_image(
            img.as_raw(),
            img.width(),
            img.height(),
            image::ExtendedColorType::Rgba8,
        )
        .unwrap();
    buf
}

#[test]
fn test_identical_images_return_zero_diff() {
    let img = image::RgbaImage::from_pixel(2, 2, image::Rgba([255, 0, 0, 255]));
    let buf = encode_png(&img);

    let result = diff_images(&buf, &buf, 10).unwrap();
    assert_eq!(result.different_pixels, 0);
    assert_eq!(result.diff_percentage, 0.0);
}

#[test]
fn test_different_images_detect_changes() {
    let red = image::RgbaImage::from_pixel(2, 2, image::Rgba([255, 0, 0, 255]));
    let blue = image::RgbaImage::from_pixel(2, 2, image::Rgba([0, 0, 255, 255]));

    let result = diff_images(&encode_png(&red), &encode_png(&blue), 10).unwrap();
    assert_eq!(result.different_pixels, 4);
    assert!((result.diff_percentage - 100.0).abs() < 0.01);
}

#[test]
fn test_threshold_sensitivity() {
    let a = image::RgbaImage::from_pixel(1, 1, image::Rgba([100, 100, 100, 255]));
    let b = image::RgbaImage::from_pixel(1, 1, image::Rgba([105, 100, 100, 255]));

    // Threshold 10: difference of 5 should NOT be detected
    let result = diff_images(&encode_png(&a), &encode_png(&b), 10).unwrap();
    assert_eq!(result.different_pixels, 0);

    // Threshold 3: difference of 5 SHOULD be detected
    let result = diff_images(&encode_png(&a), &encode_png(&b), 3).unwrap();
    assert_eq!(result.different_pixels, 1);
}

#[test]
fn test_diff_image_is_generated() {
    let a = image::RgbaImage::from_pixel(2, 2, image::Rgba([255, 0, 0, 255]));
    let b = image::RgbaImage::from_pixel(2, 2, image::Rgba([0, 0, 255, 255]));

    let result = diff_images(&encode_png(&a), &encode_png(&b), 10).unwrap();
    assert!(result.diff_image.is_some());
    let diff_img = result.diff_image.unwrap();
    assert!(!diff_img.is_empty());
    // Verify it's a valid PNG
    let loaded = image::load_from_memory(&diff_img);
    assert!(loaded.is_ok());
}
