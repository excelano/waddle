//! Zip assembly for the office packages.
//!
//! Both targets are zip files with rules about entries. ODF wants `mimetype`
//! first and stored uncompressed, so a `file` command can read the type from
//! the first bytes; OPC has no such rule. The caller states the order and
//! which entries are stored; this module writes them.
//!
//! Author: David M. Anderson
//! Built with AI assistance (Claude, Anthropic)

use std::io::{Cursor, Write};

use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

use crate::Error;

/// One entry of a package.
pub struct Part<'a> {
    pub name: &'a str,
    pub bytes: &'a [u8],
    /// Stored as is rather than deflated.
    pub stored: bool,
}

/// Write the parts, in the order given, and return the package bytes.
pub fn zip(parts: &[Part<'_>]) -> Result<Vec<u8>, Error> {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    for part in parts {
        let method = if part.stored {
            CompressionMethod::Stored
        } else {
            CompressionMethod::Deflated
        };
        let options = SimpleFileOptions::default().compression_method(method);
        writer.start_file(part.name, options)?;
        writer
            .write_all(part.bytes)
            .map_err(zip::result::ZipError::Io)?;
    }
    Ok(writer.finish()?.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;
    use zip::ZipArchive;

    #[test]
    fn keeps_order_and_storage() {
        let bytes = zip(&[
            Part {
                name: "mimetype",
                bytes: b"application/x-test",
                stored: true,
            },
            Part {
                name: "content.xml",
                bytes: b"<x/>",
                stored: false,
            },
        ])
        .unwrap();
        let mut archive = ZipArchive::new(Cursor::new(bytes)).unwrap();
        let first = archive.by_index(0).unwrap();
        assert_eq!(first.name(), "mimetype");
        assert_eq!(first.compression(), CompressionMethod::Stored);
        drop(first);
        let second = archive.by_index(1).unwrap();
        assert_eq!(second.name(), "content.xml");
        assert_eq!(second.compression(), CompressionMethod::Deflated);
    }
}
