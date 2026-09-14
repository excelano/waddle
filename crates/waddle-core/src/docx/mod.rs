//! Office Open XML WordprocessingML.
//!
//! An OPC package: `[Content_Types].xml`, the package relationships,
//! `word/document.xml` with its relationships, the stock `word/styles.xml`,
//! `word/numbering.xml`, and the media. The constructs are the ones
//! docling.rs's DOCX reader keys on, so that reading the package back
//! yields the nodes it was written from; `DESIGN.md` §5 is the mapping and
//! §6 the diffs the reader cannot close.
//!
//! Author: David M. Anderson
//! Built with AI assistance (Claude, Anthropic)

use docling_core::{CaptionParent, DoclingDocument, Node, PictureImage, Table};

use crate::Error;
use crate::inline::{
    self, Position, Run, caption_runs, collapse_spaces, is_blank, merge_adjacent, outside_links,
    plain,
};
use crate::list::Style as ListStyle;
use crate::media::{self, PLACEHOLDER_PNG, PLACEHOLDER_SIZE_IN};
use crate::node::kind;
use crate::package::{self, Part};
use crate::report::{Output, Reason, Warning};
use crate::xml;

mod list;

const STYLES: &str = include_str!("styles.xml");

const ROOT_RELS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
</Relationships>
"#;

const DOCUMENT_HEAD: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:pic="http://schemas.openxmlformats.org/drawingml/2006/picture" xmlns:w14="http://schemas.microsoft.com/office/word/2010/wordml" xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006" mc:Ignorable="w14">
<w:body>
"#;

/// Letter, one-inch margins, in twentieths of a point.
const DOCUMENT_TAIL: &str = r#"<w:sectPr><w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="720" w:footer="720" w:gutter="0"/></w:sectPr>
</w:body>
</w:document>
"#;

/// OOXML's highest heading.
const MAX_HEADING: u8 = 9;

/// English Metric Units per inch.
const EMU_PER_INCH: f64 = 914_400.0;

/// Write the document as a DOCX package.
pub fn write(doc: &DoclingDocument) -> Result<Output, Error> {
    let mut writer = Writer::default();
    writer.nodes(&doc.nodes);
    let document = writer.document();
    let rels = writer.rels();
    let numbering = list::numbering_xml(&writer.numbering, &writer.nums);
    let content_types = writer.content_types();
    let mut parts = vec![
        Part {
            name: "[Content_Types].xml",
            bytes: content_types.as_bytes(),
            stored: false,
        },
        Part {
            name: "_rels/.rels",
            bytes: ROOT_RELS.as_bytes(),
            stored: false,
        },
        Part {
            name: "word/document.xml",
            bytes: document.as_bytes(),
            stored: false,
        },
        Part {
            name: "word/_rels/document.xml.rels",
            bytes: rels.as_bytes(),
            stored: false,
        },
        Part {
            name: "word/styles.xml",
            bytes: STYLES.as_bytes(),
            stored: false,
        },
        Part {
            name: "word/numbering.xml",
            bytes: numbering.as_bytes(),
            stored: false,
        },
    ];
    for media in &writer.media {
        parts.push(Part {
            name: &media.part,
            bytes: &media.bytes,
            stored: false,
        });
    }
    let bytes = package::zip(&parts)?;
    Ok(Output {
        bytes,
        warnings: writer.warnings,
    })
}

/// One relationship of the document part beyond the two fixed ones.
enum Rel {
    Hyperlink(String),
    Image(String),
}

/// One image part of the package.
struct Media {
    /// The package path, `word/media/image1.png`.
    part: String,
    extension: &'static str,
    media_type: String,
    bytes: Vec<u8>,
}

#[derive(Default)]
pub(super) struct Writer {
    body: String,
    rels: Vec<Rel>,
    media: Vec<Media>,
    numbering: Vec<ListStyle>,
    /// Each `w:num`, as the index of its definition.
    nums: Vec<usize>,
    /// Inside a table cell, where only run formatting keeps a cell rich in
    /// the reader and only a rich cell keeps its links.
    in_cell: bool,
    warnings: Vec<Warning>,
}

