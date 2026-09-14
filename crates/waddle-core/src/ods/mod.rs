//! The OpenDocument Spreadsheet writer: a `DoclingDocument`'s tables as
//! sheets, one sheet per table.
//!
//! A spreadsheet has no place for prose, so this target writes the tabular
//! nodes and reports every other node as dropped. `DESIGN.md` §7 settles that;
//! §6 carries what the ODS reader cannot give back.
//!
//! The reader this writer answers to is `walk_spreadsheet` in docling.rs's
//! `backend/odf.rs`. It reads cell *text* and nothing else: it flood-fills each
//! sheet into its disconnected data regions and emits one `Table` per region,
//! always with `structure: None`. So a header band, a column span and the sheet
//! name itself do not come back, and a blank row inside a table returns as two
//! tables rather than one. This writer emits a plain rectangular grid for that
//! reason — spans and covered cells would not survive and would only confuse
//! the flood fill.
//!
//! Author: David M. Anderson
//! Built with AI assistance (Claude, Anthropic)

use docling_core::{CaptionParent, DoclingDocument, Node, Table};

use crate::Error;
use crate::package::{self, Part};
use crate::report::{Output, Reason, Warning};
use crate::target::Target;
use crate::xml;

const STYLES: &str = include_str!("styles.xml");

/// Write `doc` as an ODS package.
pub fn write(doc: &DoclingDocument) -> Result<Output, Error> {
    let mut writer = Writer::default();
    writer.nodes(&doc.nodes);
    let content = writer.content();
    let parts = vec![
        Part {
            name: "mimetype",
            bytes: Target::Ods.media_type().as_bytes(),
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
            bytes: MANIFEST.as_bytes(),
            stored: false,
        },
    ];
    let bytes = package::zip(&parts)?;
    Ok(Output {
        bytes,
        warnings: writer.warnings,
    })
}

/// Fixed, because an ODS this writer produces has no picture parts: a
/// spreadsheet cell holds text, and a `Picture` node is reported as dropped.
const MANIFEST: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<manifest:manifest xmlns:manifest="urn:oasis:names:tc:opendocument:xmlns:manifest:1.0" manifest:version="1.3">
  <manifest:file-entry manifest:full-path="/" manifest:version="1.3" manifest:media-type="application/vnd.oasis.opendocument.spreadsheet"/>
  <manifest:file-entry manifest:full-path="content.xml" manifest:media-type="text/xml"/>
  <manifest:file-entry manifest:full-path="styles.xml" manifest:media-type="text/xml"/>
</manifest:manifest>
"#;

#[derive(Default)]
struct Writer {
    body: String,
    warnings: Vec<Warning>,
    /// The sheet names already taken, in order, so a repeated caption and a
    /// caption colliding with a generated name both still get a unique name.
    names: Vec<String>,
}

impl Writer {
    fn nodes(&mut self, nodes: &[Node]) {
        for node in nodes {
            self.node(node);
        }
    }

