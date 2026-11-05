//! What both targets share about pictures: the placeholder, the size on the
//! page, and the file extension for a media type.
//!
//! Author: David M. Anderson
//! Built with AI assistance (Claude, Anthropic)

use docling_core::PictureImage;

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

/// A picture's size on the page: the size its source document displayed it
/// at where the reader recorded one, else its pixel size at 96 dpi, scaled
/// down to the text width when wider.
pub fn fit(image: &PictureImage) -> (f64, f64) {
    let (w, h) = match image.display_size {
        Some([w, h]) => (f64::from(w), f64::from(h)),
        None => (
            f64::from(image.width) / 96.0,
            f64::from(image.height) / 96.0,
        ),
    };
    if w <= 0.0 || h <= 0.0 {
        return PLACEHOLDER_SIZE_IN;
    }
    if w > TEXT_WIDTH_IN {
        (TEXT_WIDTH_IN, h * TEXT_WIDTH_IN / w)
    } else {
        (w, h)
    }
}

/// An SVG drawn as a PNG at twice 96 dpi for its size on the page, in the
/// system's fonts, loaded once. `None` when the SVG does not parse or draws
/// at no size.
#[cfg(feature = "svg-fallback")]
pub fn svg_to_png(svg: &[u8], (width_in, height_in): (f64, f64)) -> Option<Vec<u8>> {
    use resvg::{tiny_skia, usvg};
    use std::sync::{Arc, OnceLock};

    static FONTS: OnceLock<(Arc<usvg::fontdb::Database>, String)> = OnceLock::new();
    let (fonts, serif) = FONTS.get_or_init(system_fonts);
    let options = usvg::Options {
        fontdb: Arc::clone(fonts),
        font_family: serif.clone(),
        ..usvg::Options::default()
    };
    let tree = usvg::Tree::from_data(svg, &options).ok()?;
    let width = (width_in * 192.0).round() as u32;
    let height = (height_in * 192.0).round() as u32;
    let mut pixmap = tiny_skia::Pixmap::new(width.max(1), height.max(1))?;
    let size = tree.size();
    let transform = tiny_skia::Transform::from_scale(
        pixmap.width() as f32 / size.width(),
        pixmap.height() as f32 / size.height(),
    );
    resvg::render(&tree, transform, &mut pixmap.as_mut());
    pixmap.encode_png().ok()
}

/// The system's fonts, with each generic family pointed at a face the
/// system has: fontdb names Arial and Times New Roman whether or not they are
/// installed, and an SVG's `sans-serif` then draws nothing. The face for an
/// SVG that names no family is the serif one, as a browser's is.
#[cfg(feature = "svg-fallback")]
fn system_fonts() -> (std::sync::Arc<resvg::usvg::fontdb::Database>, String) {
    use resvg::usvg::fontdb::{Database, Family, Query};

    let mut db = Database::new();
    db.load_system_fonts();
    let first = |db: &Database, names: &[&str]| {
        names
            .iter()
            .find(|name| {
                db.query(&Query {
                    families: &[Family::Name(name)],
                    ..Query::default()
                })
                .is_some()
            })
            .map(|name| (*name).to_owned())
    };
    let sans = first(
        &db,
        &[
            "Arial",
            "Liberation Sans",
            "Helvetica",
            "DejaVu Sans",
            "Noto Sans",
        ],
    );
    let serif = first(
        &db,
        &[
            "Times New Roman",
            "Liberation Serif",
            "DejaVu Serif",
            "Noto Serif",
        ],
    );
    let mono = first(
        &db,
        &[
            "Courier New",
            "Liberation Mono",
            "DejaVu Sans Mono",
            "Noto Sans Mono",
        ],
    );
    if let Some(name) = &sans {
        db.set_sans_serif_family(name);
    }
    if let Some(name) = &serif {
        db.set_serif_family(name);
    }
    if let Some(name) = &mono {
        db.set_monospace_family(name);
    }
    let default = serif
        .or(sans)
        .unwrap_or_else(|| "Times New Roman".to_owned());
    (std::sync::Arc::new(db), default)
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
