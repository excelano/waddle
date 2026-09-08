//! The package formats this crate writes.
//!
//! Author: David M. Anderson
//! Built with AI assistance (Claude, Anthropic)

use std::fmt;
use std::str::FromStr;

/// An output package format, named the way `--to` names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Target {
    /// OpenDocument Text.
    Odt,
    /// Office Open XML WordprocessingML.
    Docx,
}

impl Target {
    /// The file extension, without the dot.
    pub fn extension(self) -> &'static str {
        match self {
            Target::Odt => "odt",
            Target::Docx => "docx",
        }
    }

    /// The package's media type, which ODF also stores as its first entry.
    pub fn media_type(self) -> &'static str {
        match self {
            Target::Odt => "application/vnd.oasis.opendocument.text",
            Target::Docx => {
                "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
            }
        }
    }
}

impl fmt::Display for Target {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.extension())
    }
}

/// The error for a format name this crate does not write.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownTarget(pub String);

impl fmt::Display for UnknownTarget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "unknown output format `{}`; expected odt or docx",
            self.0
        )
    }
}

impl std::error::Error for UnknownTarget {}

impl FromStr for Target {
    type Err = UnknownTarget;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "odt" => Ok(Target::Odt),
            "docx" => Ok(Target::Docx),
            _ => Err(UnknownTarget(s.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_case_insensitively() {
        assert_eq!("odt".parse::<Target>(), Ok(Target::Odt));
        assert_eq!("DOCX".parse::<Target>(), Ok(Target::Docx));
        assert_eq!(
            "pdf".parse::<Target>().unwrap_err().to_string(),
            "unknown output format `pdf`; expected odt or docx"
        );
    }

    #[test]
    fn extension_is_the_display_form() {
        assert_eq!(Target::Odt.to_string(), Target::Odt.extension());
    }
}
