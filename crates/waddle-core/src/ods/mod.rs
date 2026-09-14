//! The OpenDocument Spreadsheet writer: a `DoclingDocument`'s tables as
//! sheets, one sheet per table.
//!
//! A spreadsheet has no place for prose, so this target writes the tabular
//! nodes and reports every other node as dropped. `DESIGN.md` §7 settles that;
//! §6 carries what the ODS reader cannot give back. The node walk is shared
//! with XLSX in [`crate::sheet`]; what lives here is the package and its XML.
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

use docling_core::DoclingDocument;

use crate::Error;
use crate::package::{self, Part};
use crate::report::Output;
use crate::sheet::{self, Sheet};
use crate::target::Target;
use crate::xml;

const STYLES: &str = include_str!("styles.xml");

/// Write `doc` as an ODS package.
pub fn write(doc: &DoclingDocument) -> Result<Output, Error> {
    // ODF caps a sheet name at 127 characters where Excel stops at 31.
    let (sheets, warnings) = sheet::collect(&doc.nodes, 127);
    let content = content(&sheets);
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
    Ok(Output { bytes, warnings })
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

/// The sheets as `table:table` elements. The header band takes a bold cell
/// style for the sake of someone opening the file; the reader returns every
/// table with `structure: None`, so it is not the round trip's.
fn content(sheets: &[Sheet]) -> String {
    let mut body = String::new();
    for sheet in sheets {
        let columns = sheet.rows.first().map_or(0, Vec::len);
        body.push_str(&format!(
            "<table:table table:name=\"{}\">\n<table:table-column table:number-columns-repeated=\"{columns}\"/>\n",
            xml::attr(&sheet.name)
        ));
        for (r, row) in sheet.rows.iter().enumerate() {
            body.push_str("<table:table-row>");
            for text in row {
                let style = if r < sheet.header_rows {
                    "Header"
                } else {
                    "Default"
                };
                body.push_str(&format!(
                    "<table:table-cell table:style-name=\"{style}\" office:value-type=\"string\"><text:p>{}</text:p></table:table-cell>",
                    xml::text(text)
                ));
            }
            body.push_str("</table:table-row>\n");
        }
        body.push_str("</table:table>\n");
    }
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:table="urn:oasis:names:tc:opendocument:xmlns:table:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" office:version="1.3">
<office:body><office:spreadsheet>
{body}</office:spreadsheet></office:body></office:document-content>
"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use docling_core::{CaptionParent, Node, Table};

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
        content(&sheet::collect(&doc.nodes, 127).0)
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
        let (sheets, warnings) = sheet::collect(&doc.nodes, 127);
        // The table is the only thing a spreadsheet has a cell for; the rest
        // is reported rather than dropped in silence. DESIGN.md §7.
        assert_eq!(sheets.len(), 1);
        let kinds: Vec<&str> = warnings.iter().map(|w| w.node).collect();
        assert_eq!(kinds, vec!["heading", "paragraph"]);
        assert!(
            warnings
                .iter()
                .all(|w| w.reason == crate::report::Reason::Unsupported)
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
