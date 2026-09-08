//! The conformance test: write a package, read it back with docling.rs's own
//! reader, and compare. `DESIGN.md` §6.
//!
//! Three comparisons, weakest to strongest. The Markdown exports agree once
//! whitespace is set aside, because the ODF reader rebuilds a paragraph's
//! Markdown from its runs with docling's own spacing rules. The structured
//! runs come back with their text and formatting. And a second trip is a
//! fixed point: what the reader produced, written and read again, is equal.
//!
//! When LibreOffice is installed, every package is also opened by `soffice`
//! and converted to PDF, which is the cheapest proof that a package loads
//! without a repair prompt. Without it that check is skipped and says so.
//!
//! Author: David M. Anderson
//! Built with AI assistance (Claude, Anthropic)

use std::path::Path;
use std::process::Command;

use docling::{DocumentConverter, InputFormat, SourceDocument};
use docling_core::{DoclingDocument, InlineRun, Node, Script, inline_paragraph_node};
use waddle_core::{Output, Target, docx, odt};

fn read_back(target: Target, bytes: Vec<u8>) -> DoclingDocument {
    let format = match target {
        Target::Odt => InputFormat::Odt,
        Target::Docx => InputFormat::Docx,
    };
    let source = SourceDocument::from_bytes(format!("t.{target}"), format, bytes);
    DocumentConverter::new()
        .convert(source)
        .expect("docling reads the package back")
        .document
}

/// With `WADDLE_DUMP=/some/dir`, every package written here is also saved
/// there, named by the test, so it can be opened in Writer by hand.
fn trip(target: Target, doc: &DoclingDocument) -> (DoclingDocument, Output) {
    let out = match target {
        Target::Odt => odt::write(doc),
        Target::Docx => docx::write(doc),
    }
    .expect("the package writes");
    let name = format!(
        "{}.{target}",
        std::thread::current()
            .name()
            .unwrap_or("test")
            .replace("::", "_")
    );
    if let Some(dir) = std::env::var_os("WADDLE_DUMP") {
        std::fs::write(Path::new(&dir).join(&name), &out.bytes)
            .expect("dump directory is writable");
    }
    assert_opens_in_writer(&name, &out.bytes);
    (read_back(target, out.bytes.clone()), out)
}

fn odt_trip(doc: &DoclingDocument) -> (DoclingDocument, Output) {
    trip(Target::Odt, doc)
}

fn docx_trip(doc: &DoclingDocument) -> (DoclingDocument, Output) {
    trip(Target::Docx, doc)
}

