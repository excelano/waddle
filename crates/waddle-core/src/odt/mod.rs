//! OpenDocument Text.
//!
//! One `content.xml` written from the nodes, the stock `styles.xml` beside
//! it, the pictures, a manifest, and `mimetype` first and stored. The
//! constructs are the ones docling.rs's ODF reader keys on, so that reading
//! the package back yields the nodes it was written from; `DESIGN.md` §5 is
//! the mapping and §6 the diffs the reader cannot close.
//!
//! Author: David M. Anderson
//! Built with AI assistance (Claude, Anthropic)

use docling_core::{DoclingDocument, Node, PictureImage, Table};

use crate::inline::{
    self, Position, Run, collapse_spaces, is_blank, merge_adjacent, outside_links, plain,
};
use crate::list::Style as ListStyle;
use crate::media::{self, PLACEHOLDER_PNG, PLACEHOLDER_SIZE_IN};
use crate::node::kind;
use crate::package::{self, Part};
use crate::report::{Output, Reason, Warning};
use crate::xml;
use crate::{Error, Target};

mod list;

const STYLES: &str = include_str!("styles.xml");

const CONTENT_HEAD: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" xmlns:table="urn:oasis:names:tc:opendocument:xmlns:table:1.0" xmlns:draw="urn:oasis:names:tc:opendocument:xmlns:drawing:1.0" xmlns:fo="urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0" xmlns:xlink="http://www.w3.org/1999/xlink" xmlns:svg="urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0" office:version="1.3">
<office:font-face-decls>
<style:font-face style:name="Liberation Mono" svg:font-family="'Liberation Mono'" style:font-family-generic="modern" style:font-pitch="fixed"/>
</office:font-face-decls>
"#;

/// The automatic styles every document declares: the page-break paragraph
/// and the table and cell styles.
const FIXED_AUTOMATIC_STYLES: &str = r#"<style:style style:name="Pbreak" style:family="paragraph" style:parent-style-name="Standard"><style:paragraph-properties fo:break-before="page"/></style:style>
<style:style style:name="WaddleTable" style:family="table"><style:table-properties style:width="6.5in" table:align="margins"/></style:style>
<style:style style:name="WaddleCell" style:family="table-cell"><style:table-cell-properties fo:padding="0.097cm" fo:border="0.5pt solid #000000"/></style:style>
"#;

/// ODF's highest outline level.
const MAX_OUTLINE: u8 = 10;

