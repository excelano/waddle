//! The OpenDocument Presentation writer: a `DoclingDocument` as slides.
//!
//! The reader this answers to is `walk_presentation` in docling.rs's
//! `backend/odf.rs`. Each `<draw:page>` is a slide. A frame carrying
//! `presentation:class="title"` is the slide's title and its paragraphs come
//! back as a level 1 `Heading`; a slide with no such frame is given one
//! anyway, from the page's `draw:name`. So the reader delimits slides with
//! level 1 headings and never with a `PageBreak`, and this writer does the
//! inverse.
//!
//! **The slide rule**, the thread `DESIGN.md` §9 left open: a level 1 heading
//! starts a slide and becomes its title, a `PageBreak` starts a slide with no
//! title, and a document with neither is one slide. Level 1 is the document
//! title everywhere else in this crate, which is the same rule seen from the
//! other side — a deck read in gives one level 1 heading per slide, and
//! writing it back puts each of them on a slide of its own.
//!
//! Author: David M. Anderson
//! Built with AI assistance (Claude, Anthropic)

use docling_core::{CaptionParent, DoclingDocument, Node, PictureImage, Table};

use crate::Error;
use crate::inline::{self, Run};
use crate::media::{self, PLACEHOLDER_PNG, PLACEHOLDER_SIZE_IN};
use crate::package::{self, Part};
use crate::report::{Output, Reason, Warning};
use crate::target::Target;
use crate::xml;

const STYLES: &str = include_str!("styles.xml");

/// Write `doc` as an ODP package.
pub fn write(doc: &DoclingDocument) -> Result<Output, Error> {
    let mut writer = Writer::default();
    writer.document(&doc.nodes);
    let content = writer.content();
    let manifest = writer.manifest();
    let mut parts = vec![
        Part {
            name: "mimetype",
            bytes: Target::Odp.media_type().as_bytes(),
            stored: true,
        },
        Part {
            name: "content.xml",
            bytes: content.as_bytes(),
            stored: false,
        },
        Part {
            name: "styles.xml",
            bytes: STYLES.as_bytes(),
            stored: false,
        },
        Part {
            name: "META-INF/manifest.xml",
            bytes: manifest.as_bytes(),
            stored: false,
        },
    ];
    for picture in &writer.pictures {
        parts.push(Part {
            name: &picture.name,
            bytes: &picture.bytes,
            stored: false,
        });
    }
    let bytes = package::zip(&parts)?;
    Ok(Output {
        bytes,
        warnings: writer.warnings,
    })
}

struct Picture {
    name: String,
    media_type: String,
    bytes: Vec<u8>,
}

#[derive(Default)]
struct Writer {
    body: String,
    warnings: Vec<Warning>,
    pictures: Vec<Picture>,
}

/// One slide: the heading that opened it, and the nodes that followed.
#[derive(Default)]
struct Slide<'a> {
    title: Option<&'a str>,
    body: Vec<&'a Node>,
}

impl Writer {
    fn document(&mut self, nodes: &[Node]) {
        let mut flat = Vec::new();
        self.flatten(nodes, &mut flat);
        for (index, slide) in split(&flat).iter().enumerate() {
            self.slide(index + 1, slide);
        }
    }

