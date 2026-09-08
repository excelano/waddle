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
use waddle_core::{Output, odt};

fn read_back(bytes: Vec<u8>) -> DoclingDocument {
    let source = SourceDocument::from_bytes("t.odt", InputFormat::Odt, bytes);
    DocumentConverter::new()
        .convert(source)
        .expect("docling reads the package back")
        .document
}

/// With `WADDLE_DUMP=/some/dir`, every package written here is also saved
/// there, named by the test, so it can be opened in Writer by hand.
fn odt_trip(doc: &DoclingDocument) -> (DoclingDocument, Output) {
    let out = odt::write(doc).expect("odt writes");
    let name = std::thread::current()
        .name()
        .unwrap_or("test")
        .replace("::", "_");
    if let Some(dir) = std::env::var_os("WADDLE_DUMP") {
        std::fs::write(Path::new(&dir).join(format!("{name}.odt")), &out.bytes)
            .expect("dump directory is writable");
    }
    assert_opens_in_writer(&name, &out.bytes);
    (read_back(out.bytes.clone()), out)
}

/// Convert the package to PDF with headless LibreOffice, each call in its
/// own profile directory so parallel tests do not share one instance.
fn assert_opens_in_writer(name: &str, bytes: &[u8]) {
    if Command::new("soffice").arg("--version").output().is_err() {
        eprintln!("soffice not installed; skipping the Writer check for {name}");
        return;
    }
    let dir = std::env::temp_dir().join(format!("waddle-writer-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    let odt = dir.join(format!("{name}.odt"));
    std::fs::write(&odt, bytes).expect("temp odt");
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
    let pdf = dir.join(format!("{name}.pdf"));
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
    let (again, _) = odt_trip(back);
    assert_eq!(
        again.nodes, back.nodes,
        "a second trip changes the document"
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
