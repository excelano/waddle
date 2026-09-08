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
    Ok(Input {
        name,
        bytes,
        format,
        base_dir,
    })
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