    /// The transparent nodes resolved away, so the slide rule sees the
    /// document's own order. A dropped layer is reported here rather than
    /// reaching the split and becoming a slide boundary.
    fn flatten<'a>(&mut self, nodes: &'a [Node], out: &mut Vec<&'a Node>) {
        for node in nodes {
            match node {
                Node::Group {
                    layer: None,
                    children,
                    ..
                } => self.flatten(children, out),
                Node::Located { inner, .. }
                | Node::Prov { inner, .. }
                | Node::Commented { inner, .. }
                | Node::DoclangOnly(inner) => self.flatten(std::slice::from_ref(inner), out),
                Node::Furniture { layer, .. } => self.warn(node, Reason::Layer(layer.value())),
                Node::Group { layer, .. } => self.warn(
                    node,
                    Reason::Layer(layer.as_ref().map_or("furniture", |l| l.value())),
                ),
                Node::CommentSection { .. }
                | Node::PageFurniture { .. }
                | Node::PageInfo { .. } => self.warn(node, Reason::Unsupported),
                Node::Heading { .. }
                | Node::Paragraph { .. }
                | Node::CheckboxItem { .. }
                | Node::ListItem { .. }
                | Node::Code { .. }
                | Node::Formula { .. }
                | Node::Caption { .. }
                | Node::FieldRegion { .. }
                | Node::InlineGroup { .. }
                | Node::TextDump(_)
                | Node::PageBreak
                | Node::Table(_)
                | Node::Picture { .. }
                | Node::Chart { .. } => out.push(node),
            }
        }
    }

    fn warn(&mut self, node: &Node, reason: Reason) {
        self.warnings.push(Warning {
            node: crate::node::kind(node),
            reason,
        });
    }

    fn slide(&mut self, number: usize, slide: &Slide<'_>) {
        self.body.push_str(&format!(
            "<draw:page draw:name=\"{}\" draw:master-page-name=\"Default\">\n",
            xml::attr(&format!("Slide {number}"))
        ));
        // A title frame is what the reader reads as the slide's title; a slide
        // without one is given a heading made from `draw:name` instead, which
        // is content this document did not have. §6.
        if let Some(title) = slide.title {
            self.body
                .push_str("<draw:frame presentation:class=\"title\"><draw:text-box>\n");
            self.paragraph(&inline::from_markdown(title));
            self.body.push_str("</draw:text-box></draw:frame>\n");
        }
        // Text accumulates into one frame; a table or a picture closes it and
        // takes a frame of its own, because the reader reads those from the
        // frame's descendants rather than from the text box.
        let mut open = false;
        for node in &slide.body {
            if takes_own_frame(node) {
                self.close_text_box(&mut open);
                self.block_frame(node);
            } else {
                self.open_text_box(&mut open);
                self.text_node(node);
            }
        }
        self.close_text_box(&mut open);
        self.body.push_str("</draw:page>\n");
    }

    fn open_text_box(&mut self, open: &mut bool) {
        if !*open {
            self.body.push_str("<draw:frame><draw:text-box>\n");
            *open = true;
        }
    }

    fn close_text_box(&mut self, open: &mut bool) {
        if *open {
            self.body.push_str("</draw:text-box></draw:frame>\n");
            *open = false;
        }
    }

    /// A block nested inside a table cell that cannot have a frame of its
    /// own there. A `draw:frame` reads its tables and images out of its
    /// *descendants*, so a real nested table would be hoisted out and counted
    /// twice; its text goes in as paragraphs instead and the nested node
    /// itself does not come back. §6.
    fn flattened(&mut self, node: &Node) {
        match node {
            Node::Table(table) | Node::Chart { table, .. } => {
                self.caption_frame_text(table.caption.as_deref());
                for row in &table.rows {
                    let line = row.join(" ");
                    self.paragraph(&inline::from_markdown(&line));
                }
            }
            Node::Picture { caption, .. } => {
                self.caption_frame_text(caption.as_deref());
            }
            Node::FieldRegion { items } => {
                for item in items {
                    let line = format!(
                        "{} {}",
                        item.key.clone().unwrap_or_default(),
                        item.value.clone().unwrap_or_default()
                    );
                    self.paragraph(&inline::from_markdown(&line));
                }
            }
            Node::Heading { .. }
            | Node::Paragraph { .. }
            | Node::CheckboxItem { .. }
            | Node::ListItem { .. }
            | Node::Code { .. }
            | Node::Formula { .. }
            | Node::Caption { .. }
            | Node::InlineGroup { .. }
            | Node::TextDump(_)
            | Node::PageBreak
            | Node::Group { .. }
            | Node::Located { .. }
            | Node::Prov { .. }
            | Node::Commented { .. }
            | Node::DoclangOnly(_)
            | Node::Furniture { .. }
            | Node::CommentSection { .. }
            | Node::PageFurniture { .. }
            | Node::PageInfo { .. } => {}
        }
    }

    /// A caption written as a plain paragraph, for use where a frame of its
    /// own is not available: inside a table cell.
    fn caption_frame_text(&mut self, caption: Option<&str>) {
        if let Some(caption) = caption {
            self.paragraph(&inline::from_markdown(caption));
        }
    }

    /// A caption in a text box of its own, before the frame it belongs to,
    /// which is where docling's Markdown puts it. A slide's text lives in a
    /// `draw:text-box` and a table or image in the frame's descendants, so the
    /// caption cannot share the frame it captions.
    fn caption_frame(&mut self, caption: Option<&str>, href: Option<&str>) {
        let Some(caption) = caption else { return };
        let runs = inline::caption_runs(caption, href);
        if inline::is_blank(&runs) {
            return;
        }
        self.body.push_str("<draw:frame><draw:text-box>\n");
        self.paragraph(&runs);
        self.body.push_str("</draw:text-box></draw:frame>\n");
    }

    /// A table, a chart or a picture in its own frame. Exhaustive; the text
    /// nodes never reach here, because `takes_own_frame` sent them elsewhere.
    fn block_frame(&mut self, node: &Node) {
        match node {
            Node::Table(table) => {
                self.caption_frame(table.caption.as_deref(), None);
                self.table_frame(table);
            }
            Node::Chart { table, caption, .. } if !table.rows.is_empty() => {
                self.caption_frame(caption.as_deref(), None);
                self.table_frame(table);
            }
            Node::Chart { caption, .. } => {
                self.caption_frame(caption.as_deref(), None);
                self.picture_frame(None);
            }
            Node::Picture {
                caption,
                caption_href,
                image,
                ..
            } => {
                self.caption_frame(caption.as_deref(), caption_href.as_deref());
                self.picture_frame(image.as_ref());
            }
            // A table of keys and values in every other target, and the thing
            // the corpus counts as a table, so it is one here too.
            Node::FieldRegion { items } => {
                let rows = items
                    .iter()
                    .map(|item| {
                        vec![
                            item.key.clone().unwrap_or_default(),
                            item.value.clone().unwrap_or_default(),
                        ]
                    })
                    .collect();
                self.table_frame(&Table {
                    rows,
                    location: None,
                    structure: None,
                    cell_blocks: None,
                    caption: None,
                    caption_parent: CaptionParent::Body,
                    cells: None,
                });
            }
            Node::Heading { .. }
            | Node::Paragraph { .. }
            | Node::CheckboxItem { .. }
            | Node::ListItem { .. }
            | Node::Code { .. }
            | Node::Formula { .. }
            | Node::Caption { .. }
            | Node::InlineGroup { .. }
            | Node::TextDump(_)
            | Node::PageBreak
            | Node::Group { .. }
            | Node::Located { .. }
            | Node::Prov { .. }
            | Node::Commented { .. }
            | Node::DoclangOnly(_)
            | Node::Furniture { .. }
            | Node::CommentSection { .. }
            | Node::PageFurniture { .. }
            | Node::PageInfo { .. } => {}
        }
    }

    /// The nodes a slide's text box can hold. Exhaustive on purpose; the
    /// variants handled by the caller are unreachable here and say so.
    fn text_node(&mut self, node: &Node) {
        match node {
            // Level 1 opened this slide and is its title, so a heading
            // reaching here is level 2 or deeper: ODF outline level is one
            // shallower, the inverse of what the reader does. §5.
            Node::Heading { level, text } => {
                let outline = level.saturating_sub(1).max(1);
                self.body
                    .push_str(&format!("<text:h text:outline-level=\"{outline}\">"));
                self.runs(&inline::from_markdown(text));
                self.body.push_str("</text:h>\n");
            }
            Node::Paragraph { text } => self.paragraph(&inline::from_markdown(text)),
            Node::InlineGroup { md_text, .. } => self.paragraph(&inline::from_markdown(md_text)),
            Node::CheckboxItem { checked, text } => {
                let mark = if *checked { "[x] " } else { "[ ] " };
                let mut runs = vec![inline::plain(mark)];
                runs.extend(inline::from_markdown(text));
                self.paragraph(&runs);
            }
            // A list on a slide is a list: the reader's `add_odf_list` reads
            // `text:list` back. Depth nests, and the marker style is the
            // stock one, so an ordered list returns unordered. §6.
            Node::ListItem { text, dclx, .. } => {
                let text = dclx.as_ref().map_or(text.as_str(), |d| d.text.as_str());
                self.body.push_str("<text:list><text:list-item>");
                self.paragraph(&inline::from_markdown(text));
                self.body.push_str("</text:list-item></text:list>\n");
            }
            Node::Code { text, pretty, .. } => {
                let code = pretty.as_deref().unwrap_or(text);
                for line in code.lines() {
                    self.paragraph(&[inline::plain(line)]);
                }
            }
            Node::Formula { latex, orig, .. } => {
                let source = if latex.is_empty() { orig } else { latex };
                self.paragraph(&[inline::plain(source)]);
            }
            Node::Caption { text, href } => {
                self.paragraph(&inline::caption_runs(text, href.as_deref()));
            }
            Node::TextDump(text) => {
                for block in text.split("\n\n") {
                    self.paragraph(&inline::from_markdown(block));
                }
            }
            // A page break is the slide boundary itself and never reaches a
            // text box; the rest are resolved or reported before the split.
            Node::PageBreak
            | Node::Table(_)
            | Node::Chart { .. }
            | Node::Picture { .. }
            | Node::FieldRegion { .. }
            | Node::Group { .. }
            | Node::Located { .. }
            | Node::Prov { .. }
            | Node::Commented { .. }
            | Node::DoclangOnly(_)
            | Node::Furniture { .. }
            | Node::CommentSection { .. }
            | Node::PageFurniture { .. }
            | Node::PageInfo { .. } => {}
        }
    }

    fn paragraph(&mut self, runs: &[Run]) {
        if inline::is_blank(runs) {
            return;
        }
        self.body.push_str("<text:p>");
        self.runs(runs);
        self.body.push_str("</text:p>\n");
    }

    fn runs(&mut self, runs: &[Run]) {
        for run in inline::merge_adjacent(runs.to_vec()) {
            let text = xml::text(&run.text);
            match &run.href {
                Some(href) => self.body.push_str(&format!(
                    "<text:a xlink:href=\"{}\">{text}</text:a>",
                    xml::attr(href)
                )),
                None => self.body.push_str(&text),
            }
        }
    }

    /// A table in its own frame: the reader takes `table:table` from a
    /// frame's descendants. Presentation tables are plain text cells.
    fn table_frame(&mut self, table: &Table) {
        let columns = table.rows.iter().map(Vec::len).max().unwrap_or(0);
        if columns == 0 {
            return;
        }
        self.body.push_str(&format!(
            "<draw:frame><table:table>\n<table:table-column table:number-columns-repeated=\"{columns}\"/>\n"
        ));
        for (r, row) in table.rows.iter().enumerate() {
            self.body.push_str("<table:table-row>");
            for c in 0..columns {
                self.body.push_str("<table:table-cell>");
                // A rich cell keeps its blocks. Flattening them to the cell's
                // text would hand the reader Markdown to parse again, and the
                // second trip would not land where the first one did.
                let blocks = table
                    .cell_blocks
                    .as_ref()
                    .and_then(|b| b.get(r))
                    .and_then(|row| row.get(c))
                    .filter(|blocks| !blocks.is_empty());
                match blocks {
                    Some(blocks) => {
                        for block in blocks {
                            if takes_own_frame(block) {
                                self.flattened(block);
                            } else {
                                self.text_node(block);
                            }
                        }
                    }
                    None => {
                        let text = row.get(c).map_or("", String::as_str);
                        self.paragraph(&inline::from_markdown(text));
                    }
                }
                self.body.push_str("</table:table-cell>");
            }
            self.body.push_str("</table:table-row>\n");
        }
        self.body.push_str("</table:table></draw:frame>\n");
    }

    fn picture_frame(&mut self, image: Option<&PictureImage>) {
        let (media_type, bytes, width_in, height_in) = match image {
            Some(image) => {
                let (w, h) = media::fit(image.width, image.height);
                (image.mimetype.as_str(), image.data.as_slice(), w, h)
            }
            None => {
                self.warnings.push(Warning {
                    node: "picture",
                    reason: Reason::Placeholder,
                });
                let (w, h) = PLACEHOLDER_SIZE_IN;
                ("image/png", PLACEHOLDER_PNG, w, h)
            }
        };
        let index = self.pictures.len() + 1;
        let name = format!("Pictures/image{index}.{}", media::extension(media_type));
        self.body.push_str(&format!(
            "<draw:frame svg:width=\"{width_in}in\" svg:height=\"{height_in}in\"><draw:image xlink:href=\"{}\" draw:mime-type=\"{}\"/></draw:frame>\n",
            xml::attr(&name),
            xml::attr(media_type)
        ));
        self.pictures.push(Picture {
            name,
            media_type: media_type.to_string(),
            bytes: bytes.to_vec(),
        });
    }

    fn manifest(&self) -> String {
        let mut out = String::from(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<manifest:manifest xmlns:manifest="urn:oasis:names:tc:opendocument:xmlns:manifest:1.0" manifest:version="1.3">
  <manifest:file-entry manifest:full-path="/" manifest:version="1.3" manifest:media-type="application/vnd.oasis.opendocument.presentation"/>
  <manifest:file-entry manifest:full-path="content.xml" manifest:media-type="text/xml"/>
  <manifest:file-entry manifest:full-path="styles.xml" manifest:media-type="text/xml"/>
"#,
        );
        for picture in &self.pictures {
            out.push_str(&format!(
                "  <manifest:file-entry manifest:full-path=\"{}\" manifest:media-type=\"{}\"/>\n",
                xml::attr(&picture.name),
                xml::attr(&picture.media_type)
            ));
        }
        out.push_str("</manifest:manifest>\n");
        out
    }

    fn content(&self) -> String {
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:draw="urn:oasis:names:tc:opendocument:xmlns:drawing:1.0" xmlns:presentation="urn:oasis:names:tc:opendocument:xmlns:presentation:1.0" xmlns:table="urn:oasis:names:tc:opendocument:xmlns:table:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" xmlns:svg="urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0" xmlns:xlink="http://www.w3.org/1999/xlink" office:version="1.3">
<office:body><office:presentation>
{}</office:presentation></office:body></office:document-content>
"#,
            self.body
        )
    }
}

