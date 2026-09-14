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
use waddle_core::{Target, docx, odp, ods, odt, xlsx};

const FORMATS: &[&str] = &["md", "docx", "odf", "html", "pptx", "xlsx"];

/// The crate's own data directory, and the repository-root mirror beside it.
/// docling.rs 1.50 moved most sources out of the crate into a mirror at the
/// repository root, leaving a `mirror.txt` manifest in their place, so a
/// format's sources are now in either directory or both.
fn corpus() -> Option<(PathBuf, PathBuf)> {
    let dir = match std::env::var_os("WADDLE_CORPUS") {
        Some(dir) => PathBuf::from(dir),
        None => PathBuf::from(std::env::var_os("HOME")?)
            .join("clones/docling.rs/crates/docling/tests/data"),
    };
    if !dir.is_dir() {
        return None;
    }
    // <root>/crates/docling/tests/data upwards four is <root>.
    let mirror = dir
        .ancestors()
        .nth(4)
        .map_or_else(PathBuf::new, |root| root.join("tests").join("data"));
    Some((dir, mirror))
}

/// Every source a format has: the files the crate still carries itself, plus
/// the ones its `mirror.txt` names in the repository-root mirror. The
/// manifest is the authority rather than the mirror's directory listing,
/// because the mirror keeps a rendered `.pdf` beside each source and those
/// are not this suite's to read.
fn sources((dir, mirror): &(PathBuf, PathBuf), format: &str) -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = std::fs::read_dir(dir.join(format).join("sources"))
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_file())
        .collect();
    if let Ok(manifest) = std::fs::read_to_string(dir.join(format).join("mirror.txt")) {
        for name in manifest.lines().map(str::trim).filter(|l| !l.is_empty()) {
            let path = mirror.join(format).join("sources").join(name);
            if path.is_file() {
                paths.push(path);
            }
        }
    }
    paths.sort();
    paths
}

fn write(target: Target, doc: &DoclingDocument) -> Result<Vec<u8>, String> {
    match target {
        Target::Odt => odt::write(doc),
        Target::Docx => docx::write(doc),
        Target::Ods => ods::write(doc),
        Target::Odp => odp::write(doc),
        Target::Xlsx => xlsx::write(doc),
    }
    .map(|out| out.bytes)
    .map_err(|e| e.to_string())
}

fn read_back(target: Target, name: &str, bytes: Vec<u8>) -> Result<DoclingDocument, String> {
    let format = match target {
        Target::Odt => InputFormat::Odt,
        Target::Docx => InputFormat::Docx,
        Target::Ods => InputFormat::Ods,
        Target::Odp => InputFormat::Odp,
        Target::Xlsx => InputFormat::Xlsx,
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
            Node::Picture { caption, .. } => out.extend(caption.clone()),
            // A standalone caption is a paragraph of its own, so its text
            // comes back like any other paragraph's.
            Node::Caption { text, .. } => out.push(text.clone()),
            Node::Group {
                layer: None,
                children,
                ..
            } => body_text(children, out),
            Node::Located { inner, .. }
            | Node::Prov { inner, .. }
            | Node::Commented { inner, .. }
            | Node::DoclangOnly(inner) => body_text(std::slice::from_ref(inner), out),
            // Written, but not as text this comparison can follow: a
            // formula becomes its source in a code paragraph, a field
            // region a table of its items, a text dump a run of paragraphs.
            Node::Formula { .. } | Node::FieldRegion { .. } | Node::TextDump(_) => {}
            // Dropped by the writer, by layer or by kind, so none of their
            // text is expected on the way back. DESIGN.md §5.
            Node::ListItem { .. }
            | Node::Group { .. }
            | Node::Furniture { .. }
            | Node::PageFurniture { .. }
            | Node::CommentSection { .. }
            | Node::PageInfo { .. } => {}
            // No text of its own.
            Node::PageBreak => {}
        }
    }
}

/// The corpus documents whose tables cannot survive a sheet, listed by name
/// rather than tolerated in general. The ODS reader rebuilds a table by
/// flood-filling the non-empty cells of a sheet, so a table whose filled cells
/// are not 4-connected returns as several tables, and one with no filled cells
/// at all does not return: `odf_presentation_02.odp` carries a 1x1 table
/// holding an empty string, and a spreadsheet has no way to say that a table
/// was there. The rest hold merged or sparse tables whose blank interiors are
/// the gaps the flood fill parts on. DESIGN.md §6.
const SPARSE_IN_A_SHEET: &[&str] = &[
    "odf_presentation_01.odp",
    "odf_presentation_02.odp",
    "powerpoint_sample.pptx",
    "word_tables.docx",
];

