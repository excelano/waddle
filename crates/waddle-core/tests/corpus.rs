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

use std::collections::HashSet;
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

/// A piece of body text, and whether a heading holds it: the DOCX reader
/// returns a heading as plain text.
struct Piece {
    text: String,
    heading: bool,
}

impl From<String> for Piece {
    fn from(text: String) -> Self {
        Piece {
            text,
            heading: false,
        }
    }
}

/// The body text pieces a reader must give back: everything a person reads.
/// The body's text pieces into `out`, and the ids of its anchors into
/// `anchors`.
fn body_text(nodes: &[Node], out: &mut Vec<Piece>, anchors: &mut HashSet<String>) {
    for node in nodes {
        match node {
            Node::Heading { text, .. } => out.push(Piece {
                text: text.clone(),
                heading: true,
            }),
            Node::Paragraph { text }
            | Node::ListText { text, .. }
            | Node::CheckboxItem { text, .. } => out.push(text.clone().into()),
            // The Markdown has the runs' text and the links the runs lack.
            Node::InlineGroup { md_text, .. } => out.push(md_text.clone().into()),
            // The DocLang-only form is what the writer takes when present.
            Node::ListItem {
                text,
                dclx,
                layer: None,
                ..
            } => out.push(
                dclx.as_ref()
                    .map_or(text.clone(), |d| d.text.clone())
                    .into(),
            ),
            Node::Code { text, .. } => out.push(text.clone().into()),
            // A cell's blocks in place of its flat text, in the order a
            // reader that nests rather than flattens would give them. A
            // merged cell's blocks live on its anchor, the position it starts
            // at, and every position it covers reads them there.
            Node::Table(table) | Node::Chart { table, .. } => {
                let span_anchors = table.span_anchors();
                for (r, row) in table.rows.iter().enumerate() {
                    for (c, text) in row.iter().enumerate() {
                        let (ar, ac) = span_anchors[r][c];
                        match table
                            .cell_blocks
                            .as_ref()
                            .and_then(|b| b.get(ar))
                            .and_then(|row| row.get(ac))
                            .filter(|blocks| !blocks.is_empty())
                        {
                            Some(blocks) => body_text(blocks, out, anchors),
                            None => out.push(text.clone().into()),
                        }
                    }
                }
                if let Some(caption) = &table.caption {
                    out.push(caption.clone().into());
                }
            }
            Node::Picture { caption, .. } => out.extend(caption.clone().map(Piece::from)),
            // A standalone caption is a paragraph of its own, so its text
            // comes back like any other paragraph's.
            Node::Caption { text, .. } | Node::LabeledText { text, .. } => {
                out.push(text.clone().into())
            }
            Node::Group {
                layer: None,
                children,
                ..
            } => body_text(children, out, anchors),
            Node::Located { inner, .. }
            | Node::Prov { inner, .. }
            | Node::Commented { inner, .. }
            | Node::Track { inner, .. }
            | Node::DoclangOnly(inner) => body_text(std::slice::from_ref(inner), out, anchors),
            // Written, but not as text this comparison can follow: a
            // formula becomes its source in a code paragraph, a field
            // region a table of its items, a text dump a run of paragraphs.
            Node::Formula { .. } | Node::FieldRegion { .. } | Node::TextDump(_) => {}
            // Dropped by the writer, by layer or by kind, so none of their
            // text is expected on the way back. DESIGN.md §5.
            Node::ListItem { .. }
            | Node::Group { .. }
            | Node::Furniture { .. }
            | Node::FurnitureText { .. }
            | Node::PageFurniture { .. }
            | Node::CommentSection { .. }
            | Node::KeyValueGraph { .. }
            | Node::PictureChildren(_)
            | Node::PageInfo { .. } => {}
            // No text of its own.
            Node::Anchor { id } => {
                anchors.insert(id.clone());
            }
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

/// The corpus documents whose DOCX second trip loses the empty DocLang-only
/// texts the reader leaves after a table, one for each multi-run paragraph in
/// its cells. The first trip's reader splits those paragraphs into one cell
/// block per run, the writer writes the blocks as they came, and the second
/// trip's reader finds no multi-run paragraph to leave a text for. Every cell
/// is the same on both trips. DESIGN.md §6.
const ORPHAN_TEXTS_IN_DOCX: &[&str] = &["html_inline_group_in_table_cell.html"];

fn orphan_texts_in_docx(target: Target, name: &str) -> bool {
    target == Target::Docx && ORPHAN_TEXTS_IN_DOCX.contains(&name)
}

/// A source piece as the target's reader can return it. The DOCX reader
/// returns a heading as plain text, so every link in one comes back as its
/// text alone, and it resolves a link to a bare fragment only through a
/// bookmark, which the writer writes for each of the document's anchors, so
/// an internal link to anything else comes back as its text. It writes
/// each `&` of a link destination as `&amp;amp;`, escaping it twice where the
/// ODF reader writes it as it is. DESIGN.md §6.
fn as_the_reader_returns(target: Target, piece: &Piece, anchors: &HashSet<String>) -> String {
    if target != Target::Docx {
        return piece.text.clone();
    }
    let opener = if piece.heading { "](" } else { "](#" };
    let mut out = String::new();
    let mut rest = piece.text.as_str();
    while let Some(at) = rest.find(opener) {
        let Some(end) = destination_end(&rest[at + 2..]) else {
            out.push_str(&rest[..=at]);
            return out;
        };
        let destination = &rest[at + 2..at + 2 + end];
        let bookmarked = !piece.heading
            && destination
                .strip_prefix('#')
                .is_some_and(|id| anchors.contains(id));
        if bookmarked {
            out.push_str(&rest[..at + 2 + end + 1]);
        } else {
            out.push_str(&rest[..=at]);
        }
        rest = &rest[at + 2 + end + 1..];
    }
    out.push_str(rest);
    escaped_twice_in_destinations(&out)
}

/// `text` with every `&` inside a link destination written `&amp;amp;`.
fn escaped_twice_in_destinations(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(at) = rest.find("](") {
        out.push_str(&rest[..at + 2]);
        rest = &rest[at + 2..];
        let Some(end) = destination_end(rest) else {
            break;
        };
        out.push_str(&rest[..end].replace('&', "&amp;amp;"));
        rest = &rest[end..];
    }
    out.push_str(rest);
    out
}

/// The corpus documents whose DOCX second trip is not a fixed point because
/// a link destination has an `&` in it: the reader escapes it twice, so each
/// trip adds an `amp;`. DESIGN.md §6.
const AMPERSAND_LINKS_IN_DOCX: &[&str] = &["wiki_duck.html"];

fn ampersand_links_in_docx(target: Target, name: &str) -> bool {
    target == Target::Docx && AMPERSAND_LINKS_IN_DOCX.contains(&name)
}

/// Whether every picture with data returns with data: in DOCX, only when the
/// writer draws a PNG for each SVG.
fn svg_comes_back(target: Target) -> bool {
    target != Target::Docx || cfg!(feature = "svg-fallback")
}

/// Whether the target's reader can return this picture as a picture at all.
/// The ODF reader, following docling's `_odf_image_can_be_bitmap`, takes no
/// `image/svg+xml` frame as a picture, and docling 2.53 and later render a
/// DOCX's DrawingML shapes and text boxes as SVG. DESIGN.md §6.
fn read_as_a_picture(target: Target, image: Option<&docling_core::PictureImage>) -> bool {
    let svg = image.is_some_and(|i| i.mimetype == "image/svg+xml");
    !(svg && matches!(target, Target::Odt | Target::Odp))
}

/// The placeholder as the DOCX reader returns it. That reader cannot measure
/// an SVG, so built without `svg-fallback`, which puts a PNG in front of one,
/// it returns an SVG picture as a picture without data; the writer writes a
/// picture without data as the placeholder, so the second trip returns the
/// placeholder where the first returned nothing. The comparison reads the
/// placeholder as no data, on both trips. DESIGN.md §6.
fn docx_placeholder() -> &'static str {
    static PLACEHOLDER: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    PLACEHOLDER.get_or_init(|| {
        let mut doc = DoclingDocument::new("placeholder");
        doc.push(Node::Picture {
            caption: None,
            caption_href: None,
            image: None,
            classification: None,
            description: None,
            caption_parent: Default::default(),
            caption_location: None,
        });
        let back = read_back(
            Target::Docx,
            "placeholder.docx",
            write(Target::Docx, &doc).unwrap(),
        )
        .unwrap();
        let image = back.nodes.iter().find_map(|n| {
            if let Node::Picture { image, .. } = n {
                image.clone()
            } else {
                None
            }
        });
        format!(
            "image: Some({:?})",
            image.expect("the placeholder reads back as a picture")
        )
    })
}

/// The index of the `)` that closes a link destination, parentheses inside it
/// balanced: `Taxonomy_(biology))` closes at the second.
fn destination_end(dest: &str) -> Option<usize> {
    let mut depth = 0usize;
    for (i, c) in dest.char_indices() {
        match c {
            '(' => depth += 1,
            ')' if depth == 0 => return Some(i),
            ')' => depth -= 1,
            _ => {}
        }
    }
    None
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

/// What a trip counts: the nodes the writer turns into a table, the nodes
/// it turns into a picture, and the pictures that carry data.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Want {
    Tables,
    Pictures,
    PicturesWithData,
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
            | Node::Track { inner, .. }
            | Node::DoclangOnly(inner) => count(std::slice::from_ref(inner), want, target),
            Node::Table(table) => usize::from(want == Want::Tables && !one_cell(target, table)),
            // A field region is written as a table of its keys and values.
            Node::FieldRegion { .. } => usize::from(want == Want::Tables),
            // A chart with data is written as a table, one without as a
            // picture.
            Node::Chart { table, .. } => usize::from(match want {
                Want::Tables => !table.rows.is_empty() && !one_cell(target, table),
                Want::Pictures => table.rows.is_empty(),
                Want::PicturesWithData => false,
            }),
            Node::Picture { image, .. } => usize::from(match want {
                Want::Tables => false,
                Want::Pictures => read_as_a_picture(target, image.as_ref()),
                Want::PicturesWithData => {
                    image.is_some() && read_as_a_picture(target, image.as_ref())
                }
            }),
            // Neither a table nor a picture, in either target: written as
            // something else, or dropped by layer or by kind.
            Node::Heading { .. }
            | Node::Paragraph { .. }
            | Node::ListText { .. }
            | Node::CheckboxItem { .. }
            | Node::ListItem { .. }
            | Node::Code { .. }
            | Node::Formula { .. }
            | Node::Caption { .. }
            | Node::LabeledText { .. }
            | Node::Group { .. }
            | Node::InlineGroup { .. }
            | Node::Furniture { .. }
            | Node::FurnitureText { .. }
            | Node::CommentSection { .. }
            | Node::PageFurniture { .. }
            | Node::PageBreak
            | Node::PageInfo { .. }
            | Node::Anchor { .. }
            | Node::KeyValueGraph { .. }
            | Node::PictureChildren(_)
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
    shape_of(&format!("{node:?}"))
}

fn shape_of(debug: &str) -> String {
    let mut out = String::new();
    let mut in_space = false;
    for c in debug.chars() {
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
    let mut anchors = HashSet::new();
    body_text(&doc.nodes, &mut pieces, &mut anchors);
    let mut back_pieces = Vec::new();
    body_text(&back.nodes, &mut back_pieces, &mut HashSet::new());
    let haystack = words(
        &back_pieces
            .iter()
            .map(|p| p.text.as_str())
            .collect::<Vec<_>>()
            .join(" "),
    );
    let missing: Vec<&Piece> = pieces
        .iter()
        .filter(|p| {
            let w = words(&as_the_reader_returns(target, p, &anchors));
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
                .map(|p| p.text.chars().take(60).collect::<String>())
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
    // Every picture is written with bytes, its own or the placeholder's, so
    // every picture comes back with data; an SVG does from DOCX only through
    // the PNG the writer puts in front of it.
    let with_data_back = count(&back.nodes, Want::PicturesWithData, target);
    if with_data_back != pictures_back && keeps_prose(target) && svg_comes_back(target) {
        problems.push(format!(
            "{} of {pictures_back} pictures came back without data",
            pictures_back - with_data_back
        ));
    }
    let again = read_back(target, &name, write(target, &back)?)?;
    let second = |node: &Node| {
        let seen = shape(node);
        if target == Target::Docx {
            seen.replace(&shape_of(docx_placeholder()), "image: None")
        } else {
            seen
        }
    };
    let same = again.nodes.len() == back.nodes.len()
        && again
            .nodes
            .iter()
            .zip(&back.nodes)
            .all(|(a, b)| second(a) == second(b));
    if !same
        && !sparse_in_a_sheet(target, &name)
        && !orphan_texts_in_docx(target, &name)
        && !ampersand_links_in_docx(target, &name)
    {
        let first = again
            .nodes
            .iter()
            .zip(&back.nodes)
            .position(|(a, b)| second(a) != second(b))
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
