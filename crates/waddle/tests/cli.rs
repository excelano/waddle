//! The binary driven as an agent would drive it: exit codes, the three
//! input forms with their pictures found, the destination rules, and the
//! flags. Fixtures are built here from a document, so nothing outside the
//! repository is needed.
//!
//! Author: David M. Anderson
//! Built with AI assistance (Claude, Anthropic)

use std::io::{Cursor, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use docling_core::{CaptionParent, DoclingDocument, Node, PictureImage};

/// A 16 by 12 grey PNG.
const PNG: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x10, 0x00, 0x00, 0x00, 0x0c, 0x08, 0x02, 0x00, 0x00, 0x00, 0xe4, 0x85, 0xaa,
    0xd6, 0x00, 0x00, 0x00, 0x13, 0x49, 0x44, 0x41, 0x54, 0x78, 0xda, 0x63, 0xb8, 0x40, 0x22, 0x60,
    0x18, 0xd5, 0x30, 0xaa, 0x01, 0x3b, 0x00, 0x00, 0x99, 0xc3, 0xd4, 0x10, 0x65, 0x6a, 0xad, 0xca,
    0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
];

fn waddle(args: &[&str], stdin: Option<&[u8]>) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_waddle"))
        .args(args)
        .env("NO_COLOR", "1")
        .stdin(if stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("waddle runs");
    if let Some(bytes) = stdin {
        child.stdin.take().unwrap().write_all(bytes).unwrap();
    }
    child.wait_with_output().unwrap()
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("waddle-cli-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A heading, a paragraph, and a picture with bytes.
fn document() -> DoclingDocument {
    let mut doc = DoclingDocument::new("t");
    doc.add_heading(1, "Ducks");
    doc.add_paragraph("A **duck** on a pond.");
    doc.push(Node::Picture {
        caption: Some("Figure 1".into()),
        caption_href: None,
        image: Some(PictureImage {
            mimetype: "image/png".into(),
            width: 16,
            height: 12,
            data: PNG.to_vec(),
        }),
        classification: None,
        caption_parent: CaptionParent::Body,
    });
    doc
}

/// The first picture part of a written package.
fn picture_in(odt: &Path) -> Vec<u8> {
    let mut archive = zip::ZipArchive::new(Cursor::new(std::fs::read(odt).unwrap())).unwrap();
    let mut file = archive.by_name("Pictures/image1.png").unwrap();
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).unwrap();
    bytes
}

/// Bare DocLang beside its `assets/`, as docling's export names them.
fn write_dclg(dir: &Path) -> PathBuf {
    let xml = document().export_to_doclang();
    let uri = xml
        .split("<src uri=\"")
        .nth(1)
        .and_then(|rest| rest.split('"').next())
        .expect("the export names an asset");
    let asset = dir.join(uri);
    std::fs::create_dir_all(asset.parent().unwrap()).unwrap();
    std::fs::write(&asset, PNG).unwrap();
    let path = dir.join("ducks.dclg");
    std::fs::write(&path, xml).unwrap();
    path
}

#[test]
fn a_missing_file_is_exit_1_and_a_bad_format_exit_2() {
    let out = waddle(&["nowhere.dclg", "--to", "odt"], None);
    assert_eq!(out.status.code(), Some(1));
    assert!(text(&out.stderr).starts_with("waddle: cannot read nowhere.dclg"));
    let out = waddle(&["nowhere.dclg", "--to", "pdf"], None);
    assert_eq!(out.status.code(), Some(2));
    let out = waddle(&["-", "--to", "odt"], Some(b"<doclang/>"));
    assert_eq!(out.status.code(), Some(2), "{}", text(&out.stderr));
    assert!(text(&out.stderr).contains("-o is required"));
}

#[test]
fn a_file_that_is_none_of_the_formats_is_exit_1() {
    let dir = scratch("format");
    let path = dir.join("notes.txt");
    std::fs::write(&path, "just prose").unwrap();
    let out = waddle(&[path.to_str().unwrap(), "--to", "odt"], None);
    assert_eq!(out.status.code(), Some(1));
    assert!(
        text(&out.stderr).contains("is not DocLang"),
        "{}",
        text(&out.stderr)
    );
}

#[test]
fn bare_doclang_finds_its_assets_and_writes_beside_itself() {
    let dir = scratch("dclg");
    let path = write_dclg(&dir);
    let out = waddle(&[path.to_str().unwrap(), "--to", "odt"], None);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    let written = dir.join("ducks.odt");
    assert_eq!(text(&out.stdout).trim(), written.to_str().unwrap());
    assert_eq!(
        text(&out.stderr),
        "",
        "no warnings when the picture is found"
    );
    assert_eq!(picture_in(&written), PNG);
}

#[test]
fn the_archive_carries_its_assets_and_standard_input_works() {
    let dir = scratch("dclx");
    let dclg = write_dclg(&dir);
    let xml = std::fs::read(&dclg).unwrap();
    let uri = text(&xml)
        .split("<src uri=\"")
        .nth(1)
        .and_then(|rest| rest.split('"').next().map(str::to_string))
        .unwrap();
    let mut archive = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default();
    archive.start_file("document.xml", options).unwrap();
    archive.write_all(&xml).unwrap();
    archive.start_file(&uri, options).unwrap();
    archive.write_all(PNG).unwrap();
    let dclx = archive.finish().unwrap().into_inner();
    let target = dir.join("from-stdin.odt");
    let out = waddle(
        &["-", "--to", "odt", "-o", target.to_str().unwrap()],
        Some(&dclx),
    );
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    assert_eq!(picture_in(&target), PNG);
}

#[test]
fn json_carries_its_pictures_as_data_uris() {
    let dir = scratch("json");
    let path = dir.join("ducks.json");
    std::fs::write(&path, document().export_to_json()).unwrap();
    let out = waddle(
        &[
            path.to_str().unwrap(),
            "--to",
            "odt",
            "-o",
            dir.to_str().unwrap(),
        ],
        None,
    );
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    assert_eq!(picture_in(&dir.join("ducks.odt")), PNG);
}

#[test]
fn docx_is_the_other_target() {
    let dir = scratch("docx");
    let path = write_dclg(&dir);
    let out = waddle(&[path.to_str().unwrap(), "--to", "docx"], None);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    let written = dir.join("ducks.docx");
    let mut archive = zip::ZipArchive::new(Cursor::new(std::fs::read(&written).unwrap())).unwrap();
    let mut file = archive.by_name("word/media/image1.png").unwrap();
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).unwrap();
    assert_eq!(bytes, PNG);
}