/// The slide rule. A level 1 heading starts a slide and titles it, a page
/// break starts an untitled one, and content before either opens the first
/// slide. A document with no split point at all is one slide.
fn split<'a>(nodes: &[&'a Node]) -> Vec<Slide<'a>> {
    let mut slides: Vec<Slide<'a>> = Vec::new();
    for node in nodes {
        match node {
            Node::Heading { level: 1, text } => slides.push(Slide {
                title: Some(text),
                body: Vec::new(),
            }),
            Node::PageBreak => slides.push(Slide::default()),
            // Everything else belongs to the slide that is open, and opens
            // the first one if none is. Spelled out so a variant docling.rs
            // adds is decided here rather than landing on a slide by default.
            Node::Heading { .. }
            | Node::Paragraph { .. }
            | Node::CheckboxItem { .. }
            | Node::ListItem { .. }
            | Node::Code { .. }
            | Node::Formula { .. }
            | Node::Caption { .. }
            | Node::FieldRegion { .. }
            | Node::InlineGroup { .. }
            | Node::TextDump(_)
            | Node::Table(_)
            | Node::Picture { .. }
            | Node::Chart { .. }
            | Node::Group { .. }
            | Node::Located { .. }
            | Node::Prov { .. }
            | Node::Commented { .. }
            | Node::DoclangOnly(_)
            | Node::Furniture { .. }
            | Node::CommentSection { .. }
            | Node::PageFurniture { .. }
            | Node::PageInfo { .. } => {
                if slides.is_empty() {
                    slides.push(Slide::default());
                }
                slides.last_mut().expect("a slide is open").body.push(node);
            }
        }
    }
    slides
}

