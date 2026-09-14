//! The Office Open XML Spreadsheet writer: a `DoclingDocument`'s tables as
//! worksheets, one worksheet per table.
//!
//! The same shape as the ODS writer and for the same reason — a spreadsheet
//! holds cells, so this target writes the tabular nodes and reports every
//! other one as dropped, which `DESIGN.md` §7 settled. The node walk is
//! shared with ODS through [`crate::sheet`]; what differs is the package.
//!
//! The reader is `backend/xlsx.rs`, which scans every worksheet for contiguous
//! rectangular regions by flood fill and emits one `Table` per region, exactly
//! as the ODS reader does. It differs in one way worth having: it wraps each
//! sheet in a `Group` labelled `sheet` and **named for the sheet**, so a
//! caption written as a sheet name comes back here where in ODS it is lost.
//! It also emits a `PageInfo` per sheet, wraps each table in a `Prov`, and
//! trails a `PageBreak` after every sheet but the first; all three are
//! structure this writer does not reproduce, and §6 says so.
//!
//! Strings are written inline (`t="inlineStr"`) rather than through a shared
//! string table, which is a part this package then does not need at all.
//!
//! Author: David M. Anderson
//! Built with AI assistance (Claude, Anthropic)

use docling_core::DoclingDocument;

use crate::Error;
use crate::package::{self, Part};
use crate::report::Output;
use crate::sheet::{self, Sheet};
use crate::xml;

/// Write `doc` as an XLSX package.
pub fn write(doc: &DoclingDocument) -> Result<Output, Error> {
    let (sheets, warnings) = sheets_of(doc);
    let content_types = content_types(&sheets);
    let workbook = workbook(&sheets);
    let workbook_rels = workbook_rels(&sheets);
    let sheet_parts: Vec<(String, String)> = sheets
        .iter()
        .enumerate()
        .map(|(i, s)| (format!("xl/worksheets/sheet{}.xml", i + 1), worksheet(s)))
        .collect();

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
            name: "xl/workbook.xml",
            bytes: workbook.as_bytes(),
            stored: false,
        },
        Part {
            name: "xl/_rels/workbook.xml.rels",
            bytes: workbook_rels.as_bytes(),
            stored: false,
        },
    ];
    for (name, body) in &sheet_parts {
        parts.push(Part {
            name,
            bytes: body.as_bytes(),
            stored: false,
        });
    }
    let bytes = package::zip(&parts)?;
    Ok(Output { bytes, warnings })
}

/// The sheets a workbook holds. Excel caps a sheet name at 31 characters, and
/// refuses a workbook with no sheet at all, so a document that had no table
/// gets one empty sheet rather than none.
fn sheets_of(doc: &DoclingDocument) -> (Vec<Sheet>, Vec<crate::report::Warning>) {
    let (sheets, warnings) = sheet::collect(&doc.nodes, 31);
    let sheets = if sheets.is_empty() {
        vec![Sheet {
            name: String::from("Sheet1"),
            rows: Vec::new(),
            header_rows: 0,
        }]
    } else {
        sheets
    };
    (sheets, warnings)
}

const ROOT_RELS: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/>
</Relationships>
"#;

fn content_types(sheets: &[Sheet]) -> String {
    let mut out = String::from(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
<Default Extension="xml" ContentType="application/xml"/>
<Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/>
"#,
    );
    for i in 1..=sheets.len() {
        out.push_str(&format!(
            "<Override PartName=\"/xl/worksheets/sheet{i}.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml\"/>\n"
        ));
    }
    out.push_str("</Types>\n");
    out
}

fn workbook(sheets: &[Sheet]) -> String {
    let mut out = String::from(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
<sheets>
"#,
    );
    for (i, sheet) in sheets.iter().enumerate() {
        let n = i + 1;
        out.push_str(&format!(
            "<sheet name=\"{}\" sheetId=\"{n}\" r:id=\"rId{n}\"/>\n",
            xml::attr(&sheet.name)
        ));
    }
    out.push_str("</sheets>\n</workbook>\n");
    out
}

