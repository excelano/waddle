//! The corpus round trip: every document docling.rs's own test corpus holds
//! in the formats waddle cares about, read by docling, written as ODT, read
//! back, and checked for three properties. `DESIGN.md` §6.
//!
//! The corpus is the clone at `~/clones/docling.rs`, or `WADDLE_CORPUS`
//! pointing at its `crates/docling/tests/data`. Without it the test skips
//! and says so; `cargo test` does not imply a checkout.
//!
//! The properties are chosen to survive the diffs the reader cannot close,
//! so they hold across the corpus rather than on a curated subset: every
//! piece of body text in the source is somewhere in the result, tables and
//! pictures are as many as they were, and a second trip is a fixed point.
//! `WADDLE_CORPUS_WRITER=1` also opens every package in headless Writer,
//! which takes a second per document.
//!
//! Author: David M. Anderson
//! Built with AI assistance (Claude, Anthropic)

use std::path::PathBuf;
use std::process::Command;

use docling::{DocumentConverter, InputFormat, SourceDocument};
use docling_core::{DoclingDocument, Node};
use waddle_core::{Target, docx, odt};

const FORMATS: &[&str] = &["md", "docx", "odf", "html", "pptx", "xlsx"];

fn corpus() -> Option<PathBuf> {
    let dir = match std::env::var_os("WADDLE_CORPUS") {
        Some(dir) => PathBuf::from(dir),
        None => PathBuf::from(std::env::var_os("HOME")?)
            .join("clones/docling.rs/crates/docling/tests/data"),
    };
    dir.is_dir().then_some(dir)
}

fn write(target: Target, doc: &DoclingDocument) -> Result<Vec<u8>, String> {
    match target {
        Target::Odt => odt::write(doc),
        Target::Docx => docx::write(doc),
    }
    .map(|out| out.bytes)
    .map_err(|e| e.to_string())
}

fn read_back(target: Target, name: &str, bytes: Vec<u8>) -> Result<DoclingDocument, String> {
    let format = match target {
        Target::Odt => InputFormat::Odt,
        Target::Docx => InputFormat::Docx,
    };
    let source = SourceDocument::from_bytes(name, format, bytes);
    DocumentConverter::new()
        .convert(source)
        .map(|r| r.document)
        .map_err(|e| format!("reader refused the package: {e}"))
}

/// The letters and digits of a text, with docling's HTML entities decoded
/// first: what survives a trip is the words, not the markers, spacing or
/// escaping around them.
fn words(s: &str) -> String {
    s.replace("[x]", "")
        .replace("[ ]", "")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .chars()
        .filter(|c| c.is_alphanumeric())
        .collect()
}

/// The body text pieces a reader must give back: everything a person reads.
fn body_text(nodes: &[Node], out: &mut Vec<String>) {
    for node in nodes {
        match node {
            Node::Heading { text, .. }
            | Node::Paragraph { text }
            | Node::CheckboxItem { text, .. } => out.push(text.clone()),
            // The Markdown has the runs' text and the links the runs lack.
            Node::InlineGroup { md_text, .. } => out.push(md_text.clone()),
            // The DocLang-only form is what the writer takes when present.
            Node::ListItem {
                text,
                dclx,
                layer: None,
                ..
            } => out.push(dclx.as_ref().map_or(text.clone(), |d| d.text.clone())),
            Node::Code { text, .. } => out.push(text.clone()),
            // A cell's blocks in place of its flat text, in the order a
            // reader that nests rather than flattens would give them.
            Node::Table(table) | Node::Chart { table, .. } => {
                for (r, row) in table.rows.iter().enumerate() {
                    for (c, text) in row.iter().enumerate() {
                        match table
                            .cell_blocks
                            .as_ref()
                            .and_then(|b| b.get(r))
                            .and_then(|row| row.get(c))
                            .filter(|blocks| !blocks.is_empty())
                        {
                            Some(blocks) => body_text(blocks, out),
                            None => out.push(text.clone()),
                        }
                    }
                }
                if let Some(caption) = &table.caption {
                    out.push(caption.clone());
                }
            }
            Node::Picture {
                caption: Some(c), ..
            } => out.push(c.clone()),
            Node::Group {
                layer: None,
                children,
                ..
            } => body_text(children, out),
            Node::Located { inner, .. }
            | Node::Commented { inner, .. }
            | Node::DoclangOnly(inner) => body_text(std::slice::from_ref(inner), out),
            _ => {}
        }
    }
}

fn count(nodes: &[Node], what: fn(Target, &Node) -> bool, target: Target) -> usize {
    nodes
        .iter()
        .map(|n| match n {
            Node::Group {
                layer: None,
                children,
                ..
            } => count(children, what, target),
            Node::Located { inner, .. }
            | Node::Commented { inner, .. }
            | Node::DoclangOnly(inner) => count(std::slice::from_ref(inner), what, target),
            n => usize::from(what(target, n)),
        })
        .sum()
}

/// A chart with data is written as a table, one without as a picture.
/// The DOCX reader unwraps a one-cell table into its content by design
/// (DESIGN.md §6), so one is not counted for that target.
fn is_table(target: Target, n: &Node) -> bool {
    let one_cell = |t: &docling_core::Table| {
        target == Target::Docx && t.rows.len() == 1 && t.rows[0].len() == 1
    };
    match n {
        Node::Table(t) => !one_cell(t),
        Node::FieldRegion { .. } => true,
        Node::Chart { table, .. } => !table.rows.is_empty() && !one_cell(table),
        _ => false,
    }
}