impl Writer {
    fn document(&self) -> String {
        let mut out = String::from(DOCUMENT_HEAD);
        out.push_str(&self.body);
        // Word wants the body to end in a paragraph.
        if self.body.ends_with("</w:tbl>\n") {
            out.push_str("<w:p/>\n");
        }
        out.push_str(DOCUMENT_TAIL);
        out
    }

    fn rels(&self) -> String {
        let mut out = String::from(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>
<Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/numbering" Target="numbering.xml"/>
"#,
        );
        for (i, rel) in self.rels.iter().enumerate() {
            let id = rel_id(i);
            match rel {
                Rel::Hyperlink(target) => out.push_str(&format!(
                    "<Relationship Id=\"{id}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink\" Target=\"{}\" TargetMode=\"External\"/>\n",
                    xml::attr(target)
                )),
                Rel::Image(part) => out.push_str(&format!(
                    "<Relationship Id=\"{id}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/image\" Target=\"{}\"/>\n",
                    xml::attr(part.trim_start_matches("word/"))
                )),
            }
        }
        out.push_str("</Relationships>\n");
        out
    }

    fn content_types(&self) -> String {
        let mut out = String::from(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
<Default Extension="xml" ContentType="application/xml"/>
"#,
        );
        let mut seen: Vec<&str> = Vec::new();
        for media in &self.media {
            if !seen.contains(&media.extension) {
                seen.push(media.extension);
                out.push_str(&format!(
                    "<Default Extension=\"{}\" ContentType=\"{}\"/>\n",
                    media.extension,
                    xml::attr(&media.media_type)
                ));
            }
        }
        out.push_str(
            r#"<Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
<Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/>
<Override PartName="/word/numbering.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.numbering+xml"/>
</Types>
"#,
        );
        out
    }

    fn warn(&mut self, node: &Node, reason: Reason) {
        self.warnings.push(Warning {
            node: kind(node),
            reason,
        });
    }

    /// A sequence of nodes. Consecutive list items form one list.
    pub(super) fn nodes(&mut self, nodes: &[Node]) {
        let mut i = 0;
        while i < nodes.len() {
            if matches!(nodes[i], Node::ListItem { .. }) {
                let end = nodes[i..]
                    .iter()
                    .position(|n| !matches!(n, Node::ListItem { .. }))
                    .map_or(nodes.len(), |p| i + p);
                self.list(&nodes[i..end]);
                i = end;
            } else {
                self.node(&nodes[i]);
                i += 1;
            }
        }
    }

