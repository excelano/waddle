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

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// The command named in the version stamp.
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
    /// The previous stamp's version, when it could be read back.
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

/// The version stamp directly after the YAML frontmatter, never inside it:
/// the description in that block decides whether the skill fires at all.
fn stamp(body: &str, version: &str) -> String {
    let note = format!(
        "> This skill documents {TOOL} {version}. If `{TOOL} -V` reports a different\n\
         > version, the skill is stale; run `{TOOL} --install-skill` to refresh it.\n"
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

fn stamped_version(body: &str) -> Option<String> {
    let prefix = format!("> This skill documents {TOOL} ");
    body.lines()
        .find_map(|l| l.strip_prefix(&prefix))
        .and_then(|rest| rest.split_whitespace().next())
        .map(|v| v.trim_end_matches('.').to_string())
}

fn payload(version: &str) -> Vec<(&'static str, String)> {
    FILES
        .iter()
        .map(|(name, body)| {
            let text = if *name == "SKILL.md" {
                stamp(body, version)
            } else {
                (*body).to_string()
            };
            (*name, text)
        })
        .collect()
}

fn write_skill(dest: &Path, version: &str) -> io::Result<Outcome> {
    // A symlink here points an installed skill at a working tree, which
    // tracks edits a copy cannot; overwriting it would disconnect the two.
    if let Ok(target) = fs::read_link(dest) {
        return Ok(Outcome::Linked(target));
    }
    let files = payload(version);
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
        Outcome::Updated(existing_stamp.as_deref().and_then(stamped_version))
    })
}

/// Handle `--install-skill`. Returns the process exit code.
pub fn install() -> u8 {
    let version = env!("CARGO_PKG_VERSION");
    let me = invoked();
    let dest = match destination() {
        Ok(d) => d,
        Err(e) => return fail(&e.to_string()),
    };
    match write_skill(&dest, version) {
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
                    println!("{me}: skill already current at {path} ({TOOL} {version})");
                }
                Outcome::Installed => {
                    println!("{me}: installed skill to {path} ({TOOL} {version})");
                    println!("{me}: restart Claude Code to pick it up.");
                }
                Outcome::Updated(from) => {
                    match from {
                        Some(old) if old != version => {
                            println!("{me}: updated skill at {path} ({old} to {version})");
                        }
                        _ => println!("{me}: updated skill at {path} ({TOOL} {version})"),
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
        let out = stamp(body, "9.9.9");
        let fm_end = out.find("\n---\n").map(|i| i + 5).unwrap();
        assert!(!out[..fm_end].contains("This skill documents"));
        assert!(out[fm_end..].contains("This skill documents waddle 9.9.9"));
        assert!(out.ends_with("# waddle\n"));
    }

    #[test]
    fn a_stamp_round_trips_through_the_reader() {
        let stamped = stamp("---\nname: waddle\n---\n\nbody\n", "0.5.1");
        assert_eq!(stamped_version(&stamped).as_deref(), Some("0.5.1"));
        assert_eq!(stamped_version("---\nname: waddle\n---\n\nbody\n"), None);
    }

    #[test]
    fn the_shipped_skill_keeps_its_frontmatter() {
        let (_, body) = FILES.iter().find(|(n, _)| *n == "SKILL.md").unwrap();
        let out = stamp(body, "1.2.3");
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
            write_skill(&dest, "0.0.1").unwrap(),
            Outcome::Installed
        ));
        assert!(matches!(
            write_skill(&dest, "0.0.1").unwrap(),
            Outcome::AlreadyCurrent
        ));
        match write_skill(&dest, "0.0.2").unwrap() {
            Outcome::Updated(Some(old)) => assert_eq!(old, "0.0.1"),
            other => panic!("{}", matches!(other, Outcome::Installed)),
        }
        assert!(dest.join("reference.md").is_file());
        fs::remove_dir_all(&home).unwrap();
    }
}