/// Whether a node takes a frame of its own rather than going in the slide's
/// text box: the reader reads a table and an image from a frame's descendants
/// and text from a `draw:text-box`, so the two cannot share one frame.
fn takes_own_frame(node: &Node) -> bool {
    match node {
        Node::Table(_) | Node::Picture { .. } | Node::Chart { .. } | Node::FieldRegion { .. } => {
            true
        }
        Node::Heading { .. }
        | Node::Paragraph { .. }
        | Node::CheckboxItem { .. }
        | Node::ListItem { .. }
        | Node::Code { .. }
        | Node::Formula { .. }
        | Node::Caption { .. }
        | Node::InlineGroup { .. }
        | Node::TextDump(_)
        | Node::PageBreak
        | Node::Group { .. }
        | Node::Located { .. }
        | Node::Prov { .. }
        | Node::Commented { .. }
        | Node::DoclangOnly(_)
        | Node::Furniture { .. }
        | Node::CommentSection { .. }
        | Node::PageFurniture { .. }
        | Node::PageInfo { .. } => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn content_of(doc: &DoclingDocument) -> String {
        let mut writer = Writer::default();
        writer.document(&doc.nodes);
        writer.content()
    }

    #[test]
    fn a_level_one_heading_starts_a_slide_and_titles_it() {
        let mut doc = DoclingDocument::new("t");
        doc.add_heading(1, "First");
        doc.add_paragraph("one");
        doc.add_heading(1, "Second");
        doc.add_paragraph("two");
        let xml = content_of(&doc);
        assert_eq!(xml.matches("<draw:page ").count(), 2);
        assert_eq!(
            xml.matches(r#"<draw:frame presentation:class="title">"#)
                .count(),
            2
        );
        assert!(xml.contains("First"));
        assert!(xml.contains("Second"));
    }

    #[test]
    fn a_page_break_starts_a_slide_with_no_title() {
        let mut doc = DoclingDocument::new("t");
        doc.add_paragraph("before");
        doc.push(Node::PageBreak);
        doc.add_paragraph("after");
        let xml = content_of(&doc);
        assert_eq!(xml.matches("<draw:page ").count(), 2);
        assert_eq!(xml.matches(r#"presentation:class="title""#).count(), 0);
    }

    #[test]
    fn a_document_with_no_split_point_is_one_slide() {
        let mut doc = DoclingDocument::new("t");
        doc.add_heading(2, "Not a split");
        doc.add_paragraph("body");
        let xml = content_of(&doc);
        assert_eq!(xml.matches("<draw:page ").count(), 1);
        // Level 1 is the slide title, so level 2 is ODF outline level 1 —
        // the inverse of what the reader does. DESIGN.md §5.
        assert!(xml.contains(r#"<text:h text:outline-level="1">"#));
    }

    #[test]
    fn a_table_and_its_caption_take_frames_beside_the_text() {
        let mut doc = DoclingDocument::new("t");
        doc.add_paragraph("intro");
        doc.push(Node::Table(Table {
            rows: vec![vec!["A".into(), "B".into()]],
            location: None,
            structure: None,
            cell_blocks: None,
            caption: Some("Table 1".into()),
            caption_parent: CaptionParent::Body,
            cells: None,
        }));
        let xml = content_of(&doc);
        // The text box closes before the table's frame opens: a frame reads
        // its tables from its descendants and its text from a text box.
        assert!(xml.contains("</draw:text-box></draw:frame>\n<draw:frame><draw:text-box>"));
        assert!(xml.contains("<draw:frame><table:table>"));
        assert!(xml.contains("Table 1"));
    }

    #[test]
    fn the_package_is_an_odp() {
        let mut doc = DoclingDocument::new("t");
        doc.add_paragraph("x");
        let out = write(&doc).unwrap();
        assert_eq!(&out.bytes[30..38], b"mimetype");
        assert!(
            String::from_utf8_lossy(&out.bytes)
                .contains("application/vnd.oasis.opendocument.presentation")
        );
    }
}