/// Write the document as an ODT package.
pub fn write(doc: &DoclingDocument) -> Result<Output, Error> {
    let mut writer = Writer::default();
    writer.nodes(&doc.nodes);
    let content = writer.content();
    let manifest = writer.manifest();
    let mut parts = vec![
        Part {
            name: "mimetype",
            bytes: Target::Odt.media_type().as_bytes(),
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

/// The formatting of a `text:span`, without the text. One automatic style
/// is declared per distinct value.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Span {
    bold: bool,
    italic: bool,
    underline: bool,
    strike: bool,
    script: Position,
    code: bool,
}

impl Span {
    /// The span a run needs, or none for plain text.
    fn of(run: &Run) -> Option<Span> {
        let span = Span {
            bold: run.bold,
            italic: run.italic,
            underline: run.underline,
            strike: run.strike,
            script: run.script,
            code: run.code,
        };
        let plain = Span {
            bold: false,
            italic: false,
            underline: false,
            strike: false,
            script: Position::Baseline,
            code: false,
        };
        (span != plain).then_some(span)
    }

    fn properties(&self) -> String {
        let mut p = String::new();
        if self.bold {
            p.push_str(r#" fo:font-weight="bold" style:font-weight-asian="bold" style:font-weight-complex="bold""#);
        }
        if self.italic {
            p.push_str(r#" fo:font-style="italic" style:font-style-asian="italic" style:font-style-complex="italic""#);
        }
        if self.underline {
            p.push_str(r#" style:text-underline-style="solid" style:text-underline-width="auto" style:text-underline-color="font-color""#);
        }
        if self.strike {
            p.push_str(
                r#" style:text-line-through-style="solid" style:text-line-through-type="single""#,
            );
        }
        match self.script {
            Position::Baseline => {}
            Position::Sub => p.push_str(r#" style:text-position="sub 58%""#),
            Position::Super => p.push_str(r#" style:text-position="super 58%""#),
        }
        if self.code {
            p.push_str(r#" style:font-name="Liberation Mono""#);
        }
        p
    }
}

/// One image part of the package.
struct Picture {
    name: String,
    media_type: String,
    bytes: Vec<u8>,
}

#[derive(Default)]
pub(super) struct Writer {
    body: String,
    spans: Vec<Span>,
    lists: Vec<ListStyle>,
    pictures: Vec<Picture>,
    tables: usize,
    warnings: Vec<Warning>,
}

impl Writer {
    /// The whole `content.xml`.
    fn content(&self) -> String {
        let mut out = String::from(CONTENT_HEAD);
        out.push_str("<office:automatic-styles>\n");
        out.push_str(FIXED_AUTOMATIC_STYLES);
        for (i, span) in self.spans.iter().enumerate() {
            out.push_str(&format!(
                "<style:style style:name=\"T{}\" style:family=\"text\"><style:text-properties{}/></style:style>\n",
                i + 1,
                span.properties()
            ));
        }
        for (i, style) in self.lists.iter().enumerate() {
            out.push_str(&list::style_xml(style, &list::style_name(i)));
        }
        out.push_str("</office:automatic-styles>\n<office:body>\n<office:text>\n");
        out.push_str(&self.body);
        out.push_str("</office:text>\n</office:body>\n</office:document-content>\n");
        out
    }

    fn manifest(&self) -> String {
        let mut out = String::from(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<manifest:manifest xmlns:manifest="urn:oasis:names:tc:opendocument:xmlns:manifest:1.0" manifest:version="1.3">
  <manifest:file-entry manifest:full-path="/" manifest:version="1.3" manifest:media-type="application/vnd.oasis.opendocument.text"/>
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
            Node::Paragraph { text } => {
                self.paragraph("Text_20_body", &inline::from_markdown(text))
            }
            Node::InlineGroup { runs, md_text, .. } => {
                self.paragraph("Text_20_body", &inline::from_group(md_text, runs))
            }
            Node::CheckboxItem { checked, text } => {
                let mut runs = vec![Run {
                    text: if *checked { "☑ " } else { "☐ " }.to_string(),
                    ..Run::default()
                }];
                runs.extend(inline::from_markdown(text));
                self.paragraph("Text_20_body", &runs);
            }
            // A single item outside a run of them, such as one under a wrapper.
            Node::ListItem { .. } => self.list(std::slice::from_ref(node)),
            Node::Code { text, pretty, .. } => {
                let code = pretty.as_deref().unwrap_or(text);
                self.paragraph("Preformatted_20_Text", &[plain(code)]);
            }
            Node::Table(table) => self.table(table, table.caption.as_deref()),
            Node::Picture { caption, image, .. } => {
                self.picture(caption.as_deref(), image.as_ref())
            }
            Node::Formula { latex, orig, .. } => {
                let source = if latex.is_empty() { orig } else { latex };
                self.paragraph("Preformatted_20_Text", &[plain(source)]);
            }
            // A chart with its data is that data; one without is a picture.
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
                        cells: None,
                    },
                    None,
                );
            }
            Node::TextDump(text) => {
                for block in text.split("\n\n") {
                    self.paragraph("Text_20_body", &[plain(block)]);
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
            | Node::Commented { inner, .. }
            | Node::DoclangOnly(inner) => self.node(inner),
            Node::Furniture { layer, .. } => self.warn(node, Reason::Layer(layer.value())),
            Node::PageBreak => self.body.push_str("<text:p text:style-name=\"Pbreak\"/>\n"),
            // Page geometry has no content to carry and is not worth a warning.
            Node::PageInfo { .. } => {}
            Node::CommentSection { .. } | Node::PageFurniture { .. } => {
                self.warn(node, Reason::Unsupported)
            }
        }
    }

    /// Level 1 is the document title; every other level is the target's
    /// heading one below it. Both readers agree; `DESIGN.md` §5.
    fn heading(&mut self, level: u8, runs: &[Run]) {
        if is_blank(runs) {
            return;
        }
        if level <= 1 {
            self.paragraph("Title", runs);
            return;
        }
        let outline = (level - 1).min(MAX_OUTLINE);
        self.body.push_str(&format!(
            "<text:h text:style-name=\"Heading_20_{outline}\" text:outline-level=\"{outline}\">"
        ));
        self.runs(runs);
        self.body.push_str("</text:h>\n");
    }

    fn paragraph(&mut self, style: &str, runs: &[Run]) {
        if is_blank(runs) {
            return;
        }
        self.body
            .push_str(&format!("<text:p text:style-name=\"{style}\">"));
        self.runs(runs);
        self.body.push_str("</text:p>\n");
    }

    /// The caption, then the table, which is the order docling's Markdown
    /// renders them in.
    fn table(&mut self, table: &Table, caption: Option<&str>) {
        if let Some(caption) = caption {
            self.paragraph("Caption", &inline::from_markdown(caption));
        }
        let columns = table.rows.iter().map(Vec::len).max().unwrap_or(0);
        if columns == 0 {
            return;
        }
        self.tables += 1;
        self.body.push_str(&format!(
            "<table:table table:name=\"Table{}\" table:style-name=\"WaddleTable\">\n<table:table-column table:number-columns-repeated=\"{columns}\"/>\n",
            self.tables
        ));
        let cells = table.derive_cells();
        let is_header = |r: usize| {
            cells
                .iter()
                .filter(|c| c.start_row == r)
                .all(|c| c.column_header)
        };
        // The header band is the leading rows the structure marks as header;
        // ODF wants them contiguous at the top.
        let header_rows = (0..table.rows.len()).take_while(|&r| is_header(r)).count();
        if header_rows > 0 {
            self.body.push_str("<table:table-header-rows>\n");
        }
        for r in 0..table.rows.len() {
            self.body.push_str("<table:table-row>");
            for c in 0..columns {
                let anchor = cells
                    .iter()
                    .find(|cell| cell.start_row == r && cell.start_col == c);
                let Some(cell) = anchor else {
                    self.body.push_str("<table:covered-table-cell/>");
                    continue;
                };
                let mut attrs = String::from(r#" table:style-name="WaddleCell""#);
                if cell.col_span > 1 {
                    attrs.push_str(&format!(
                        " table:number-columns-spanned=\"{}\"",
                        cell.col_span
                    ));
                }
                if cell.row_span > 1 {
                    attrs.push_str(&format!(" table:number-rows-spanned=\"{}\"", cell.row_span));
                }
                let blocks = table
                    .cell_blocks
                    .as_ref()
                    .and_then(|b| b.get(r))
                    .and_then(|row| row.get(c))
                    .filter(|blocks| !blocks.is_empty());
                // A typed cell reads back flat; an untyped one keeps its runs
                // and links, which the reader calls rich. Formatting is only
                // worth keeping where there is some.
                let runs = blocks.map_or_else(|| inline::from_markdown(&cell.text), |_| Vec::new());
                let plain_cell = blocks.is_none()
                    && runs
                        .iter()
                        .all(|r| Span::of(r).is_none() && r.href.is_none());
                if plain_cell {
                    attrs.push_str(r#" office:value-type="string""#);
                }
                self.body.push_str(&format!("<table:table-cell{attrs}>"));
                match blocks {
                    Some(blocks) => self.nodes(blocks),
                    None => {
                        let style = if r < header_rows {
                            "Table_20_Heading"
                        } else {
                            "Table_20_Contents"
                        };
                        if is_blank(&runs) {
                            self.body.push_str("<text:p/>");
                        } else {
                            self.paragraph(style, &runs);
                        }
                    }
                }
                self.body.push_str("</table:table-cell>");
            }
            self.body.push_str("</table:table-row>\n");
            if r + 1 == header_rows {
                self.body.push_str("</table:table-header-rows>\n");
            }
        }
        self.body.push_str("</table:table>\n");
    }

    /// The caption, then the picture in its own paragraph. A picture without
    /// bytes is written as the grey placeholder and reported.
    fn picture(&mut self, caption: Option<&str>, image: Option<&PictureImage>) {
        if let Some(caption) = caption {
            self.paragraph("Caption", &inline::from_markdown(caption));
        }
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
        self.pictures.push(Picture {
            name: name.clone(),
            media_type: media_type.to_string(),
            bytes: bytes.to_vec(),
        });
        self.body.push_str(&format!(
            "<text:p text:style-name=\"Text_20_body\"><draw:frame draw:name=\"Image{index}\" text:anchor-type=\"as-char\" svg:width=\"{width_in:.3}in\" svg:height=\"{height_in:.3}in\" draw:z-index=\"0\"><draw:image xlink:href=\"{}\" xlink:type=\"simple\" xlink:show=\"embed\" xlink:actuate=\"onLoad\" draw:mime-type=\"{}\"/></draw:frame></text:p>\n",
            xml::attr(&name),
            xml::attr(media_type)
        ));
    }

    /// The runs of one paragraph. Consecutive runs with the same link share
    /// one `text:a`.
    pub(super) fn runs(&mut self, runs: &[Run]) {
        let runs = merge_adjacent(outside_links(runs));
        let mut at_start = true;
        let mut open_href: Option<&str> = None;
        for run in &runs {
            if run.href.as_deref() != open_href {
                if open_href.is_some() {
                    self.body.push_str("</text:a>");
                }
                open_href = run.href.as_deref();
                if let Some(href) = open_href {
                    self.body.push_str(&format!(
                        "<text:a xlink:type=\"simple\" xlink:href=\"{}\" text:style-name=\"Internet_20_link\">",
                        xml::attr(href)
                    ));
                }
            }
            match Span::of(run) {
                Some(span) => {
                    let name = self.span_name(span);
                    self.body
                        .push_str(&format!("<text:span text:style-name=\"{name}\">"));
                    self.text(&run.text, &mut at_start);
                    self.body.push_str("</text:span>");
                }
                None => self.text(&run.text, &mut at_start),
            }
        }
        if open_href.is_some() {
            self.body.push_str("</text:a>");
        }
    }

    fn span_name(&mut self, span: Span) -> String {
        let index = match self.spans.iter().position(|s| *s == span) {
            Some(i) => i,
            None => {
                self.spans.push(span);
                self.spans.len() - 1
            }
        };
        format!("T{}", index + 1)
    }

    /// Character data with ODF's whitespace rules: a newline is a line
    /// break, a tab is a tab, and a run of spaces, or any space at the start
    /// of a line, is counted in `text:s` because the format collapses it
    /// otherwise.
    fn text(&mut self, s: &str, at_start: &mut bool) {
        let mut spaces = 0usize;
        for c in s.chars() {
            if c == ' ' {
                spaces += 1;
                continue;
            }
            self.spaces(spaces, *at_start);
            spaces = 0;
            match c {
                '\n' => {
                    self.body.push_str("<text:line-break/>");
                    *at_start = true;
                }
                '\t' => {
                    self.body.push_str("<text:tab/>");
                    *at_start = false;
                }
                _ => {
                    self.body.push_str(&xml::text(&c.to_string()));
                    *at_start = false;
                }
            }
        }
        self.spaces(spaces, *at_start);
        if spaces > 0 {
            *at_start = false;
        }
    }

    fn spaces(&mut self, count: usize, at_start: bool) {
        if count == 0 {
            return;
        }
        if at_start {
            self.body.push_str(&format!("<text:s text:c=\"{count}\"/>"));
        } else {
            self.body.push(' ');
            if count > 1 {
                self.body
                    .push_str(&format!("<text:s text:c=\"{}\"/>", count - 1));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn content_of(doc: &DoclingDocument) -> String {
        let mut writer = Writer::default();
        writer.nodes(&doc.nodes);
        writer.content()
    }

    #[test]
    fn title_and_headings_follow_the_reader_convention() {
        let mut doc = DoclingDocument::new("t");
        doc.add_heading(1, "Report");
        doc.add_heading(2, "Intro");
        let xml = content_of(&doc);
        assert!(xml.contains(r#"<text:p text:style-name="Title">Report</text:p>"#));
        assert!(xml.contains(
            r#"<text:h text:style-name="Heading_20_1" text:outline-level="1">Intro</text:h>"#
        ));
    }

    #[test]
    fn spans_are_declared_once_per_formatting() {
        let mut doc = DoclingDocument::new("t");
        doc.add_paragraph("**a** and **b** and *c*");
        let xml = content_of(&doc);
        assert_eq!(xml.matches("<style:style style:name=\"T").count(), 2);
        assert!(xml.contains(r#"<text:span text:style-name="T1">a</text:span>"#));
        assert!(xml.contains(r#"<text:span text:style-name="T1">b</text:span>"#));
        assert!(xml.contains(r#"<text:span text:style-name="T2">c</text:span>"#));
    }

    #[test]
    fn whitespace_is_counted_and_breaks_are_elements() {
        let mut doc = DoclingDocument::new("t");
        doc.add_paragraph("a  b\n c\td");
        let xml = content_of(&doc);
        assert!(xml.contains(
            r#"a <text:s text:c="1"/>b<text:line-break/><text:s text:c="1"/>c<text:tab/>d"#
        ));
    }

    #[test]
    fn unsupported_nodes_warn_and_layers_drop() {
        let mut doc = DoclingDocument::new("t");
        doc.push(Node::PageFurniture {
            footer: true,
            location: [0, 500, 511, 511],
            text: "page 1".into(),
        });
        doc.push(Node::Furniture {
            layer: docling_core::ContentLayer::Furniture,
            inner: Box::new(Node::Paragraph {
                text: "running head".into(),
            }),
        });
        let mut writer = Writer::default();
        writer.nodes(&doc.nodes);
        assert_eq!(
            writer.warnings,
            vec![
                Warning {
                    node: "page_furniture",
                    reason: Reason::Unsupported
                },
                Warning {
                    node: "furniture",
                    reason: Reason::Layer("furniture")
                },
            ]
        );
        assert!(!writer.body.contains("running head"));
    }

    #[test]
    fn a_table_has_a_header_band_spans_and_covered_cells() {
        let mut doc = DoclingDocument::new("t");
        doc.push(Node::Table(Table {
            rows: vec![
                vec!["A".into(), "B".into(), "C".into()],
                vec!["wide".into(), "wide".into(), "c".into()],
            ],
            location: None,
            structure: Some(docling_core::TableStructure {
                header_row: vec![true, false],
                col_continuation: vec![vec![false; 3], vec![false, true, false]],
                row_continuation: vec![vec![false; 3], vec![false; 3]],
                row_header: Vec::new(),
                col_header: Vec::new(),
            }),
            cell_blocks: None,
            caption: Some("Table 1: things".into()),
            cells: None,
        }));
        let xml = content_of(&doc);
        assert!(xml.contains(r#"<text:p text:style-name="Caption">Table 1: things</text:p>"#));
        assert!(xml.contains("<table:table-header-rows>"));
        assert!(xml.contains(
            "table:number-columns-spanned=\"2\" office:value-type=\"string\"><text:p text:style-name=\"Table_20_Contents\">wide</text:p>\n</table:table-cell><table:covered-table-cell/>"
        ));
        assert!(xml.contains(r#"<text:p text:style-name="Table_20_Heading">A</text:p>"#));
    }

    #[test]
    fn a_picture_without_bytes_is_a_placeholder_part() {
        let mut doc = DoclingDocument::new("t");
        doc.push(Node::Picture {
            caption: Some("Figure 1".into()),
            caption_href: None,
            image: None,
            classification: None,
        });
        let mut writer = Writer::default();
        writer.nodes(&doc.nodes);
        assert_eq!(writer.pictures.len(), 1);
        assert_eq!(writer.pictures[0].name, "Pictures/image1.png");
        assert_eq!(writer.warnings[0].reason, Reason::Placeholder);
        assert!(writer.manifest().contains("Pictures/image1.png"));
        assert!(
            writer
                .body
                .contains(r#"<text:p text:style-name="Caption">Figure 1</text:p>"#)
        );
    }

    #[test]
    fn code_is_one_preformatted_paragraph_with_line_breaks() {
        let mut doc = DoclingDocument::new("t");
        doc.push(Node::Code {
            language: Some("rust".into()),
            text: "fn a() {\n    *b*\n}".into(),
            orig: None,
            pretty: None,
        });
        let xml = content_of(&doc);
        assert!(xml.contains(
            r#"<text:p text:style-name="Preformatted_20_Text">fn a() {<text:line-break/><text:s text:c="4"/>*b*<text:line-break/>}</text:p>"#
        ));
    }
}
