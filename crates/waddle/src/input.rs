//! Reading the input: a file or standard input, its format told from its
//! bytes, and docling's own reader turning it into a document.
//!
//! Author: David M. Anderson
//! Built with AI assistance (Claude, Anthropic)

use std::io::Read;
use std::path::{Path, PathBuf};

use docling::{DocumentConverter, InputFormat, SourceDocument};
use waddle_core::DoclingDocument;

/// The three serialisations docling.rs reads back losslessly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// Bare DocLang XML, `.dclg`.
    Doclang,
    /// The DocLang archive, `.dclx`: a zip with `document.xml` and assets.
    Dclx,
    /// docling's JSON export.
    Json,
}

impl Format {
    fn docling(self) -> InputFormat {
        match self {
            Format::Doclang => InputFormat::XmlDoclang,
            Format::Dclx => InputFormat::Dclx,
            Format::Json => InputFormat::JsonDocling,
        }
    }
}

/// What was read, kept whole because the asset resolver reads it again.
pub struct Input {
    /// The document's name: the file stem, or `waddle` for standard input.
    pub name: String,
    pub bytes: Vec<u8>,
    pub format: Format,
    /// The directory relative picture paths resolve against: the file's,
    /// or none for standard input.
    pub base_dir: Option<PathBuf>,
}

/// The format a document's first bytes say it is.
pub fn sniff(bytes: &[u8]) -> Option<Format> {
    if bytes.starts_with(b"PK") {
        return Some(Format::Dclx);
    }
    let body = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    match body.iter().find(|b| !b.is_ascii_whitespace())? {
        b'{' => Some(Format::Json),
        b'<' => Some(Format::Doclang),
        _ => None,
    }
}

/// Read a file, or standard input for `-`.
pub fn read(path: &Path) -> Result<Input, String> {
    let from_stdin = path.as_os_str() == "-";
    let (name, bytes, base_dir) = if from_stdin {
        let mut bytes = Vec::new();
        std::io::stdin()
            .read_to_end(&mut bytes)
            .map_err(|e| format!("cannot read standard input: {e}"))?;
        ("waddle".to_string(), bytes, None)
    } else {
        let bytes = std::fs::read(path)
            .map_err(|e| format!("cannot read {}: {}", path.display(), reason(&e)))?;
        let name = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "waddle".to_string());
        let base_dir = path.parent().map(Path::to_path_buf);
        (name, bytes, base_dir)
    };
    let what = if from_stdin {
        "standard input".to_string()
    } else {
        path.display().to_string()
    };
    let format = sniff(&bytes)
        .ok_or_else(|| format!("{what} is not DocLang, a DocLang archive or docling JSON"))?;
    if format == Format::Dclx
        && let Some(other) = not_an_archive(&bytes)
    {
        return Err(format!(
            "{what} is {other}, not DocLang; waddle reads DocLang, a DocLang archive or docling JSON"
        ));
    }
    Ok(Input {
        name,
        bytes,
        format,
        base_dir,
    })
}

/// What a zip is when it is not a DocLang archive, which is one without a
/// `document.xml`. docling would read an office package given to it as an
/// archive under its own format, and waddle does not claim that conversion.
fn not_an_archive(bytes: &[u8]) -> Option<String> {
    let Ok(mut zip) = zip::ZipArchive::new(std::io::Cursor::new(bytes)) else {
        return Some("a zip archive that cannot be opened".to_string());
    };
    let has = |zip: &zip::ZipArchive<_>, name: &str| zip.index_for_name(name).is_some();
    if has(&zip, "document.xml") {
        return None;
    }
    let what = if has(&zip, "word/document.xml") {
        "a Word document (DOCX)".to_string()
    } else if has(&zip, "xl/workbook.xml") {
        "an Excel workbook (XLSX)".to_string()
    } else if has(&zip, "ppt/presentation.xml") {
        "a PowerPoint presentation (PPTX)".to_string()
    } else {
        let mut mimetype = String::new();
        let odf = zip
            .by_name("mimetype")
            .ok()
            .and_then(|mut f| f.read_to_string(&mut mimetype).ok())
            .is_some_and(|_| mimetype.starts_with("application/vnd.oasis.opendocument"));
        if odf {
            format!("an OpenDocument file ({})", mimetype.trim())
        } else {
            "a zip archive without a document.xml".to_string()
        }
    };
    Some(what)
}

/// docling's reading of the input.
pub fn convert(input: &Input) -> Result<DoclingDocument, String> {
    let source = SourceDocument::from_bytes(
        input.name.clone(),
        input.format.docling(),
        input.bytes.clone(),
    );
    DocumentConverter::new()
        .convert(source)
        .map(|r| r.document)
        .map_err(|e| format!("docling cannot read the input: {e}"))
}

/// An I/O error's message without the OS's parenthetical code.
pub fn reason(e: &std::io::Error) -> String {
    let text = e.to_string();
    match text.find(" (os error") {
        Some(at) => text[..at].to_string(),
        None => text,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_are_told_from_the_first_bytes() {
        assert_eq!(sniff(b"PK\x03\x04"), Some(Format::Dclx));
        assert_eq!(sniff(b"  \n{\"name\": 1}"), Some(Format::Json));
        assert_eq!(sniff(b"\xEF\xBB\xBF<doclang/>"), Some(Format::Doclang));
        assert_eq!(sniff(b"# not any of them"), None);
        assert_eq!(sniff(b""), None);
    }
}