fn workbook_rels(sheets: &[Sheet]) -> String {
    let mut out = String::from(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
"#,
    );
    for i in 1..=sheets.len() {
        out.push_str(&format!(
            "<Relationship Id=\"rId{i}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet\" Target=\"worksheets/sheet{i}.xml\"/>\n"
        ));
    }
    out.push_str("</Relationships>\n");
    out
}

/// One worksheet. Cells carry their text inline, so the package needs no
/// shared string table; `xml:space="preserve"` keeps a cell that is only
/// spaces from collapsing to nothing.
fn worksheet(sheet: &Sheet) -> String {
    let mut out = String::from(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
<sheetData>
"#,
    );
    for (r, row) in sheet.rows.iter().enumerate() {
        out.push_str(&format!("<row r=\"{}\">", r + 1));
        for (c, text) in row.iter().enumerate() {
            out.push_str(&format!(
                "<c r=\"{}{}\" t=\"inlineStr\"><is><t xml:space=\"preserve\">{}</t></is></c>",
                column_name(c),
                r + 1,
                xml::text(text)
            ));
        }
        out.push_str("</row>\n");
    }
    out.push_str("</sheetData>\n</worksheet>\n");
    out
}

/// A zero-based column index as its spreadsheet letters: 0 is A, 25 is Z,
/// 26 is AA.
fn column_name(mut index: usize) -> String {
    let mut name = Vec::new();
    loop {
        name.push(b'A' + (index % 26) as u8);
        if index < 26 {
            break;
        }
        index = index / 26 - 1;
    }
    name.reverse();
    String::from_utf8(name).expect("ASCII letters")
}

/// The nodes a workbook has no cell for are reported by the shared walk; this
/// re-exports nothing of its own. See [`crate::sheet`].
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

    #[test]
    fn columns_count_in_letters() {
        assert_eq!(column_name(0), "A");
        assert_eq!(column_name(25), "Z");
        assert_eq!(column_name(26), "AA");
        assert_eq!(column_name(27), "AB");
        assert_eq!(column_name(701), "ZZ");
        assert_eq!(column_name(702), "AAA");
    }

    #[test]
    fn a_table_is_a_worksheet_named_for_its_caption() {
        let mut doc = DoclingDocument::new("t");
        doc.push(Node::Table(table_of(
            vec![vec!["A", "B"], vec!["1", "2"]],
            Some("Quarters"),
        )));
        let (sheets, _) = sheets_of(&doc);
        assert!(workbook(&sheets).contains(r#"<sheet name="Quarters" sheetId="1" r:id="rId1"/>"#));
        // The package assembles, and its parts are deflated rather than
        // readable in the bytes, so the XML is what the assertions read.
        assert!(write(&doc).is_ok());
    }

    #[test]
    fn cells_are_inline_strings_at_their_own_references() {
        let sheet = Sheet {
            name: "S".into(),
            rows: vec![vec!["A".into(), "B".into()], vec!["1".into(), "2".into()]],
            header_rows: 1,
        };
        let xml = worksheet(&sheet);
        assert!(
            xml.contains(r#"<c r="A1" t="inlineStr"><is><t xml:space="preserve">A</t></is></c>"#)
        );
        assert!(xml.contains(r#"<c r="B2" t="inlineStr"#));
        assert_eq!(xml.matches("<row ").count(), 2);
        // No shared string table means no part to keep in step with.
        assert!(!xml.contains("sharedStrings"));
    }

    #[test]
    fn a_document_with_no_table_still_opens() {
        let mut doc = DoclingDocument::new("t");
        doc.add_paragraph("prose only");
        let (sheets, warnings) = sheets_of(&doc);
        // Excel refuses a workbook with no sheet, so there is always one.
        assert_eq!(sheets.len(), 1);
        assert_eq!(sheets[0].name, "Sheet1");
        assert!(sheets[0].rows.is_empty());
        assert_eq!(warnings.len(), 1);
    }
}
