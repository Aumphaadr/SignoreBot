//! Export a saved reward image as the three PNG sizes used by Twitch.
use base64::Engine as _;
use image::{imageops::FilterType, DynamicImage, ImageFormat, ImageReader, RgbaImage};
use std::{
    io::Cursor,
    path::{Path, PathBuf},
};

pub const SIZES: [u32; 3] = [28, 56, 112];

fn decode(data_url: &str) -> Result<DynamicImage, String> {
    let (mime, encoded) = data_url
        .strip_prefix("data:")
        .and_then(|s| s.split_once(";base64,"))
        .ok_or("Некорректная картинка награды")?;
    if !["image/png", "image/jpeg", "image/webp", "image/gif"].contains(&mime)
        || encoded.len() > 1_400_000
    {
        return Err("Неподдерживаемая картинка награды".into());
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|e| e.to_string())?;
    let format = image::guess_format(&bytes).map_err(|e| e.to_string())?;
    if format.to_mime_type() != mime {
        return Err("Формат картинки не совпадает с содержимым".into());
    }
    let mut reader = ImageReader::with_format(Cursor::new(bytes), format);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(4096);
    limits.max_image_height = Some(4096);
    limits.max_alloc = Some(64 * 1024 * 1024);
    reader.limits(limits);
    reader
        .decode()
        .map_err(|e| format!("Не удалось прочитать картинку: {e}"))
}

fn png_sizes(data_url: &str) -> Result<Vec<(u32, Vec<u8>)>, String> {
    let original = decode(data_url)?;
    SIZES
        .into_iter()
        .map(|size| {
            let resized = original.resize(size, size, FilterType::Lanczos3).to_rgba8();
            let mut square = RgbaImage::new(size, size);
            image::imageops::overlay(
                &mut square,
                &resized,
                ((size - resized.width()) / 2).into(),
                ((size - resized.height()) / 2).into(),
            );
            let mut png = Cursor::new(Vec::new());
            DynamicImage::ImageRgba8(square)
                .write_to(&mut png, ImageFormat::Png)
                .map_err(|e| e.to_string())?;
            Ok((size, png.into_inner()))
        })
        .collect()
}

/// Each export gets its own folder so an earlier icon set is never overwritten.
pub fn export(data_url: &str, parent: &Path) -> Result<PathBuf, String> {
    if !parent.is_dir() {
        return Err("Выберите существующую папку для иконок".into());
    }
    let images = png_sizes(data_url)?;
    let folder = loop {
        let id = uuid::Uuid::new_v4().simple().to_string();
        let folder = parent.join(format!("reward-icons-{}", &id[..8]));
        match std::fs::create_dir(&folder) {
            Ok(()) => break folder,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e.to_string()),
        }
    };
    for (size, png) in images {
        if let Err(e) = std::fs::write(folder.join(format!("reward-icon-{size}x{size}.png")), png) {
            let _ = std::fs::remove_dir_all(&folder);
            return Err(e.to_string());
        }
    }
    Ok(folder)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;
    fn data_url(image: RgbaImage) -> String {
        let mut png = Cursor::new(Vec::new());
        DynamicImage::ImageRgba8(image)
            .write_to(&mut png, ImageFormat::Png)
            .unwrap();
        format!(
            "data:image/png;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(png.into_inner())
        )
    }
    #[test]
    fn exports_three_real_png_files_with_exact_dimensions_and_transparency() {
        let mut image = RgbaImage::new(112, 112);
        for y in 28..84 {
            for x in 28..84 {
                image.put_pixel(x, y, Rgba([30, 180, 90, 255]));
            }
        }
        let parent = tempfile::tempdir().unwrap();
        let folder = export(&data_url(image), parent.path()).unwrap();
        assert_eq!(std::fs::read_dir(&folder).unwrap().count(), 3);
        for size in SIZES {
            let bytes =
                std::fs::read(folder.join(format!("reward-icon-{size}x{size}.png"))).unwrap();
            assert_eq!(image::guess_format(&bytes).unwrap(), ImageFormat::Png);
            let decoded = image::load_from_memory(&bytes).unwrap().to_rgba8();
            assert_eq!(decoded.dimensions(), (size, size));
            assert_eq!(decoded.get_pixel(0, 0)[3], 0);
            assert_eq!(
                decoded.get_pixel(size / 2, size / 2),
                &Rgba([30, 180, 90, 255])
            );
        }
    }
    #[test]
    fn non_square_artwork_is_fitted_without_stretching() {
        let url = data_url(RgbaImage::from_pixel(56, 112, Rgba([255, 40, 50, 255])));
        for (size, png) in png_sizes(&url).unwrap() {
            let image = image::load_from_memory(&png).unwrap().to_rgba8();
            assert_eq!(image.dimensions(), (size, size));
            assert_eq!(image.get_pixel(0, size / 2)[3], 0);
            assert_eq!(
                image.get_pixel(size / 2, size / 2),
                &Rgba([255, 40, 50, 255])
            );
        }
    }
    #[test]
    fn repeated_exports_preserve_existing_files() {
        let parent = tempfile::tempdir().unwrap();
        let url = data_url(RgbaImage::from_pixel(112, 112, Rgba([255, 255, 255, 255])));
        let first = export(&url, parent.path()).unwrap();
        let file = first.join("reward-icon-28x28.png");
        let before = std::fs::read(&file).unwrap();
        let second = export(&url, parent.path()).unwrap();
        assert_ne!(first, second);
        assert_eq!(std::fs::read(file).unwrap(), before);
    }
    #[test]
    fn invalid_data_leaves_no_partial_export() {
        let parent = tempfile::tempdir().unwrap();
        for url in [
            "https://example.test/a.png",
            "data:text/html;base64,aGVsbG8=",
            "data:image/png;base64,aGVsbG8=",
            "data:image/png;base64,@@@@",
        ] {
            assert!(export(url, parent.path()).is_err());
        }
        assert_eq!(std::fs::read_dir(parent.path()).unwrap().count(), 0);
    }
}