    /// One node. Exhaustive on purpose; see the crate documentation.
    fn node(&mut self, node: &Node) {
        match node {
            Node::Heading { level, text } => {
                self.heading(*level, &collapse_spaces(inline::from_markdown(text)))
            }
            // The reader returns a blank paragraph as an empty text node, so
            // one is written for one; it is how Word keeps a blank line.
            Node::Paragraph { text } => self.paragraph(None, &inline::from_markdown(text), true),
            Node::InlineGroup { runs, md_text, .. } => {
                self.paragraph(None, &inline::from_group(md_text, runs), true)
            }
            Node::CheckboxItem { checked, text } => self.checkbox(*checked, text),
            Node::ListItem { .. } => self.list(std::slice::from_ref(node)),
            Node::Code { text, pretty, .. } => {
                let code = pretty.as_deref().unwrap_or(text);
                for line in code.lines() {
                    self.paragraph(Some("SourceCode"), &[plain(line)], true);
                }
            }
            Node::Table(table) => self.table(table, table.caption.as_deref()),
            Node::Picture { caption, image, .. } => {
                self.picture(caption.as_deref(), image.as_ref())
            }
            Node::Formula { latex, orig, .. } => {
                let source = if latex.is_empty() { orig } else { latex };
                self.paragraph(Some("SourceCode"), &[plain(source)], false);
            }
            // A standalone caption: one no picture or table claimed, which
            // the HTML backend emits for a figure that produced neither. The
            // Caption style is the one this writer already gives a table's or
            // picture's caption; the hyperlink annotation covers the whole
            // text, as it does in docling's Markdown.
            Node::Caption { text, href } => {
                self.paragraph(Some("Caption"), &caption_runs(text, href.as_deref()), false);
            }
            Node::Chart { table, caption, .. } if !table.rows.is_empty() => {
                self.table(table, caption.as_deref())
            }
            Node::Chart { caption, .. } => self.picture(caption.as_deref(), None),
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
                self.table(
                    &Table {
                        rows,
                        location: None,
                        structure: None,
                        cell_blocks: None,
                        caption: None,
                        // No caption to hang, so the field is the default
                        // every declarative backend leaves alone.
                        caption_parent: CaptionParent::Body,
                        cells: None,
                    },
                    None,
                );
            }
            Node::TextDump(text) => {
                for block in text.split("\n\n") {
                    self.paragraph(None, &[plain(block)], false);
                }
            }
            Node::Group {
                layer: Some(layer), ..
            } => self.warn(node, Reason::Layer(layer.value())),
            Node::Group {
                layer: None,
                children,
                ..
            } => self.nodes(children),
            Node::Located { inner, .. }
            | Node::Prov { inner, .. }
            | Node::Commented { inner, .. }
            | Node::DoclangOnly(inner) => self.node(inner),
            Node::Furniture { layer, .. } => self.warn(node, Reason::Layer(layer.value())),
            Node::PageBreak => self
                .body
                .push_str("<w:p><w:r><w:br w:type=\"page\"/></w:r></w:p>\n"),
            Node::PageInfo { .. } => {}
            Node::CommentSection { .. } | Node::PageFurniture { .. } => {
                self.warn(node, Reason::Unsupported)
            }
        }
    }

    /// Level 1 is the document title; every other level is the target's
    /// heading one below it. `DESIGN.md` §5.
    fn heading(&mut self, level: u8, runs: &[Run]) {
        if is_blank(runs) {
            return;
        }
        if level <= 1 {
            self.paragraph(Some("Title"), runs, false);
            return;
        }
        let n = (level - 1).min(MAX_HEADING);
        self.paragraph(Some(&format!("Heading{n}")), runs, false);
    }

    /// A paragraph in a style, or in the default one. A blank one is written
    /// empty when `keep_blank`, else not at all.
    fn paragraph(&mut self, style: Option<&str>, runs: &[Run], keep_blank: bool) {
        if is_blank(runs) {
            if keep_blank {
                match style {
                    Some(style) => self.body.push_str(&format!(
                        "<w:p><w:pPr><w:pStyle w:val=\"{style}\"/></w:pPr></w:p>\n"
                    )),
                    None => self.body.push_str("<w:p/>\n"),
                }
            }
            return;
        }
        self.body.push_str("<w:p>");
        if let Some(style) = style {
            self.body
                .push_str(&format!("<w:pPr><w:pStyle w:val=\"{style}\"/></w:pPr>"));
        }
        self.runs(runs);
        self.body.push_str("</w:p>\n");
    }

    /// A checkbox content control, which is what the reader detects,
    /// followed by the text.
    fn checkbox(&mut self, checked: bool, text: &str) {
        let (state, glyph) = if checked { ("1", "☑") } else { ("0", "☐") };
        self.body.push_str(&format!(
            "<w:p><w:sdt><w:sdtPr><w14:checkbox><w14:checked w14:val=\"{state}\"/><w14:checkedState w14:val=\"2612\" w14:font=\"MS Gothic\"/><w14:uncheckedState w14:val=\"2610\" w14:font=\"MS Gothic\"/></w14:checkbox></w:sdtPr><w:sdtContent><w:r><w:rPr><w:rFonts w:ascii=\"MS Gothic\" w:hAnsi=\"MS Gothic\" w:eastAsia=\"MS Gothic\"/></w:rPr><w:t>{glyph}</w:t></w:r></w:sdtContent></w:sdt>"
        ));
        let mut runs = vec![plain(" ")];
        runs.extend(inline::from_markdown(text));
        self.runs(&runs);
        self.body.push_str("</w:p>\n");
    }

    /// The caption, then the table, which is the order docling's Markdown
    /// renders them in.
    fn table(&mut self, table: &Table, caption: Option<&str>) {
        if let Some(caption) = caption {
            self.paragraph(Some("Caption"), &inline::from_markdown(caption), false);
        }
        let columns = table.rows.iter().map(Vec::len).max().unwrap_or(0);
        if columns == 0 {
            return;
        }
        // Two tables back to back get the paragraph between them that keeps
        // Word from joining them into one; judged on what was written, since
        // the tables may sit in different groups or wrappers.
        if self.body.ends_with("</w:tbl>\n") {
            self.body.push_str("<w:p/>\n");
        }
        let cells = table.derive_cells();
        // Which anchor covers each grid position.
        let mut covered = vec![vec![None; columns]; table.rows.len()];
        for (index, cell) in cells.iter().enumerate() {
            for row in covered
                .iter_mut()
                .take(cell.start_row + cell.row_span)
                .skip(cell.start_row)
            {
                for slot in row
                    .iter_mut()
                    .take(cell.start_col + cell.col_span)
                    .skip(cell.start_col)
                {
                    *slot = Some(index);
                }
            }
        }
        let is_header = |r: usize| {
            cells
                .iter()
                .filter(|c| c.start_row == r)
                .all(|c| c.column_header)
        };
        let header_rows = (0..table.rows.len()).take_while(|&r| is_header(r)).count();
        let width = 9360 / columns;
        self.body.push_str(
            "<w:tbl><w:tblPr><w:tblStyle w:val=\"TableGrid\"/><w:tblW w:w=\"0\" w:type=\"auto\"/><w:tblLook w:val=\"04A0\"/></w:tblPr><w:tblGrid>",
        );
        for _ in 0..columns {
            self.body.push_str(&format!("<w:gridCol w:w=\"{width}\"/>"));
        }
        self.body.push_str("</w:tblGrid>\n");
        for (r, row) in covered.iter().enumerate() {
            self.body.push_str("<w:tr>");
            if r < header_rows {
                self.body.push_str("<w:trPr><w:tblHeader/></w:trPr>");
            }
            for (c, slot) in row.iter().enumerate() {
                let Some(index) = *slot else {
                    self.body.push_str(
                        "<w:tc><w:tcPr><w:tcW w:w=\"0\" w:type=\"auto\"/></w:tcPr><w:p/></w:tc>",
                    );
                    continue;
                };
                let cell = &cells[index];
                if cell.start_col != c {
                    continue; // inside a horizontal span: no cell of its own
                }
                let mut props = String::from("<w:tcW w:w=\"0\" w:type=\"auto\"/>");
                if cell.col_span > 1 {
                    props.push_str(&format!("<w:gridSpan w:val=\"{}\"/>", cell.col_span));
                }
                if cell.start_row != r {
                    // A vertical continuation: a cell that says so and nothing else.
                    props.push_str("<w:vMerge/>");
                    self.body
                        .push_str(&format!("<w:tc><w:tcPr>{props}</w:tcPr><w:p/></w:tc>"));
                    continue;
                }
                if cell.row_span > 1 {
                    props.push_str("<w:vMerge w:val=\"restart\"/>");
                }
                self.body
                    .push_str(&format!("<w:tc><w:tcPr>{props}</w:tcPr>"));
                self.in_cell = true;
                let blocks = table
                    .cell_blocks
                    .as_ref()
                    .and_then(|b| b.get(r))
                    .and_then(|row| row.get(c))
                    .filter(|blocks| !blocks.is_empty());
                match blocks {
                    Some(blocks) => {
                        self.nodes(blocks);
                        // A cell ends in a paragraph; after a nested table
                        // Word requires one.
                        if matches!(blocks.last(), Some(Node::Table(_))) {
                            self.body.push_str("<w:p/>");
                        }
                    }
                    None => {
                        let style = if r < header_rows {
                            "TableHead"
                        } else {
                            "TableText"
                        };
                        let runs = inline::from_markdown(&cell.text);
                        if is_blank(&runs) {
                            self.body.push_str("<w:p/>");
                        } else {
                            self.paragraph(Some(style), &runs, false);
                        }
                    }
                }
                self.in_cell = false;
                self.body.push_str("</w:tc>");
            }
            self.body.push_str("</w:tr>\n");
        }
        self.body.push_str("</w:tbl>\n");
    }

    /// The caption, then the picture in its own paragraph. A picture without
    /// bytes is written as the grey placeholder and reported.
    fn picture(&mut self, caption: Option<&str>, image: Option<&PictureImage>) {
        if let Some(caption) = caption {
            self.paragraph(Some("Caption"), &inline::from_markdown(caption), false);
        }
        let (media_type, bytes, (width_in, height_in)) = match image {
            Some(image) => (
                image.mimetype.as_str(),
                image.data.as_slice(),
                media::fit(image.width, image.height),
            ),
            None => {
                self.warnings.push(Warning {
                    node: "picture",
                    reason: Reason::Placeholder,
                });
                ("image/png", PLACEHOLDER_PNG, PLACEHOLDER_SIZE_IN)
            }
        };
        let index = self.media.len() + 1;
        let extension = media::extension(media_type);
        let part = format!("word/media/image{index}.{extension}");
        self.media.push(Media {
            part: part.clone(),
            extension,
            media_type: media_type.to_string(),
            bytes: bytes.to_vec(),
        });
        self.rels.push(Rel::Image(part));
        let rid = rel_id(self.rels.len() - 1);
        let cx = (width_in * EMU_PER_INCH) as u64;
        let cy = (height_in * EMU_PER_INCH) as u64;
        self.body.push_str(&format!(
            "<w:p><w:r><w:drawing><wp:inline distT=\"0\" distB=\"0\" distL=\"0\" distR=\"0\"><wp:extent cx=\"{cx}\" cy=\"{cy}\"/><wp:docPr id=\"{index}\" name=\"Picture {index}\"/><a:graphic><a:graphicData uri=\"http://schemas.openxmlformats.org/drawingml/2006/picture\"><pic:pic><pic:nvPicPr><pic:cNvPr id=\"{index}\" name=\"image{index}.{extension}\"/><pic:cNvPicPr/></pic:nvPicPr><pic:blipFill><a:blip r:embed=\"{rid}\"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill><pic:spPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"{cx}\" cy=\"{cy}\"/></a:xfrm><a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom></pic:spPr></pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing></w:r></w:p>\n"
        ));
    }

    /// The runs of one paragraph. Consecutive runs with the same link share
    /// one `w:hyperlink`.
    pub(super) fn runs(&mut self, runs: &[Run]) {
        let runs = merge_adjacent(outside_links(runs));
        let mut open_href: Option<&str> = None;
        for run in &runs {
            if run.href.as_deref() != open_href {
                if open_href.is_some() {
                    self.body.push_str("</w:hyperlink>");
                }
                open_href = run.href.as_deref();
                if let Some(href) = open_href {
                    // In a cell, an empty underlined run before the link: the
                    // reader keeps a cell's links only when it finds run
                    // formatting, and it looks at the paragraph's direct runs,
                    // never inside a hyperlink. The run shows nothing.
                    if self.in_cell {
                        self.body
                            .push_str("<w:r><w:rPr><w:u w:val=\"single\"/></w:rPr></w:r>");
                    }
                    let rid = self.hyperlink(href);
                    self.body.push_str(&format!("<w:hyperlink r:id=\"{rid}\">"));
                }
            }
            self.run(run);
        }
        if open_href.is_some() {
            self.body.push_str("</w:hyperlink>");
        }
    }

    /// The relationship for a link target, one per distinct target.
    fn hyperlink(&mut self, target: &str) -> String {
        let index = match self
            .rels
            .iter()
            .position(|r| matches!(r, Rel::Hyperlink(t) if t == target))
        {
            Some(i) => i,
            None => {
                self.rels.push(Rel::Hyperlink(target.to_string()));
                self.rels.len() - 1
            }
        };
        rel_id(index)
    }

    /// One `w:r`: its properties, then its text with breaks as elements.
    fn run(&mut self, run: &Run) {
        let mut props = String::new();
        if run.href.is_some() {
            props.push_str("<w:rStyle w:val=\"Hyperlink\"/>");
        }
        if run.code {
            props.push_str(
                "<w:rFonts w:ascii=\"Consolas\" w:hAnsi=\"Consolas\" w:cs=\"Consolas\"/>",
            );
        }
        if run.bold {
            props.push_str("<w:b/><w:bCs/>");
        }
        if run.italic {
            props.push_str("<w:i/><w:iCs/>");
        }
        if run.strike {
            props.push_str("<w:strike/>");
        }
        if run.underline {
            props.push_str("<w:u w:val=\"single\"/>");
        }
        match run.script {
            Position::Baseline => {}
            Position::Sub => props.push_str("<w:vertAlign w:val=\"subscript\"/>"),
            Position::Super => props.push_str("<w:vertAlign w:val=\"superscript\"/>"),
        }
        self.body.push_str("<w:r>");
        if !props.is_empty() {
            self.body.push_str(&format!("<w:rPr>{props}</w:rPr>"));
        }
        for (i, line) in run.text.split('\n').enumerate() {
            if i > 0 {
                self.body.push_str("<w:br/>");
            }
            if !line.is_empty() {
                self.body.push_str(&format!(
                    "<w:t xml:space=\"preserve\">{}</w:t>",
                    xml::text(line)
                ));
            }
        }
        self.body.push_str("</w:r>");
    }
}

