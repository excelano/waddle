//! `--install-skill` / `--uninstall-skill`: write the repository's Claude Code
//! skill into the user's personal skills directory.
//!
//! The skill is compiled into the binary rather than shipped as a package data
//! file, because the binary is the only artifact present on every install
//! path: apt, Homebrew, `cargo install`, the curl one-liner, a prebuilt GitHub
//! binary and a source build all produce it, and `cargo install` ships no data
//! files at all. Embedding is what makes one instruction, `waddle
//! --install-skill`, true everywhere. The contract is fleet-wide.
//!
//! Author: David M. Anderson
//! Built with AI assistance (Claude, Anthropic)

use sha2::{Digest, Sha256};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

/// The command named in the revision stamp.
const TOOL: &str = "waddle";

/// The directory under `~/.claude/skills/`, matching `skills/<name>/` in the
/// repository so a manual copy needs no rename.
const SKILL: &str = "waddle";

/// The skill's files, embedded at compile time. `include_str!` has no
/// directory form, so the list is written by hand, and a test fails if it
/// falls behind what the repository ships.
const FILES: &[(&str, &str)] = &[
    ("SKILL.md", include_str!("../skills/waddle/SKILL.md")),
    (
        "reference.md",
        include_str!("../skills/waddle/reference.md"),
    ),
];

/// The embedded skill's revision, which the stamp and `-V` both carry.
static REVISION: LazyLock<String> = LazyLock::new(|| revision_of(FILES));

static VERSION: LazyLock<String> =
    LazyLock::new(|| format!("{} (skill {})", env!("CARGO_PKG_VERSION"), *REVISION));

/// The version `-V` prints after the binary's name.
pub fn version() -> &'static str {
    &VERSION
}

