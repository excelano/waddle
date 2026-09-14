//! Picture bytes, which docling's DocLang and JSON readers leave behind.
//!
//! Both readers hand back every picture with `image: None`; the DocLang
//! reader says in its own doc comment that the `<src>` payload is not
//! re-imported, and the JSON reader hard-codes it. This module reads the
//! sources out of the input itself, in document order, and pairs them by
//! ordinal with the picture nodes the reader produced in the same order.
//! The pairing holds because both walks are the document's order; a count
//! that differs is reported and nothing is paired. This is a workaround for
//! an upstream gap and is deleted the day the readers read their sources.
//!
//! Author: David M. Anderson
//! Built with AI assistance (Claude, Anthropic)

use std::io::{Cursor, Read};
use std::path::{Component, Path};

use docling::base64;
use serde_json::Value;
use waddle_core::docling_core::PictureImage;
use waddle_core::{DoclingDocument, Node};

use crate::input::{Format, Input};

/// Fill in the pictures' bytes. Returns what could not be done, one line
/// each, for the caller to report.
pub fn resolve(doc: &mut DoclingDocument, input: &Input) -> Vec<String> {
    let sources = match input.format {
        Format::Doclang => doclang_sources(&String::from_utf8_lossy(&input.bytes)),
        Format::Dclx => match entry(&input.bytes, "document.xml") {
            Some(xml) => doclang_sources(&String::from_utf8_lossy(&xml)),
            None => return vec!["the archive has no document.xml".to_string()],
        },
        Format::Json => json_sources(&input.bytes),
    };
    let mut targets = Vec::new();
    collect(&mut doc.nodes, &mut targets);
    if sources.len() != targets.len() {
        return vec![format!(
            "{} picture sources in the input and {} pictures in the document; pictures are left as placeholders",
            sources.len(),
            targets.len()
        )];
    }
    let mut problems = Vec::new();
    for (i, (source, node)) in sources.into_iter().zip(targets).enumerate() {
        let (Some(uri), Node::Picture { image, .. }) = (source, node) else {
            continue;
        };
        match load(&uri, input) {
            Ok(picture) => *image = Some(picture),
            Err(e) => problems.push(format!("picture {}: {e}", i + 1)),
        }
    }
    problems
}

/// The `<src uri>` of every `<picture>` in the DocLang, in document order,
/// `None` for a picture without one.
fn doclang_sources(xml: &str) -> Vec<Option<String>> {
    let Ok(dom) = roxmltree::Document::parse(xml) else {
        return Vec::new();
    };
    dom.descendants()
        .filter(|n| n.has_tag_name("picture"))
        .map(|p| {
            p.children()
                .find(|c| c.has_tag_name("src"))
                .and_then(|s| s.attribute("uri"))
                .map(str::to_string)
        })
        .collect()
}

/// The image URI of every picture the JSON reader will produce, in the
/// order it produces them: the body walked through its references, groups
/// entered, furniture skipped.
fn json_sources(bytes: &[u8]) -> Vec<Option<String>> {
    let Ok(root) = serde_json::from_slice::<Value>(bytes) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    if let Some(children) = root["body"]["children"].as_array() {
        for child in children {
            json_walk(child, &root, &mut out);
        }
    }
    out
}

fn json_walk(reference: &Value, root: &Value, out: &mut Vec<Option<String>>) {
    let Some(path) = reference["$ref"].as_str() else {
        return;
    };
    let Some(item) = root.pointer(path.trim_start_matches('#')) else {
        return;
    };
    if item["content_layer"].as_str() == Some("furniture") {
        return;
    }
    if path.starts_with("#/pictures/") {
        out.push(item["image"]["uri"].as_str().map(str::to_string));
    } else if path.starts_with("#/groups/")
        && let Some(children) = item["children"].as_array()
    {
        for child in children {
            json_walk(child, root, out);
        }
    }
}