/// Whether this document's tables are among the ones a sheet cannot hold.
fn sparse_in_a_sheet(target: Target, name: &str) -> bool {
    target == Target::Ods && SPARSE_IN_A_SHEET.contains(&name)
}

/// ODS is a spreadsheet: it writes the tabular nodes and reports every other
/// one as dropped (DESIGN.md §7), so the properties about body text and about
/// pictures are not its to satisfy. The table count and the fixed point are.
fn keeps_prose(target: Target) -> bool {
    match target {
        Target::Odt | Target::Docx | Target::Odp => true,
        Target::Ods | Target::Xlsx => false,
    }
}

/// What a trip counts: the nodes the writer turns into a table, and the
/// nodes it turns into a picture.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Want {
    Tables,
    Pictures,
}

/// The DOCX reader unwraps a one-cell table into its content by design
/// (DESIGN.md §6), so one is not counted for that target.
fn one_cell(target: Target, table: &docling_core::Table) -> bool {
    target == Target::Docx && table.rows.len() == 1 && table.rows[0].len() == 1
}

/// Every arm is spelled out: a variant docling.rs adds must fail the build
/// here and be decided, as it does in the writers. A wildcard once counted a
/// provenance-wrapped table as no table at all, and the corpus reported it as
/// a round-trip diff rather than a compile error.
fn count(nodes: &[Node], want: Want, target: Target) -> usize {
    nodes
        .iter()
        .map(|node| match node {
            Node::Group {
                layer: None,
                children,
                ..
            } => count(children, want, target),
            Node::Located { inner, .. }
            | Node::Prov { inner, .. }
            | Node::Commented { inner, .. }
            | Node::DoclangOnly(inner) => count(std::slice::from_ref(inner), want, target),
            Node::Table(table) => usize::from(want == Want::Tables && !one_cell(target, table)),
            // A field region is written as a table of its keys and values.
            Node::FieldRegion { .. } => usize::from(want == Want::Tables),
            // A chart with data is written as a table, one without as a
            // picture.
            Node::Chart { table, .. } => usize::from(match want {
                Want::Tables => !table.rows.is_empty() && !one_cell(target, table),
                Want::Pictures => table.rows.is_empty(),
            }),
            Node::Picture { .. } => usize::from(want == Want::Pictures),
            // Neither a table nor a picture, in either target: written as
            // something else, or dropped by layer or by kind.
            Node::Heading { .. }
            | Node::Paragraph { .. }
            | Node::CheckboxItem { .. }
            | Node::ListItem { .. }
            | Node::Code { .. }
            | Node::Formula { .. }
            | Node::Caption { .. }
            | Node::Group { .. }
            | Node::InlineGroup { .. }
            | Node::Furniture { .. }
            | Node::CommentSection { .. }
            | Node::PageFurniture { .. }
            | Node::PageBreak
            | Node::PageInfo { .. }
            | Node::TextDump(_) => 0,
        })
        .sum()
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
    if !missing.is_empty() && keeps_prose(target) {
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
        count(&doc.nodes, Want::Tables, target),
        count(&back.nodes, Want::Tables, target),
    );
    if tables != tables_back && !sparse_in_a_sheet(target, &name) {
        problems.push(format!("{tables} tables in, {tables_back} out"));
    }
    let (pictures, pictures_back) = (
        count(&doc.nodes, Want::Pictures, target),
        count(&back.nodes, Want::Pictures, target),
    );
    if pictures != pictures_back && keeps_prose(target) {
        problems.push(format!("{pictures} pictures in, {pictures_back} out"));
    }
    let again = read_back(target, &name, write(target, &back)?)?;
    let same = again.nodes.len() == back.nodes.len()
        && again
            .nodes
            .iter()
            .zip(&back.nodes)
            .all(|(a, b)| shape(a) == shape(b));
    if !same && !sparse_in_a_sheet(target, &name) {
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
    let Some(dirs) = corpus() else {
        eprintln!("no docling.rs corpus; skipping (WADDLE_CORPUS or ~/clones/docling.rs)");
        return;
    };
    let mut failures = Vec::new();
    let mut checked = 0;
    for format in FORMATS {
        let paths = sources(&dirs, format);
        // A format that contributes nothing is a failure, not a quiet skip.
        // Upstream moved its sources once already and the suite went from 228
        // trips to 22 without saying so.
        if paths.is_empty() {
            failures.push(format!(
                "{format}: no sources, in the crate directory or the mirror"
            ));
            continue;
        }
        for path in paths {
            for target in [
                Target::Odt,
                Target::Docx,
                Target::Ods,
                Target::Odp,
                Target::Xlsx,
            ] {
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