    fn node(&mut self, node: &Node) {
        match node {
            Node::Table(table) => self.sheet(table, table.caption.as_deref()),
            // A chart with its data is that data. One without has no cells to
            // put in a sheet, so it goes the way of the prose.
            Node::Chart { table, caption, .. } if !table.rows.is_empty() => {
                self.sheet(table, caption.as_deref())
            }
            // A field region is a table of keys and values in every other
            // target, and the corpus counts it as one, so it is a sheet here.
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
                self.sheet(
                    &Table {
                        rows,
                        location: None,
                        structure: None,
                        cell_blocks: None,
                        caption: None,
                        caption_parent: CaptionParent::Body,
                        cells: None,
                    },
                    None,
                );
            }
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
            Node::Group { layer, .. } => self.warn(
                node,
                Reason::Layer(layer.as_ref().map_or("furniture", |l| l.value())),
            ),
            // Everything a spreadsheet has no cell for. Named rather than
            // swept up, so a variant docling.rs adds fails the build here.
            Node::Heading { .. }
            | Node::Paragraph { .. }
            | Node::CheckboxItem { .. }
            | Node::ListItem { .. }
            | Node::Code { .. }
            | Node::Picture { .. }
            | Node::Formula { .. }
            | Node::Caption { .. }
            | Node::Chart { .. }
            | Node::InlineGroup { .. }
            | Node::CommentSection { .. }
            | Node::PageFurniture { .. }
            | Node::PageBreak
            | Node::PageInfo { .. }
            | Node::TextDump(_) => self.warn(node, Reason::Unsupported),
        }
    }

    fn warn(&mut self, node: &Node, reason: Reason) {
        self.warnings.push(Warning {
            node: crate::node::kind(node),
            reason,
        });
    }

    /// One sheet, a plain rectangular grid of string cells. Short rows are
    /// padded to the widest, because a ragged row would leave a hole the
    /// reader's flood fill could split the region on.
    fn sheet(&mut self, table: &Table, caption: Option<&str>) {
        let columns = table.rows.iter().map(Vec::len).max().unwrap_or(0);
        if columns == 0 {
            return;
        }
        let name = self.name_for(caption);
        self.body.push_str(&format!(
            "<table:table table:name=\"{}\">\n<table:table-column table:number-columns-repeated=\"{columns}\"/>\n",
            xml::attr(&name)
        ));
        let cells = table.derive_cells();
        let is_header = |r: usize| {
            cells
                .iter()
                .filter(|c| c.start_row == r)
                .all(|c| c.column_header)
        };
        let header_rows = (0..table.rows.len()).take_while(|&r| is_header(r)).count();
        for (r, row) in table.rows.iter().enumerate() {
            self.body.push_str("<table:table-row>");
            for c in 0..columns {
                let text = row.get(c).map_or("", String::as_str);
                let style = if r < header_rows { "Header" } else { "Default" };
                self.body.push_str(&format!(
                    "<table:table-cell table:style-name=\"{style}\" office:value-type=\"string\"><text:p>{}</text:p></table:table-cell>",
                    xml::text(text)
                ));
            }
            self.body.push_str("</table:table-row>\n");
        }
        self.body.push_str("</table:table>\n");
    }

    /// A sheet name: the caption where there is one, else `Sheet<n>`. ODF
    /// requires the name to be unique within the document and forbids a
    /// handful of characters in it; a collision takes a numeric suffix.
    fn name_for(&mut self, caption: Option<&str>) -> String {
        let base = caption
            .map(|c| {
                c.chars()
                    .map(|ch| if "[]*?:/\\'".contains(ch) { ' ' } else { ch })
                    .collect::<String>()
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .filter(|c| !c.is_empty())
            .map(|c| c.chars().take(31).collect::<String>())
            .unwrap_or_else(|| format!("Sheet{}", self.names.len() + 1));
        let mut name = base.clone();
        let mut n = 2;
        while self.names.contains(&name) {
            name = format!("{base} {n}");
            n += 1;
        }
        self.names.push(name.clone());
        name
    }

    fn content(&self) -> String {
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:table="urn:oasis:names:tc:opendocument:xmlns:table:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" office:version="1.3">
<office:body><office:spreadsheet>
{}</office:spreadsheet></office:body></office:document-content>
"#,
            self.body
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table_of(rows: Vec<Vec<&str>>, caption: Option<&str>) -> Table {
        Table {
            rows: rows
                .into_iter()
                .map(|r| r.into_iter().map(String::from).collect())
                .collect(),
            location: None,
            structure: None,
            cell_blocks: None,
            caption: caption.map(String::from),
            caption_parent: CaptionParent::Body,
            cells: None,
        }
    }

    fn content_of(doc: &DoclingDocument) -> String {
        let mut writer = Writer::default();
        writer.nodes(&doc.nodes);
        writer.content()
    }

    #[test]
    fn a_table_is_a_sheet_named_for_its_caption() {
        let mut doc = DoclingDocument::new("t");
        doc.push(Node::Table(table_of(
            vec![vec!["A", "B"], vec!["1", "2"]],
            Some("Table 1: quarters"),
        )));
        let xml = content_of(&doc);
        assert!(xml.contains("<office:spreadsheet>"));
        // ODF forbids [ ] * ? : / \ in a sheet name, so the caption's colon
        // becomes a space and the run of spaces collapses.
        assert!(xml.contains(r#"<table:table table:name="Table 1 quarters">"#));
        assert!(xml.contains(r#"<table:table-column table:number-columns-repeated="2"/>"#));
        assert_eq!(xml.matches("<table:table-row>").count(), 2);
        assert_eq!(xml.matches("<table:table-cell").count(), 4);
        assert!(xml.contains("<text:p>1</text:p>"));
    }

    #[test]
    fn a_sheet_without_a_caption_is_numbered_and_a_repeat_is_suffixed() {
        let mut doc = DoclingDocument::new("t");
        doc.push(Node::Table(table_of(vec![vec!["a"]], None)));
        doc.push(Node::Table(table_of(vec![vec!["b"]], Some("Same"))));
        doc.push(Node::Table(table_of(vec![vec!["c"]], Some("Same"))));
        let xml = content_of(&doc);
        assert!(xml.contains(r#"table:name="Sheet1""#));
        assert!(xml.contains(r#"table:name="Same""#));
        // ODF requires the name to be unique within the document.
        assert!(xml.contains(r#"table:name="Same 2""#));
    }

    #[test]
    fn short_rows_are_padded_so_the_region_stays_one_table() {
        // A ragged row would leave a hole, and the reader's flood fill splits
        // a region on one, giving two tables back where one went in.
        let mut doc = DoclingDocument::new("t");
        doc.push(Node::Table(table_of(
            vec![vec!["A", "B", "C"], vec!["1"]],
            None,
        )));
        let xml = content_of(&doc);
        assert_eq!(xml.matches("<table:table-cell").count(), 6);
    }

    #[test]
    fn prose_is_dropped_and_reported() {
        let mut doc = DoclingDocument::new("t");
        doc.add_heading(2, "Heading");
        doc.add_paragraph("prose");
        doc.push(Node::Table(table_of(vec![vec!["a"]], None)));
        let mut writer = Writer::default();
        writer.nodes(&doc.nodes);
        // The table is the only thing a spreadsheet has a cell for; the rest
        // is reported rather than dropped in silence. DESIGN.md §7.
        assert_eq!(writer.body.matches("<table:table ").count(), 1);
        let kinds: Vec<&str> = writer.warnings.iter().map(|w| w.node).collect();
        assert_eq!(kinds, vec!["heading", "paragraph"]);
        assert!(
            writer
                .warnings
                .iter()
                .all(|w| w.reason == Reason::Unsupported)
        );
    }

    #[test]
    fn a_wrapper_is_written_as_what_it_wraps() {
        let mut doc = DoclingDocument::new("t");
        doc.push(Node::Prov {
            inner: Box::new(Node::Table(table_of(vec![vec!["x"]], None))),
            page_no: 1,
            bbox: [0.0, 0.0, 1.0, 1.0],
            charspan: [0, 0],
            seq: None,
        });
        assert!(content_of(&doc).contains("<text:p>x</text:p>"));
    }

    #[test]
    fn the_package_is_an_ods() {
        let mut doc = DoclingDocument::new("t");
        doc.push(Node::Table(table_of(vec![vec!["a"]], None)));
        let out = write(&doc).unwrap();
        // `mimetype` stored first is what makes an ODF package sniffable.
        assert_eq!(&out.bytes[30..38], b"mimetype");
        assert!(
            String::from_utf8_lossy(&out.bytes)
                .contains("application/vnd.oasis.opendocument.spreadsheet")
        );
    }
}