/// The first eight hex digits of SHA-256 over the files sorted by path, each
/// fed as its path, a NUL, its byte length in decimal, a NUL, and its bytes.
/// The Go tools hash the same form, so equal skill text gives an equal
/// revision across the fleet.
fn revision_of(files: &[(&str, &str)]) -> String {
    let mut sorted: Vec<_> = files.iter().collect();
    sorted.sort_by_key(|(name, _)| *name);
    let mut hash = Sha256::new();
    for (name, body) in sorted {
        hash.update(name.as_bytes());
        hash.update(b"\0");
        hash.update(body.len().to_string().as_bytes());
        hash.update(b"\0");
        hash.update(body.as_bytes());
    }
    hash.finalize()[..4]
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// The command the user typed, for message prefixes.
fn invoked() -> String {
    std::env::args()
        .next()
        .as_deref()
        .map(Path::new)
        .and_then(Path::file_stem)
        .map(|s| s.to_string_lossy().into_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| TOOL.to_string())
}

enum Outcome {
    Installed,
    AlreadyCurrent,
    /// What the previous stamp named, when it could be read back.
    Updated(Option<String>),
    /// The destination is a symlink; its target.
    Linked(PathBuf),
}

/// `~/.claude/skills/<name>`, from the environment.
fn destination() -> io::Result<PathBuf> {
    let home = if cfg!(windows) {
        std::env::var_os("USERPROFILE")
    } else {
        std::env::var_os("HOME")
    };
    let home = home.filter(|h| !h.is_empty()).ok_or_else(|| {
        io::Error::other(if cfg!(windows) {
            "cannot find your home directory: USERPROFILE is not set"
        } else {
            "cannot find your home directory: HOME is not set"
        })
    })?;
    Ok(Path::new(&home).join(".claude").join("skills").join(SKILL))
}

/// The revision stamp directly after the YAML frontmatter, never inside it:
/// the description in that block decides whether the skill fires at all.
fn stamp(body: &str, revision: &str) -> String {
    let note = format!(
        "> This skill is revision {revision}. If `{TOOL} -V` reports a different skill\n\
         > revision, the skill is stale; run `{TOOL} --install-skill` to refresh it.\n"
    );
    const FENCE: &str = "---\n";
    if let Some(rest) = body.strip_prefix(FENCE)
        && let Some(i) = rest.find("\n---\n")
    {
        let split = FENCE.len() + i + FENCE.len() + 1;
        let (front, tail) = body.split_at(split);
        return format!("{front}\n{note}{tail}");
    }
    format!("{note}\n{body}")
}

/// The revision an earlier install stamped, or for a stamp from before
/// revisions, the tool and version it named.
fn stamped_label(body: &str) -> Option<String> {
    let old = format!("> This skill documents {TOOL} ");
    let first_word = |rest: &str| {
        rest.split_whitespace()
            .next()
            .map(|w| w.trim_end_matches('.').to_string())
    };
    body.lines()
        .find_map(|l| match l.strip_prefix("> This skill is revision ") {
            Some(rest) => first_word(rest),
            None => l
                .strip_prefix(&old)
                .and_then(first_word)
                .map(|v| format!("{TOOL} {v}")),
        })
}

fn payload(revision: &str) -> Vec<(&'static str, String)> {
    FILES
        .iter()
        .map(|(name, body)| {
            let text = if *name == "SKILL.md" {
                stamp(body, revision)
            } else {
                (*body).to_string()
            };
            (*name, text)
        })
        .collect()
}

fn write_skill(dest: &Path, revision: &str) -> io::Result<Outcome> {
    // A symlink here points an installed skill at a working tree, which
    // tracks edits a copy cannot; overwriting it would disconnect the two.
    if let Ok(target) = fs::read_link(dest) {
        return Ok(Outcome::Linked(target));
    }
    let files = payload(revision);
    let existing_stamp = fs::read_to_string(dest.join("SKILL.md")).ok();
    let unchanged = files.iter().all(|(name, text)| {
        fs::read_to_string(dest.join(name)).is_ok_and(|on_disk| &on_disk == text)
    });
    if unchanged {
        return Ok(Outcome::AlreadyCurrent);
    }
    let fresh = !dest.exists();
    fs::create_dir_all(dest)?;
    for (name, text) in &files {
        fs::write(dest.join(name), text)?;
    }
    Ok(if fresh {
        Outcome::Installed
    } else {
        Outcome::Updated(existing_stamp.as_deref().and_then(stamped_label))
    })
}

/// Handle `--install-skill`. Returns the process exit code.
pub fn install() -> u8 {
    let revision = REVISION.as_str();
    let me = invoked();
    let dest = match destination() {
        Ok(d) => d,
        Err(e) => return fail(&e.to_string()),
    };
    match write_skill(&dest, revision) {
        Err(e) => fail(&format!("cannot write {}: {e}", dest.display())),
        Ok(outcome) => {
            let path = dest.display();
            match outcome {
                Outcome::Linked(target) => {
                    println!("{me}: {path} is a symlink to {}", target.display());
                    println!(
                        "{me}: leaving it alone; a link tracks its source directly, which is what you want on a machine that edits the skill."
                    );
                }
                Outcome::AlreadyCurrent => {
                    println!("{me}: skill already current at {path} (skill {revision})");
                }
                Outcome::Installed => {
                    println!("{me}: installed skill to {path} (skill {revision})");
                    println!("{me}: restart Claude Code to pick it up.");
                }
                Outcome::Updated(from) => {
                    match from {
                        Some(old) if old != revision => {
                            println!("{me}: updated skill at {path} ({old} to {revision})");
                        }
                        _ => println!("{me}: updated skill at {path} (skill {revision})"),
                    }
                    println!("{me}: restart Claude Code to pick up the change.");
                }
            }
            0
        }
    }
}

/// Handle `--uninstall-skill`. Returns the process exit code. A symlink is
/// removed here, unlike on install: a dangling link is worth nothing and a
/// link is recreated in a second.
pub fn uninstall() -> u8 {
    let me = invoked();
    let dest = match destination() {
        Ok(d) => d,
        Err(e) => return fail(&e.to_string()),
    };
    let path = dest.display();
    let link = fs::read_link(&dest).ok();
    let result = match link {
        Some(_) => fs::remove_file(&dest),
        None if dest.exists() => fs::remove_dir_all(&dest),
        None => {
            println!("{me}: no skill installed at {path}");
            return 0;
        }
    };
    match result {
        Err(e) => fail(&format!("cannot remove {path}: {e}")),
        Ok(()) => {
            match link {
                Some(target) => println!(
                    "{me}: removed the symlink at {path} (it pointed at {})",
                    target.display()
                ),
                None => println!("{me}: removed the skill at {path}"),
            }
            0
        }
    }
}

fn fail(message: &str) -> u8 {
    eprintln!("{}: {message}", invoked());
    1
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The guard that keeps `FILES` in step with what the repository ships.
    #[test]
    fn file_list_matches_the_directory() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("skills")
            .join(SKILL);
        let mut on_disk: Vec<String> = fs::read_dir(&dir)
            .expect("the skill directory exists")
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        let mut embedded: Vec<String> = FILES.iter().map(|(n, _)| n.to_string()).collect();
        on_disk.sort();
        embedded.sort();
        assert_eq!(
            embedded, on_disk,
            "FILES in src/skill.rs is out of step with skills/{SKILL}/"
        );
    }

    #[test]
    fn the_stamp_sits_under_the_frontmatter_not_inside_it() {
        let body = "---\nname: waddle\ndescription: x\n---\n\n# waddle\n";
        let out = stamp(body, "0123abcd");
        let fm_end = out.find("\n---\n").map(|i| i + 5).unwrap();
        assert!(!out[..fm_end].contains("This skill is revision"));
        assert!(out[fm_end..].contains("This skill is revision 0123abcd."));
        assert!(out.ends_with("# waddle\n"));
    }

    #[test]
    fn a_stamp_round_trips_through_the_reader() {
        let stamped = stamp("---\nname: waddle\n---\n\nbody\n", "1a2b3c4d");
        assert_eq!(stamped_label(&stamped).as_deref(), Some("1a2b3c4d"));
        assert_eq!(stamped_label("---\nname: waddle\n---\n\nbody\n"), None);
    }

    /// A skill installed before revisions carries a version stamp, and the
    /// first update has to name it.
    #[test]
    fn a_version_stamp_reads_back_as_tool_and_version() {
        let old = "---\nname: waddle\n---\n\n> This skill documents waddle 0.3.0. If `waddle -V` \
                   reports a different\n> version, the skill is stale.\n";
        assert_eq!(stamped_label(old).as_deref(), Some("waddle 0.3.0"));
    }

    /// The same fixture and answer are in the Go tools' tests, which is what
    /// holds the two languages to one canonical form.
    #[test]
    fn the_revision_matches_the_go_tools() {
        assert_eq!(
            revision_of(&[("reference.md", "b\n"), ("SKILL.md", "a\n")]),
            "0cce3525"
        );
    }

    #[test]
    fn the_stamp_carries_the_revision_version_prints() {
        let files = payload(&REVISION);
        let (_, skill) = files.iter().find(|(n, _)| *n == "SKILL.md").unwrap();
        assert_eq!(stamped_label(skill).as_deref(), Some(REVISION.as_str()));
        assert_eq!(
            version(),
            format!("{} (skill {})", env!("CARGO_PKG_VERSION"), *REVISION)
        );
    }

    #[test]
    fn the_shipped_skill_keeps_its_frontmatter() {
        let (_, body) = FILES.iter().find(|(n, _)| *n == "SKILL.md").unwrap();
        let out = stamp(body, "0123abcd");
        assert!(out.starts_with("---\nname: waddle\n"));
        assert_eq!(
            out.matches("\n---\n").count(),
            body.matches("\n---\n").count()
        );
    }

    #[test]
    fn install_update_and_uninstall_in_a_scratch_home() {
        let home = std::env::temp_dir().join(format!("waddle-skill-home-{}", std::process::id()));
        let _ = fs::remove_dir_all(&home);
        fs::create_dir_all(&home).unwrap();
        let dest = home.join(".claude").join("skills").join(SKILL);
        assert!(matches!(
            write_skill(&dest, "0000aaaa").unwrap(),
            Outcome::Installed
        ));
        assert!(matches!(
            write_skill(&dest, "0000aaaa").unwrap(),
            Outcome::AlreadyCurrent
        ));
        match write_skill(&dest, "0000bbbb").unwrap() {
            Outcome::Updated(Some(old)) => assert_eq!(old, "0000aaaa"),
            other => panic!("{}", matches!(other, Outcome::Installed)),
        }
        assert!(dest.join("reference.md").is_file());
        fs::remove_dir_all(&home).unwrap();
    }
}