#[test]
fn the_skill_installs_into_the_home_directory() {
    let home = scratch("skill");
    let out = Command::new(env!("CARGO_BIN_EXE_waddle"))
        .arg("--install-skill")
        .env("HOME", &home)
        .env("USERPROFILE", &home)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    let skill = home.join(".claude/skills/waddle/SKILL.md");
    let body = std::fs::read_to_string(&skill).unwrap();
    assert!(body.starts_with("---\nname: waddle\n"));
    assert!(body.contains(&format!(
        "This skill documents waddle {}",
        env!("CARGO_PKG_VERSION")
    )));
    assert!(home.join(".claude/skills/waddle/reference.md").is_file());
    let again = Command::new(env!("CARGO_BIN_EXE_waddle"))
        .arg("--install-skill")
        .env("HOME", &home)
        .env("USERPROFILE", &home)
        .output()
        .unwrap();
    assert!(text(&again.stdout).contains("already current"));
    let gone = Command::new(env!("CARGO_BIN_EXE_waddle"))
        .arg("--uninstall-skill")
        .env("HOME", &home)
        .env("USERPROFILE", &home)
        .output()
        .unwrap();
    assert_eq!(gone.status.code(), Some(0));
    assert!(!skill.exists());
}

#[test]
fn nothing_is_overwritten() {
    let dir = scratch("overwrite");
    let path = write_dclg(&dir);
    let first = waddle(&[path.to_str().unwrap(), "--to", "odt"], None);
    let second = waddle(&[path.to_str().unwrap(), "--to", "odt"], None);
    assert_eq!(
        text(&first.stdout).trim(),
        dir.join("ducks.odt").to_str().unwrap()
    );
    assert_eq!(
        text(&second.stdout).trim(),
        dir.join("ducks (1).odt").to_str().unwrap()
    );
    assert!(dir.join("ducks (1).odt").is_file());
}

#[test]
fn dry_run_writes_nothing_and_strict_refuses_a_warning() {
    let dir = scratch("flags");
    let mut doc = document();
    doc.push(Node::Furniture {
        layer: docling_core::ContentLayer::Furniture,
        inner: Box::new(Node::Paragraph {
            text: "running head".into(),
        }),
    });
    let path = dir.join("head.dclg");
    std::fs::write(&path, doc.export_to_doclang()).unwrap();

    let out = waddle(&[path.to_str().unwrap(), "--to", "odt", "--dry-run"], None);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    assert!(text(&out.stdout).starts_with("would write "));
    assert!(!dir.join("head.odt").exists());
    let stderr = text(&out.stderr);
    assert!(stderr.contains("would be dropped or degraded"), "{stderr}");
    assert!(
        stderr.contains("furniture: dropped, on the furniture layer"),
        "{stderr}"
    );
    assert!(
        stderr.contains("picture: written as a placeholder"),
        "the export names an asset that is not beside this file: {stderr}"
    );

    let out = waddle(&[path.to_str().unwrap(), "--to", "odt", "--strict"], None);
    assert_eq!(out.status.code(), Some(1));
    assert!(
        text(&out.stderr).contains("--strict"),
        "{}",
        text(&out.stderr)
    );
    assert!(!dir.join("head.odt").exists());

    let out = waddle(&[path.to_str().unwrap(), "--to", "odt"], None);
    assert_eq!(out.status.code(), Some(0), "warnings are not failures");
    assert!(dir.join("head.odt").is_file());
}