/// Every picture or chart node, in document order, wrappers entered and
/// table cells walked row by row. Exhaustive so a new variant is decided.
fn collect<'a>(nodes: &'a mut [Node], out: &mut Vec<&'a mut Node>) {
    for node in nodes {
        match node {
            Node::Picture { .. } | Node::Chart { .. } => out.push(node),
            Node::Group { children, .. } => collect(children, out),
            Node::Furniture { inner, .. }
            | Node::Located { inner, .. }
            | Node::Prov { inner, .. }
            | Node::Commented { inner, .. }
            | Node::DoclangOnly(inner) => collect(std::slice::from_mut(&mut **inner), out),
            Node::Table(table) => {
                if let Some(blocks) = &mut table.cell_blocks {
                    for row in blocks {
                        for cell in row {
                            collect(cell, out);
                        }
                    }
                }
            }
            Node::Heading { .. }
            | Node::Paragraph { .. }
            | Node::CheckboxItem { .. }
            | Node::ListItem { .. }
            | Node::Code { .. }
            | Node::Formula { .. }
            | Node::Caption { .. }
            | Node::FieldRegion { .. }
            | Node::InlineGroup { .. }
            | Node::CommentSection { .. }
            | Node::PageFurniture { .. }
            | Node::PageBreak
            | Node::PageInfo { .. }
            | Node::TextDump(_) => {}
        }
    }
}

/// The bytes a source names: a data URI decoded, an entry of the archive,
/// or a file beside a bare DocLang. A path that leaves the input's own
/// directory is refused; the input names its assets, not the file system.
fn load(uri: &str, input: &Input) -> Result<PictureImage, String> {
    let bytes = if let Some(rest) = uri.strip_prefix("data:") {
        let (_, payload) = rest
            .split_once(";base64,")
            .ok_or("the data URI is not base64")?;
        base64::decode(payload).ok_or("the data URI does not decode")?
    } else {
        let relative = Path::new(uri.trim_start_matches("./"));
        if relative
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
        {
            return Err(format!("`{uri}` leaves the input's directory; not read"));
        }
        match input.format {
            Format::Dclx => entry(&input.bytes, &relative.to_string_lossy())
                .ok_or_else(|| format!("`{uri}` is not in the archive"))?,
            Format::Doclang | Format::Json => {
                let dir = input
                    .base_dir
                    .as_ref()
                    .ok_or_else(|| format!("`{uri}` cannot be found beside standard input"))?;
                std::fs::read(dir.join(relative))
                    .map_err(|e| format!("`{uri}`: {}", crate::input::reason(&e)))?
            }
        }
    };
    let format = image::guess_format(&bytes).map_err(|_| "not an image format this tool knows")?;
    let (width, height) = image::ImageReader::with_format(Cursor::new(&bytes), format)
        .into_dimensions()
        .map_err(|e| format!("cannot read the image's size: {e}"))?;
    Ok(PictureImage {
        mimetype: format.to_mime_type().to_string(),
        width,
        height,
        data: bytes,
    })
}

/// One entry of a zip archive, by name.
fn entry(archive: &[u8], name: &str) -> Option<Vec<u8>> {
    let mut zip = zip::ZipArchive::new(Cursor::new(archive)).ok()?;
    let mut file = zip.by_name(name).ok()?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).ok()?;
    Some(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn doclang_sources_follow_document_order_and_keep_gaps() {
        let xml = r#"<doclang><picture><src uri="assets/a.png"/></picture><text>x</text><picture></picture><picture class="chart"><src uri="data:image/png;base64,AA=="/></picture></doclang>"#;
        assert_eq!(
            doclang_sources(xml),
            vec![
                Some("assets/a.png".to_string()),
                None,
                Some("data:image/png;base64,AA==".to_string())
            ]
        );
    }

    #[test]
    fn json_sources_walk_the_body_and_skip_furniture() {
        let json = serde_json::json!({
            "body": {"children": [{"$ref": "#/pictures/0"}, {"$ref": "#/groups/0"}, {"$ref": "#/pictures/2"}]},
            "groups": [{"children": [{"$ref": "#/pictures/1"}]}],
            "pictures": [
                {"image": {"uri": "data:image/png;base64,AA=="}},
                {"image": {"uri": "data:image/png;base64,BB=="}},
                {"content_layer": "furniture", "image": {"uri": "data:image/png;base64,CC=="}}
            ]
        });
        assert_eq!(
            json_sources(json.to_string().as_bytes()),
            vec![
                Some("data:image/png;base64,AA==".to_string()),
                Some("data:image/png;base64,BB==".to_string())
            ]
        );
    }

    #[test]
    fn paths_that_leave_the_directory_are_refused() {
        let input = Input {
            name: "t".into(),
            bytes: Vec::new(),
            format: Format::Doclang,
            base_dir: Some(std::env::temp_dir()),
        };
        let err = load("../etc/passwd", &input).unwrap_err();
        assert!(err.contains("leaves the input's directory"), "{err}");
        let err = load("/etc/passwd", &input).unwrap_err();
        assert!(err.contains("leaves the input's directory"), "{err}");
    }
}
