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
use waddle_core::odt;

const FORMATS: &[&str] = &["md", "docx", "odf", "html", "pptx", "xlsx"];

fn corpus() -> Option<PathBuf> {
    let dir = match std::env::var_os("WADDLE_CORPUS") {
        Some(dir) => PathBuf::from(dir),
        None => PathBuf::from(std::env::var_os("HOME")?)
            .join("clones/docling.rs/crates/docling/tests/data"),
    };
    dir.is_dir().then_some(dir)
}

fn read_back(name: &str, bytes: Vec<u8>) -> Result<DoclingDocument, String> {
    let source = SourceDocument::from_bytes(name, InputFormat::Odt, bytes);
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
            Node::InlineGroup { runs, .. } => out.extend(runs.iter().map(|r| r.text.clone())),
            // The DocLang-only form is what the writer takes when present.
            Node::ListItem {
                text,
                dclx,
                layer: None,
                ..
            } => out.push(dclx.as_ref().map_or(text.clone(), |d| d.text.clone())),
            Node::Code { text, .. } => out.push(text.clone()),
            Node::Table(table) | Node::Chart { table, .. } => {
                out.extend(table.rows.iter().flatten().cloned());
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

fn count(nodes: &[Node], what: fn(&Node) -> bool) -> usize {
    nodes
        .iter()
        .map(|n| match n {
            Node::Group {
                layer: None,
                children,
                ..
            } => count(children, what),
            Node::Located { inner, .. }
            | Node::Commented { inner, .. }
            | Node::DoclangOnly(inner) => count(std::slice::from_ref(inner), what),
            n => usize::from(what(n)),
        })
        .sum()
}

/// A chart with data is written as a table, one without as a picture.
fn is_table(n: &Node) -> bool {
    match n {
        Node::Table(_) | Node::FieldRegion { .. } => true,
        Node::Chart { table, .. } => !table.rows.is_empty(),
        _ => false,
    }
}

fn is_picture(n: &Node) -> bool {
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

fn opens_in_writer(name: &str, bytes: &[u8]) -> Result<(), String> {
    let dir = std::env::temp_dir().join(format!("waddle-corpus-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let odt = dir.join(format!("{name}.odt"));
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

fn check(path: &std::path::Path) -> Result<(), String> {
    let name = path.file_name().unwrap().to_string_lossy().to_string();
    let source = SourceDocument::from_file(path).map_err(|e| format!("docling: {e}"))?;
    let doc = match DocumentConverter::new().convert(source) {
        Ok(r) => r.document,
        Err(e) => {
            eprintln!("skipping {name}: docling cannot read it: {e}");
            return Ok(());
        }
    };
    let out = odt::write(&doc).map_err(|e| e.to_string())?;
    if let Some(dir) = std::env::var_os("WADDLE_DUMP") {
        std::fs::write(PathBuf::from(dir).join(format!("{name}.odt")), &out.bytes).unwrap();
    }
    if std::env::var_os("WADDLE_CORPUS_WRITER").is_some() {
        opens_in_writer(&name, &out.bytes)?;
    }
    let back = read_back(&name, out.bytes)?;

    let mut pieces = Vec::new();
    body_text(&doc.nodes, &mut pieces);
    let haystack = words(&back.export_to_markdown());
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
    let (tables, tables_back) = (count(&doc.nodes, is_table), count(&back.nodes, is_table));
    if tables != tables_back {
        problems.push(format!("{tables} tables in, {tables_back} out"));
    }
    let (pictures, pictures_back) = (
        count(&doc.nodes, is_picture),
        count(&back.nodes, is_picture),
    );
    if pictures != pictures_back {
        problems.push(format!("{pictures} pictures in, {pictures_back} out"));
    }
    let again = read_back(&name, odt::write(&back).map_err(|e| e.to_string())?.bytes)?;
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
            checked += 1;
            if let Err(problem) = check(&path) {
                failures.push(format!(
                    "{format}/{}: {problem}",
                    path.file_name().unwrap().to_string_lossy()
                ));
            }
        }
    }
    eprintln!(
        "{checked} corpus documents round-tripped, {} with problems",
        failures.len()
    );
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}
