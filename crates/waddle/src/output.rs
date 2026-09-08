//! Where the package goes, and the rule that nothing is overwritten.
//!
//! Author: David M. Anderson
//! Built with AI assistance (Claude, Anthropic)

use std::path::{Path, PathBuf};

/// The path to write, before the numbering rule: `-o` as a file, `-o` as
/// an existing directory to write into, or beside the input.
pub fn destination(
    input: Option<&Path>,
    name: &str,
    output: Option<&Path>,
    extension: &str,
) -> Result<PathBuf, String> {
    let file_name = format!("{name}.{extension}");
    match (output, input) {
        (Some(out), _) if out.is_dir() => Ok(out.join(file_name)),
        (Some(out), _) => Ok(out.to_path_buf()),
        (None, Some(input)) => Ok(input.with_file_name(file_name)),
        (None, None) => Err("-o is required when reading standard input".to_string()),
    }
}

/// The path itself when nothing is there, else the first of `name (1).ext`,
/// `name (2).ext`, … that is free. A converter that writes beside its
/// source is one wrong extension away from replacing somebody's file.
pub fn available(path: &Path) -> PathBuf {
    if !path.exists() {
        return path.to_path_buf();
    }
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let extension = path
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();
    (1..)
        .map(|n| path.with_file_name(format!("{stem} ({n}){extension}")))
        .find(|candidate| !candidate.exists())
        .expect("some number is free")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn beside_the_input_by_default() {
        let dest = destination(Some(Path::new("/x/report.dclg")), "report", None, "odt").unwrap();
        assert_eq!(dest, PathBuf::from("/x/report.odt"));
    }

    #[test]
    fn a_named_file_is_taken_as_given() {
        let dest = destination(
            Some(Path::new("/x/a.dclg")),
            "a",
            Some(Path::new("/y/b.odt")),
            "odt",
        )
        .unwrap();
        assert_eq!(dest, PathBuf::from("/y/b.odt"));
    }

    #[test]
    fn standard_input_needs_a_destination() {
        assert!(destination(None, "waddle", None, "odt").is_err());
    }

    #[test]
    fn a_taken_name_is_numbered() {
        let dir = std::env::temp_dir().join(format!("waddle-available-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("report.odt");
        assert_eq!(available(&path), path);
        std::fs::write(&path, b"x").unwrap();
        assert_eq!(available(&path), dir.join("report (1).odt"));
        std::fs::write(dir.join("report (1).odt"), b"x").unwrap();
        assert_eq!(available(&path), dir.join("report (2).odt"));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