fn is_picture(_: Target, n: &Node) -> bool {
    match n {
        Node::Picture { .. } => true,
        Node::Chart { table, .. } => table.rows.is_empty(),
        _ => false,
    }
}

/// A node with every run of whitespace in it made one space. The reader
/// returns headings, list items and rich cells as flat Markdown, in which
/// underline, sub and superscript have no marker; on the second trip such a
/// run merges into its neighbour, and the reader's spacing around a run
/// depends on whether it was one. Everything but that count is compared.
fn shape(node: &Node) -> String {
    let mut out = String::new();
    let mut in_space = false;
    for c in format!("{node:?}").chars() {
        if c.is_whitespace() {
            if !in_space {
                out.push(' ');
            }
            in_space = true;
        } else {
            out.push(c);
            in_space = false;
        }
    }
    out
}

fn opens_in_writer(target: Target, name: &str, bytes: &[u8]) -> Result<(), String> {
    let dir = std::env::temp_dir().join(format!("waddle-corpus-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let odt = dir.join(format!("{name}.{target}"));
    std::fs::write(&odt, bytes).map_err(|e| e.to_string())?;
    let result = Command::new("soffice")
        .arg("--headless")
        .arg(format!(
            "-env:UserInstallation=file://{}",
            dir.join("profile").display()
        ))
        .args(["--convert-to", "pdf", "--outdir"])
        .arg(&dir)
        .arg(&odt)
        .output()
        .map_err(|e| e.to_string())?;
    let ok = result.status.success() && dir.join(format!("{name}.pdf")).is_file();
    let _ = std::fs::remove_dir_all(&dir);
    if ok {
        Ok(())
    } else {
        Err(format!(
            "Writer did not convert it: {}",
            String::from_utf8_lossy(&result.stderr)
        ))
    }
}

fn check(target: Target, path: &std::path::Path) -> Result<(), String> {
    let name = path.file_name().unwrap().to_string_lossy().to_string();
    let source = SourceDocument::from_file(path).map_err(|e| format!("docling: {e}"))?;
    let doc = match DocumentConverter::new().convert(source) {
        Ok(r) => r.document,
        Err(e) => {
            eprintln!("skipping {name}: docling cannot read it: {e}");
            return Ok(());
        }
    };
    let bytes = write(target, &doc)?;
    if let Some(dir) = std::env::var_os("WADDLE_DUMP") {
        std::fs::write(PathBuf::from(dir).join(format!("{name}.{target}")), &bytes).unwrap();
    }
    if std::env::var_os("WADDLE_CORPUS_WRITER").is_some() {
        opens_in_writer(target, &name, &bytes)?;
    }
    let back = read_back(target, &name, bytes)?;

    let mut pieces = Vec::new();
    body_text(&doc.nodes, &mut pieces);
    let mut back_pieces = Vec::new();
    body_text(&back.nodes, &mut back_pieces);
    let haystack = words(&back_pieces.join(" "));
    let missing: Vec<&String> = pieces
        .iter()
        .filter(|p| {
            let w = words(p);
            !w.is_empty() && !haystack.contains(&w)
        })
        .collect();
    let mut problems = Vec::new();
    if !missing.is_empty() {
        problems.push(format!(
            "{} of {} text pieces missing after the trip, first: {:?}",
            missing.len(),
            pieces.len(),
            missing
                .iter()
                .take(3)
                .map(|s| s.chars().take(60).collect::<String>())
                .collect::<Vec<_>>()
        ));
    }
    let (tables, tables_back) = (
        count(&doc.nodes, is_table, target),
        count(&back.nodes, is_table, target),
    );
    if tables != tables_back {
        problems.push(format!("{tables} tables in, {tables_back} out"));
    }
    let (pictures, pictures_back) = (
        count(&doc.nodes, is_picture, target),
        count(&back.nodes, is_picture, target),
    );
    if pictures != pictures_back {
        problems.push(format!("{pictures} pictures in, {pictures_back} out"));
    }
    let again = read_back(target, &name, write(target, &back)?)?;
    let same = again.nodes.len() == back.nodes.len()
        && again
            .nodes
            .iter()
            .zip(&back.nodes)
            .all(|(a, b)| shape(a) == shape(b));
    if !same {
        let first = again
            .nodes
            .iter()
            .zip(&back.nodes)
            .position(|(a, b)| shape(a) != shape(b))
            .map_or(String::from("node count differs"), |i| {
                format!(
                    "first at node {i}: {:?} became {:?}",
                    back.nodes[i], again.nodes[i]
                )
                .chars()
                .take(300)
                .collect()
            });
        problems.push(format!("second trip is not a fixed point, {first}"));
    }
    if problems.is_empty() {
        Ok(())
    } else {
        Err(problems.join("; "))
    }
}

#[test]
fn corpus_round_trips() {
    let Some(dir) = corpus() else {
        eprintln!("no docling.rs corpus; skipping (WADDLE_CORPUS or ~/clones/docling.rs)");
        return;
    };
    let mut failures = Vec::new();
    let mut checked = 0;
    for format in FORMATS {
        let sources = dir.join(format).join("sources");
        let Ok(entries) = std::fs::read_dir(&sources) else {
            continue;
        };
        let mut paths: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_file())
            .collect();
        paths.sort();
        for path in paths {
            for target in [Target::Odt, Target::Docx] {
                checked += 1;
                if let Err(problem) = check(target, &path) {
                    failures.push(format!(
                        "{format}/{} to {target}: {problem}",
                        path.file_name().unwrap().to_string_lossy()
                    ));
                }
            }
        }
    }
    eprintln!("{checked} corpus trips, {} with problems", failures.len());
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}
