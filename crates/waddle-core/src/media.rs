//! What both targets share about pictures: the placeholder, the size on the
//! page, and the file extension for a media type.
//!
//! Author: David M. Anderson
//! Built with AI assistance (Claude, Anthropic)

/// A 16 by 12 light grey PNG, written where a picture arrives without
/// bytes. A frame with no image part reads back as nothing in either
/// reader, so the placeholder is a real picture, and the readers return a
/// picture node for it as they do for every embedded image.
pub const PLACEHOLDER_PNG: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x10, 0x00, 0x00, 0x00, 0x0c, 0x08, 0x02, 0x00, 0x00, 0x00, 0xe4, 0x85, 0xaa,
    0xd6, 0x00, 0x00, 0x00, 0x13, 0x49, 0x44, 0x41, 0x54, 0x78, 0xda, 0x63, 0xb8, 0x40, 0x22, 0x60,
    0x18, 0xd5, 0x30, 0xaa, 0x01, 0x3b, 0x00, 0x00, 0x99, 0xc3, 0xd4, 0x10, 0x65, 0x6a, 0xad, 0xca,
    0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
];

/// The placeholder's size on the page, in inches.
pub const PLACEHOLDER_SIZE_IN: (f64, f64) = (2.0, 1.5);

/// The text width inside the stock page, in inches.
const TEXT_WIDTH_IN: f64 = 6.5;

/// A picture's size on the page: its pixel size at 96 dpi, scaled down to
/// the text width when wider.
pub fn fit(width_px: u32, height_px: u32) -> (f64, f64) {
    if width_px == 0 || height_px == 0 {
        return PLACEHOLDER_SIZE_IN;
    }
    let w = f64::from(width_px) / 96.0;
    let h = f64::from(height_px) / 96.0;
    if w > TEXT_WIDTH_IN {
        (TEXT_WIDTH_IN, h * TEXT_WIDTH_IN / w)
    } else {
        (w, h)
    }
}

/// The file extension for a media type. The DOCX reader names the media
/// type from the extension, so this is the one it will read back.
pub fn extension(media_type: &str) -> &'static str {
    match media_type {
        "image/png" => "png",
        "image/jpeg" => "jpg",
        "image/gif" => "gif",
        "image/bmp" => "bmp",
        "image/tiff" => "tif",
        "image/webp" => "webp",
        "image/svg+xml" => "svg",
        _ => "bin",
    }
}