/// The id of the n-th relationship beyond the two fixed ones.
fn rel_id(index: usize) -> String {
    format!("rId{}", index + 3)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body_of(doc: &DoclingDocument) -> String {
        let mut writer = Writer::default();
        writer.nodes(&doc.nodes);
        writer.body
    }

    #[test]
    fn a_standalone_caption_is_a_caption_paragraph_under_its_link() {
        let mut doc = DoclingDocument::new("t");
        doc.push(Node::Caption {
            text: "Figure 2: **the** rig".into(),
            href: Some("https://a.org/rig".into()),
        });
        doc.push(Node::Caption {
            text: String::new(),
            href: None,
        });
        let mut writer = Writer::default();
        writer.nodes(&doc.nodes);
        let body = &writer.body;
        assert!(body.contains(r#"<w:pStyle w:val="Caption"/>"#));
        assert!(body.contains(
            "<w:rStyle w:val=\"Hyperlink\"/><w:b/><w:bCs/></w:rPr><w:t xml:space=\"preserve\">the</w:t>"
        ));
        // The annotation covers the whole caption, so the bold run is linked
        // along with the plain ones, and one relationship serves them all.
        assert_eq!(body.matches("<w:hyperlink r:id=\"rId3\">").count(), 3);
        assert_eq!(writer.rels.len(), 1);
        // An empty caption is no paragraph, as it is no line in Markdown.
        assert_eq!(body.matches(r#"<w:pStyle w:val="Caption"/>"#).count(), 1);
    }

    #[test]
    fn a_provenance_wrapper_is_written_as_what_it_wraps() {
        let mut doc = DoclingDocument::new("t");
        let mut inner = DoclingDocument::new("i");
        inner.add_heading(2, "Wrapped");
        let heading = inner.nodes.remove(0);
        doc.push(Node::Prov {
            inner: Box::new(heading),
            page_no: 1,
            bbox: [0.0, 0.0, 1.0, 1.0],
            charspan: [0, 0],
            seq: None,
        });
        let body = body_of(&doc);
        assert!(body.contains(r#"<w:pStyle w:val="Heading1"/>"#));
        assert!(body.contains("Wrapped"));
    }

    #[test]
    fn title_and_headings_follow_the_reader_convention() {
        let mut doc = DoclingDocument::new("t");
        doc.add_heading(1, "Report");
        doc.add_heading(2, "Intro");
        doc.add_heading(12, "Deep");
        let body = body_of(&doc);
        assert!(body.contains(
            r#"<w:pStyle w:val="Title"/></w:pPr><w:r><w:t xml:space="preserve">Report</w:t>"#
        ));
        assert!(body.contains(
            r#"<w:pStyle w:val="Heading1"/></w:pPr><w:r><w:t xml:space="preserve">Intro</w:t>"#
        ));
        assert!(body.contains(r#"<w:pStyle w:val="Heading9"/>"#));
    }

    #[test]
    fn runs_carry_their_properties_and_links_share_a_relationship() {
        let mut doc = DoclingDocument::new("t");
        doc.add_paragraph("**b** [x](https://a.org/) and [y](https://a.org/)\nnext");
        let mut writer = Writer::default();
        writer.nodes(&doc.nodes);
        assert!(
            writer
                .body
                .contains("<w:rPr><w:b/><w:bCs/></w:rPr><w:t xml:space=\"preserve\">b</w:t>")
        );
        assert_eq!(
            writer.body.matches("<w:hyperlink r:id=\"rId3\">").count(),
            2
        );
        assert_eq!(writer.rels.len(), 1);
        assert!(
            writer
                .body
                .contains("<w:br/><w:t xml:space=\"preserve\">next</w:t>")
        );
        assert!(
            writer
                .rels()
                .contains("Target=\"https://a.org/\" TargetMode=\"External\"")
        );
    }

    #[test]
    fn blank_paragraphs_are_kept_and_code_is_one_paragraph_per_line() {
        let mut doc = DoclingDocument::new("t");
        doc.add_paragraph("");
        doc.push(Node::Code {
            language: None,
            text: "a\n\nb".into(),
            orig: None,
            pretty: None,
        });
        let body = body_of(&doc);
        assert!(body.starts_with("<w:p/>\n"));
        assert_eq!(body.matches("<w:pStyle w:val=\"SourceCode\"/>").count(), 3);
    }

    #[test]
    fn tables_span_with_grid_span_and_vmerge() {
        let mut doc = DoclingDocument::new("t");
        doc.push(Node::Table(Table {
            rows: vec![
                vec!["A".into(), "B".into(), "C".into()],
                vec!["wide".into(), "wide".into(), "tall".into()],
                vec!["x".into(), "y".into(), "tall".into()],
            ],
            location: None,
            structure: Some(docling_core::TableStructure {
                header_row: vec![true, false, false],
                col_continuation: vec![vec![false; 3], vec![false, true, false], vec![false; 3]],
                row_continuation: vec![vec![false; 3], vec![false; 3], vec![false, false, true]],
                row_header: Vec::new(),
                col_header: Vec::new(),
            }),
            cell_blocks: None,
            caption: None,
            caption_parent: CaptionParent::Body,
            cells: None,
        }));
        let body = body_of(&doc);
        assert_eq!(body.matches("<w:tblHeader/>").count(), 1);
        assert!(body.contains("<w:gridSpan w:val=\"2\"/></w:tcPr><w:p><w:pPr><w:pStyle w:val=\"TableText\"/></w:pPr><w:r><w:t xml:space=\"preserve\">wide</w:t></w:r></w:p>\n</w:tc>"));
        assert!(body.contains("<w:vMerge w:val=\"restart\"/>"));
        assert!(body.contains("<w:vMerge/></w:tcPr><w:p/></w:tc>"));
        assert_eq!(
            body.matches("<w:tc>").count(),
            8,
            "no cell for the spanned-over position"
        );
    }

    #[test]
    fn a_picture_is_a_media_part_with_a_relationship_and_a_content_type() {
        let mut doc = DoclingDocument::new("t");
        doc.push(Node::Picture {
            caption: None,
            caption_href: None,
            image: None,
            classification: None,
            caption_parent: CaptionParent::Body,
        });
        let mut writer = Writer::default();
        writer.nodes(&doc.nodes);
        assert_eq!(writer.media[0].part, "word/media/image1.png");
        assert!(writer.body.contains("<a:blip r:embed=\"rId3\"/>"));
        assert!(writer.rels().contains("Target=\"media/image1.png\""));
        assert!(
            writer
                .content_types()
                .contains("<Default Extension=\"png\" ContentType=\"image/png\"/>")
        );
        assert_eq!(writer.warnings[0].reason, Reason::Placeholder);
    }

    #[test]
    fn a_checkbox_is_a_content_control() {
        let mut doc = DoclingDocument::new("t");
        doc.push(Node::CheckboxItem {
            checked: true,
            text: "done".into(),
        });
        let body = body_of(&doc);
        assert!(body.contains("<w14:checked w14:val=\"1\"/>"));
        assert!(body.contains("<w:t>☑</w:t>"));
        assert!(body.contains("<w:t xml:space=\"preserve\"> done</w:t>"));
    }
}