/// Convert the package to PDF with headless LibreOffice, each call in its
/// own profile directory so parallel tests do not share one instance.
/// `name` carries the package's extension.
fn assert_opens_in_writer(name: &str, bytes: &[u8]) {
    if Command::new("soffice").arg("--version").output().is_err() {
        eprintln!("soffice not installed; skipping the Writer check for {name}");
        return;
    }
    let dir = std::env::temp_dir().join(format!("waddle-writer-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    let odt = dir.join(name);
    std::fs::write(&odt, bytes).expect("temp package");
    let result = Command::new("soffice")
        .arg("--headless")
        .arg(format!(
            "-env:UserInstallation=file://{}",
            dir.join("profile").display()
        ))
        .arg("--convert-to")
        .arg("pdf")
        .arg("--outdir")
        .arg(&dir)
        .arg(&odt)
        .output()
        .expect("soffice runs");
    let pdf = dir.join(name).with_extension("pdf");
    assert!(
        result.status.success() && pdf.is_file(),
        "Writer did not convert {name}: {}{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

fn squeezed(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}

fn assert_fixed_point(back: &DoclingDocument) {
    assert_fixed_point_for(Target::Odt, back);
}

fn assert_fixed_point_for(target: Target, back: &DoclingDocument) {
    let (again, _) = trip(target, back);
    assert_eq!(
        again.nodes, back.nodes,
        "a second {target} trip changes the document"
    );
}

#[test]
fn headings_and_a_markdown_paragraph() {
    let mut doc = DoclingDocument::new("t");
    doc.add_heading(1, "Report");
    doc.add_heading(2, "Intro");
    doc.add_heading(3, "Detail");
    doc.add_paragraph(
        r"See [the spec](https://example.com/spec) for **details**, *emphasis* and a\_name &amp; more.",
    );
    let (back, out) = odt_trip(&doc);
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);

    assert_eq!(
        squeezed(&back.export_to_markdown()),
        squeezed(&doc.export_to_markdown())
    );
    assert_eq!(
        back.nodes[0],
        Node::Heading {
            level: 1,
            text: "Report".into()
        }
    );
    assert_eq!(
        back.nodes[1],
        Node::Heading {
            level: 2,
            text: "Intro".into()
        }
    );
    assert_eq!(
        back.nodes[2],
        Node::Heading {
            level: 3,
            text: "Detail".into()
        }
    );
    let Node::InlineGroup { runs, .. } = &back.nodes[3] else {
        panic!(
            "formatted paragraph reads back as runs, got {:?}",
            back.nodes[3]
        );
    };
    let texts: Vec<(&str, bool, bool)> = runs
        .iter()
        .map(|r| (r.text.as_str(), r.bold, r.italic))
        .collect();
    assert_eq!(
        texts,
        vec![
            ("See ", false, false),
            ("the spec", false, false),
            (" for ", false, false),
            ("details", true, false),
            (", ", false, false),
            ("emphasis", false, true),
            (" and a_name & more.", false, false),
        ]
    );
    assert_fixed_point(&back);
}

#[test]
fn structured_runs_keep_underline_and_script_and_their_link() {
    let runs = vec![
        InlineRun {
            text: "H".into(),
            ..Default::default()
        },
        InlineRun {
            text: "2".into(),
            script: Script::Sub,
            ..Default::default()
        },
        InlineRun {
            text: "O is ".into(),
            ..Default::default()
        },
        InlineRun {
            text: "underlined".into(),
            underline: true,
            ..Default::default()
        },
        InlineRun {
            text: " and ".into(),
            ..Default::default()
        },
        InlineRun {
            text: "struck".into(),
            strike: true,
            ..Default::default()
        },
        InlineRun {
            text: " here".into(),
            ..Default::default()
        },
    ];
    let md = "H2O is underlined and ~~struck~~ [here](https://example.com/)".to_string();
    let mut doc = DoclingDocument::new("t");
    doc.push(inline_paragraph_node(md, runs.clone(), false));
    let (back, _) = odt_trip(&doc);
    let Node::InlineGroup {
        runs: got, md_text, ..
    } = &back.nodes[0]
    else {
        panic!("got {:?}", back.nodes[0]);
    };
    let shape: Vec<(&str, Script, bool, bool)> = got
        .iter()
        .map(|r| (r.text.as_str(), r.script, r.underline, r.strike))
        .collect();
    assert_eq!(
        shape,
        vec![
            ("H", Script::Baseline, false, false),
            ("2", Script::Sub, false, false),
            ("O is ", Script::Baseline, false, false),
            ("underlined", Script::Baseline, true, false),
            (" and ", Script::Baseline, false, false),
            ("struck", Script::Baseline, false, true),
            (" ", Script::Baseline, false, false),
            ("here", Script::Baseline, false, false),
        ]
    );
    assert!(
        md_text.contains("[here](https://example.com/)"),
        "{md_text}"
    );
    assert_fixed_point(&back);
}

#[test]
fn wrappers_are_transparent_and_layers_are_dropped() {
    let mut doc = DoclingDocument::new("t");
    doc.push(Node::Group {
        label: "section".into(),
        name: None,
        layer: None,
        children: vec![Node::Located {
            location: [0, 0, 10, 10],
            inner: Box::new(Node::Paragraph {
                text: "kept".into(),
            }),
        }],
    });
    doc.push(Node::Furniture {
        layer: docling_core::ContentLayer::Furniture,
        inner: Box::new(Node::Paragraph {
            text: "running head".into(),
        }),
    });
    let (back, out) = odt_trip(&doc);
    assert_eq!(
        back.nodes,
        vec![Node::Paragraph {
            text: "kept".into()
        }]
    );
    assert_eq!(out.warnings.len(), 1);
    assert_eq!(out.warnings[0].node, "furniture");
}

fn list_item(ordered: bool, number: u64, first: bool, level: u8, text: &str) -> Node {
    Node::ListItem {
        ordered,
        number,
        first_in_list: first,
        text: text.into(),
        level,
        marker: None,
        location: None,
        dclx: None,
        href: None,
        layer: None,
    }
}

#[test]
fn lists_come_back_nested_numbered_and_started() {
    let mut doc = DoclingDocument::new("t");
    doc.push(list_item(true, 3, true, 0, "three **bold**"));
    doc.push(list_item(true, 4, false, 0, "four"));
    doc.push(list_item(false, 0, false, 1, "a bullet under four"));
    doc.push(list_item(false, 0, false, 1, "another"));
    doc.push(list_item(true, 5, false, 0, "five"));
    doc.push(list_item(false, 0, true, 0, "a new bullet list"));
    let (back, out) = odt_trip(&doc);
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    type Shape<'a> = (bool, u64, bool, u8, &'a str, Option<&'a str>);
    let shape: Vec<Shape> = back
        .nodes
        .iter()
        .map(|n| match n {
            Node::ListItem {
                ordered,
                number,
                first_in_list,
                level,
                text,
                marker,
                ..
            } => (
                *ordered,
                *number,
                *first_in_list,
                *level,
                text.as_str(),
                marker.as_deref(),
            ),
            other => panic!("not a list item: {other:?}"),
        })
        .collect();
    assert_eq!(
        shape,
        vec![
            // Two spaces: the reader joins a list item's runs with a space
            // beside the one that is there (DESIGN.md §6).
            (true, 3, true, 0, "three  **bold**", Some("3.")),
            (true, 4, false, 0, "four", Some("4.")),
            (false, 0, true, 1, "a bullet under four", None),
            (false, 0, false, 1, "another", None),
            (true, 5, false, 0, "five", Some("5.")),
            (false, 0, true, 0, "a new bullet list", None),
        ]
    );
    assert_fixed_point(&back);
}

#[test]
fn a_table_keeps_its_grid_spans_and_caption_text() {
    let mut doc = DoclingDocument::new("t");
    doc.push(Node::Table(docling_core::Table {
        rows: vec![
            vec!["Name".into(), "Q1".into(), "Q2".into()],
            vec!["Total".into(), "both".into(), "both".into()],
            vec!["Tall".into(), "1".into(), "2".into()],
            vec!["Tall".into(), "3".into(), "4".into()],
        ],
        location: None,
        structure: Some(docling_core::TableStructure {
            header_row: vec![true, false, false, false],
            col_continuation: vec![
                vec![false; 3],
                vec![false, false, true],
                vec![false; 3],
                vec![false; 3],
            ],
            row_continuation: vec![
                vec![false; 3],
                vec![false; 3],
                vec![false; 3],
                vec![true, false, false],
            ],
            row_header: Vec::new(),
            col_header: Vec::new(),
        }),
        cell_blocks: None,
        caption: Some("Table 1: quarters".into()),
        cells: None,
    }));
    let (back, out) = odt_trip(&doc);
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    assert_eq!(
        back.nodes[0],
        Node::Paragraph {
            text: "Table 1: quarters".into()
        },
        "the caption reads back as a paragraph before the table (DESIGN.md §6)"
    );
    let Node::Table(table) = &back.nodes[1] else {
        panic!("{:?}", back.nodes[1]);
    };
    assert_eq!(table.rows, doc_table(&doc).rows);
    let structure = table.structure.as_ref().expect("structure");
    assert_eq!(structure.col_continuation[1], vec![false, false, true]);
    assert_eq!(structure.row_continuation[3], vec![true, false, false]);
    assert_eq!(structure.header_row, vec![true, false, false, false]);
    assert_fixed_point(&back);
}

fn doc_table(doc: &DoclingDocument) -> &docling_core::Table {
    match &doc.nodes[0] {
        Node::Table(t) => t,
        other => panic!("{other:?}"),
    }
}

#[test]
fn pictures_with_and_without_bytes_come_back_as_pictures() {
    let mut doc = DoclingDocument::new("t");
    doc.push(Node::Picture {
        caption: Some("Figure 1: a grey box".into()),
        caption_href: None,
        image: None,
        classification: None,
    });
    // The placeholder the writer embeds is a valid PNG; take it from the
    // writer's own output to serve as real bytes for the second picture.
    let png = {
        let out = odt::write(&doc).unwrap();
        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(out.bytes)).unwrap();
        let mut file = archive.by_name("Pictures/image1.png").unwrap();
        let mut bytes = Vec::new();
        std::io::Read::read_to_end(&mut file, &mut bytes).unwrap();
        bytes
    };
    doc.push(Node::Picture {
        caption: None,
        caption_href: None,
        image: Some(docling_core::PictureImage {
            mimetype: "image/png".into(),
            width: 16,
            height: 12,
            data: png,
        }),
        classification: None,
    });
    doc.add_paragraph("after");
    let (back, out) = odt_trip(&doc);
    assert_eq!(out.warnings.len(), 1, "{:?}", out.warnings);
    assert_eq!(
        out.warnings[0].to_string(),
        "picture: written as a placeholder"
    );
    let kinds: Vec<&str> = back.nodes.iter().map(waddle_core::node::kind).collect();
    assert_eq!(kinds, vec!["paragraph", "picture", "picture", "paragraph"]);
    assert_fixed_point(&back);
}

#[test]
fn code_checkbox_and_page_break_degrade_as_documented() {
    let mut doc = DoclingDocument::new("t");
    doc.push(Node::Code {
        language: None,
        text: "fn main() {\n    println!(\"hi\");\n}".into(),
        orig: None,
        pretty: None,
    });
    doc.push(Node::CheckboxItem {
        checked: true,
        text: "done".into(),
    });
    doc.push(Node::PageBreak);
    doc.add_paragraph("next page");
    let (back, out) = odt_trip(&doc);
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    assert_eq!(
        back.nodes,
        vec![
            Node::Paragraph {
                text: "fn main() {\n    println!(\"hi\");\n}".into()
            },
            Node::Paragraph {
                text: "☑ done".into()
            },
            Node::Paragraph {
                text: "next page".into()
            },
        ]
    );
    assert_fixed_point(&back);
}

/// The DOCX reader trims every run and keeps formatting, links aside, so
/// the checks below read its shape rather than the ODF reader's.
#[test]
fn docx_headings_and_a_markdown_paragraph() {
    let mut doc = DoclingDocument::new("t");
    doc.add_heading(1, "Report");
    doc.add_heading(2, "Intro");
    doc.add_heading(3, "Detail");
    doc.add_paragraph(
        r"See [the spec](https://example.com/spec) for **details**, *emphasis* and a\_name &amp; more.",
    );
    let (back, out) = docx_trip(&doc);
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    assert_eq!(
        squeezed(&back.export_to_markdown()),
        squeezed(&doc.export_to_markdown())
    );
    assert_eq!(
        back.nodes[..3],
        [
            Node::Heading {
                level: 1,
                text: "Report".into()
            },
            Node::Heading {
                level: 2,
                text: "Intro".into()
            },
            Node::Heading {
                level: 3,
                text: "Detail".into()
            },
        ]
    );
    let Node::InlineGroup { runs, md_text, .. } = &back.nodes[3] else {
        panic!("{:?}", back.nodes[3]);
    };
    assert!(
        md_text.contains("[the spec](https://example.com/spec)"),
        "{md_text}"
    );
    assert!(runs.iter().any(|r| r.text == "details" && r.bold));
    assert!(runs.iter().any(|r| r.text == "emphasis" && r.italic));
    assert_fixed_point_for(Target::Docx, &back);
}

#[test]
fn docx_structured_runs_keep_underline_and_script() {
    let runs = vec![
        InlineRun {
            text: "H".into(),
            ..Default::default()
        },
        InlineRun {
            text: "2".into(),
            script: Script::Sub,
            ..Default::default()
        },
        InlineRun {
            text: "O is ".into(),
            ..Default::default()
        },
        InlineRun {
            text: "underlined".into(),
            underline: true,
            ..Default::default()
        },
        InlineRun {
            text: " and ".into(),
            ..Default::default()
        },
        InlineRun {
            text: "struck".into(),
            strike: true,
            ..Default::default()
        },
    ];
    let md = "H2O is underlined and ~~struck~~".to_string();
    let mut doc = DoclingDocument::new("t");
    doc.push(inline_paragraph_node(md, runs, false));
    let (back, _) = docx_trip(&doc);
    let Node::InlineGroup { runs: got, .. } = &back.nodes[0] else {
        panic!("{:?}", back.nodes[0]);
    };
    let shape: Vec<(&str, Script, bool, bool)> = got
        .iter()
        .map(|r| (r.text.as_str(), r.script, r.underline, r.strike))
        .collect();
    assert_eq!(
        shape,
        vec![
            ("H", Script::Baseline, false, false),
            ("2", Script::Sub, false, false),
            ("O is", Script::Baseline, false, false),
            ("underlined", Script::Baseline, true, false),
            ("and", Script::Baseline, false, false),
            ("struck", Script::Baseline, false, true),
        ]
    );
    assert_fixed_point_for(Target::Docx, &back);
}

#[test]
fn docx_lists_come_back_numbered_nested_and_started() {
    let mut doc = DoclingDocument::new("t");
    doc.push(list_item(true, 3, true, 0, "three **bold**"));
    doc.push(list_item(true, 4, false, 0, "four"));
    doc.push(list_item(false, 0, false, 1, "a bullet under four"));
    doc.push(list_item(false, 0, false, 1, "another"));
    doc.push(list_item(true, 5, false, 0, "five"));
    doc.push(list_item(false, 0, true, 0, "a new bullet list"));
    let (back, out) = docx_trip(&doc);
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    let shape: Vec<(bool, u64, u8, &str, Option<&str>)> = back
        .nodes
        .iter()
        .map(|n| match n {
            Node::ListItem {
                ordered,
                number,
                level,
                text,
                marker,
                ..
            } => (*ordered, *number, *level, text.as_str(), marker.as_deref()),
            other => panic!("not a list item: {other:?}"),
        })
        .collect();
    assert_eq!(
        shape,
        vec![
            (true, 3, 0, "three **bold**", Some("3.")),
            (true, 4, 0, "four", Some("4.")),
            (false, 0, 1, "a bullet under four", None),
            (false, 0, 1, "another", None),
            (true, 5, 0, "five", Some("5.")),
            (false, 0, 0, "a new bullet list", None),
        ]
    );
    assert_fixed_point_for(Target::Docx, &back);
}

#[test]
fn docx_tables_pictures_code_and_checkboxes() {
    let mut doc = DoclingDocument::new("t");
    doc.push(Node::Table(docling_core::Table {
        rows: vec![
            vec!["Name".into(), "Q1".into(), "Q2".into()],
            vec!["Total".into(), "both".into(), "both".into()],
            vec!["Tall".into(), "1".into(), "2".into()],
            vec!["Tall".into(), "3".into(), "4".into()],
        ],
        location: None,
        structure: Some(docling_core::TableStructure {
            header_row: vec![true, false, false, false],
            col_continuation: vec![
                vec![false; 3],
                vec![false, false, true],
                vec![false; 3],
                vec![false; 3],
            ],
            row_continuation: vec![
                vec![false; 3],
                vec![false; 3],
                vec![false; 3],
                vec![true, false, false],
            ],
            row_header: Vec::new(),
            col_header: Vec::new(),
        }),
        cell_blocks: None,
        caption: Some("Table 1: quarters".into()),
        cells: None,
    }));
    doc.push(Node::Picture {
        caption: Some("Figure 1".into()),
        caption_href: None,
        image: None,
        classification: None,
    });
    doc.push(Node::Code {
        language: None,
        text: "fn main() {\n    println!(\"hi\");\n}".into(),
        orig: None,
        pretty: None,
    });
    doc.push(Node::CheckboxItem {
        checked: true,
        text: "done".into(),
    });
    doc.push(Node::PageBreak);
    doc.add_paragraph("next page");
    let (back, out) = docx_trip(&doc);
    assert_eq!(out.warnings.len(), 1, "{:?}", out.warnings);
    let kinds: Vec<&str> = back.nodes.iter().map(waddle_core::node::kind).collect();
    assert_eq!(
        kinds,
        vec![
            "paragraph",
            "table",
            "paragraph",
            "picture",
            "code",
            "checkbox_item",
            "paragraph",
            "paragraph"
        ],
        "caption, table, caption, picture, code, checkbox, the page break as an empty paragraph, text"
    );
    let Node::Table(table) = &back.nodes[1] else {
        panic!()
    };
    assert_eq!(table.rows, doc_table(&doc).rows);
    let structure = table.structure.as_ref().expect("spans give a structure");
    assert_eq!(structure.col_continuation[1], vec![false, false, true]);
    assert_eq!(structure.row_continuation[3], vec![true, false, false]);
    assert_eq!(
        back.nodes[4],
        Node::Code {
            language: None,
            text: "fn main() {\n    println!(\"hi\");\n}".into(),
            orig: None,
            pretty: None
        }
    );
    assert_eq!(
        back.nodes[5],
        Node::CheckboxItem {
            checked: true,
            text: "done".into()
        }
    );
    assert_fixed_point_for(Target::Docx, &back);
}
