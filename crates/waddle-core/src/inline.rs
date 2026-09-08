//! Inline runs: the formatted pieces of one paragraph, from wherever the
//! model kept them.
//!
//! A node that came through a reader with run support carries
//! [`InlineRun`]s. A node that came through the DocLang, JSON or Markdown
//! reader carries its formatting as Markdown markers inside the text, in
//! docling's dialect: `***`, `**`, `*`, `~~`, backticks, `[text](url)`,
//! with `_` escaped as `\_` and `&`, `<`, `>` as HTML entities. This module
//! turns either into one list of [`Run`]s, which is what a writer emits.
//!
//! docling-core's own scanner of that dialect keeps no link target and trims
//! every run, and a general Markdown parser would read block structure,
//! HTML and entities into text that is inline by definition. So this is a
//! scanner of the dialect and nothing more. An unmatched marker stays
//! literal, as it does in docling's.
//!
//! Author: David M. Anderson
//! Built with AI assistance (Claude, Anthropic)

use docling_core::{InlineRun, Script};

/// One run of text with one formatting.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Run {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    pub script: Position,
    pub code: bool,
    pub href: Option<String>,
}

/// Where a run sits relative to the baseline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Position {
    #[default]
    Baseline,
    Sub,
    Super,
}

impl Run {
    /// The same formatting with different text.
    fn with_text(&self, text: String) -> Run {
        Run {
            text,
            href: self.href.clone(),
            ..*self
        }
    }
}

/// The runs of a node that carries them.
pub fn from_inline_runs(runs: &[InlineRun]) -> Vec<Run> {
    runs.iter()
        .map(|r| Run {
            text: r.text.clone(),
            bold: r.bold,
            italic: r.italic,
            underline: r.underline,
            strike: r.strike,
            script: match r.script {
                Script::Baseline => Position::Baseline,
                Script::Sub => Position::Sub,
                Script::Super => Position::Super,
            },
            code: r.code,
            href: None,
        })
        .collect()
}

/// The runs of a text in docling's Markdown dialect.
pub fn from_markdown(text: &str) -> Vec<Run> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    scan(&chars, &Run::default(), &mut out);
    out
}

/// Scan `chars` under the formatting `style`, appending runs to `out`.
/// Longest markers first, so `***` is not read as `**` and `*`. A marker
/// opens only against non-space and closes only after non-space, as in
/// CommonMark, so `2 * 3 ** 4` stays arithmetic.
fn scan(chars: &[char], style: &Run, out: &mut Vec<Run>) {
    let mut plain = String::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '\\' && i + 1 < chars.len() && chars[i + 1].is_ascii_punctuation() {
            plain.push(chars[i + 1]);
            i += 2;
            continue;
        }
        if c == '&'
            && let Some((decoded, len)) = entity(&chars[i..])
        {
            plain.push(decoded);
            i += len;
            continue;
        }
        if let Some((marker, apply)) = marker_at(chars, i)
            && let Some(end) = closing(chars, i + marker.len(), marker)
        {
            flush(&mut plain, style, out);
            let inner = apply(style);
            scan(&chars[i + marker.len()..end], &inner, out);
            i = end + marker.len();
            continue;
        }
        if c == '`'
            && let Some(end) = find(chars, i + 1, &['`'])
        {
            flush(&mut plain, style, out);
            let code: String = chars[i + 1..end].iter().collect();
            out.push(Run {
                text: unescape_entities(&code),
                code: true,
                ..style.with_text(String::new())
            });
            i = end + 1;
            continue;
        }
        if c == '['
            && let Some((anchor_end, dest_end)) = link_at(chars, i)
        {
            flush(&mut plain, style, out);
            let uri: String = chars[anchor_end + 2..dest_end].iter().collect();
            let linked = Run {
                href: Some(uri),
                ..style.with_text(String::new())
            };
            scan(&chars[i + 1..anchor_end], &linked, out);
            i = dest_end + 1;
            continue;
        }
        plain.push(c);
        i += 1;
    }
    flush(&mut plain, style, out);
}

/// The closing `marker` for one opened just before `from`: the content must
/// start and end with non-space and be non-empty.
fn closing(chars: &[char], from: usize, marker: &[char]) -> Option<usize> {
    if chars.get(from).is_none_or(|c| c.is_whitespace()) {
        return None;
    }
    let mut search = from;
    while let Some(end) = find(chars, search, marker) {
        if end > from && !chars[end - 1].is_whitespace() {
            return Some(end);
        }
        search = end + 1;
    }
    None
}

/// What an emphasis marker does to the run it encloses.
type Apply = fn(&Run) -> Run;

/// The emphasis marker starting at `i`, with what it does to a run.
fn marker_at(chars: &[char], i: usize) -> Option<(&'static [char], Apply)> {
    const BOLD_ITALIC: &[char] = &['*', '*', '*'];
    const BOLD: &[char] = &['*', '*'];
    const ITALIC: &[char] = &['*'];
    const STRIKE: &[char] = &['~', '~'];
    let starts = |m: &[char]| chars[i..].starts_with(m);
    if starts(BOLD_ITALIC) {
        Some((BOLD_ITALIC, |s| Run {
            bold: true,
            italic: true,
            ..s.with_text(String::new())
        }))
    } else if starts(BOLD) {
        Some((BOLD, |s| Run {
            bold: true,
            ..s.with_text(String::new())
        }))
    } else if starts(ITALIC) {
        Some((ITALIC, |s| Run {
            italic: true,
            ..s.with_text(String::new())
        }))
    } else if starts(STRIKE) {
        Some((STRIKE, |s| Run {
            strike: true,
            ..s.with_text(String::new())
        }))
    } else {
        None
    }
}

/// The position of `pat` at or after `from`.
fn find(chars: &[char], from: usize, pat: &[char]) -> Option<usize> {
    chars
        .get(from..)?
        .windows(pat.len())
        .position(|w| w == pat)
        .map(|p| from + p)
}

/// A `[anchor](dest)` at `i`: the index of `]` and of the closing `)`.
/// Parentheses inside the destination balance, as in docling's scanner.
fn link_at(chars: &[char], i: usize) -> Option<(usize, usize)> {
    let anchor_end = find(chars, i + 1, &[']'])?;
    if chars.get(anchor_end + 1) != Some(&'(') {
        return None;
    }
    let mut depth = 0usize;
    for (k, &c) in chars.iter().enumerate().skip(anchor_end + 2) {
        match c {
            '(' => depth += 1,
            ')' if depth == 0 => return Some((anchor_end, k)),
            ')' => depth -= 1,
            _ => {}
        }
    }
    None
}

fn flush(plain: &mut String, style: &Run, out: &mut Vec<Run>) {
    if !plain.is_empty() {
        out.push(style.with_text(std::mem::take(plain)));
    }
}

/// Decode an HTML entity at the start of `chars`: the character and the
/// length consumed.
fn entity(chars: &[char]) -> Option<(char, usize)> {
    let semi = chars.iter().take(12).position(|&c| c == ';')?;
    let name: String = chars[1..semi].iter().collect();
    let decoded = match name.as_str() {
        "amp" => '&',
        "lt" => '<',
        "gt" => '>',
        "quot" => '"',
        "apos" => '\'',
        "nbsp" => '\u{a0}',
        _ => {
            let code = if let Some(hex) = name.strip_prefix("#x").or(name.strip_prefix("#X")) {
                u32::from_str_radix(hex, 16).ok()?
            } else {
                name.strip_prefix('#')?.parse().ok()?
            };
            char::from_u32(code)?
        }
    };
    Some((decoded, semi + 1))
}

fn unescape_entities(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '&'
            && let Some((decoded, len)) = entity(&chars[i..])
        {
            out.push(decoded);
            i += len;
            continue;
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain(t: &str) -> Run {
        Run {
            text: t.into(),
            ..Default::default()
        }
    }

    #[test]
    fn markers_become_runs_and_spacing_survives() {
        let runs = from_markdown("Hello **bold** and *it* and ~~gone~~.");
        assert_eq!(
            runs,
            vec![
                plain("Hello "),
                Run {
                    bold: true,
                    ..plain("bold")
                },
                plain(" and "),
                Run {
                    italic: true,
                    ..plain("it")
                },
                plain(" and "),
                Run {
                    strike: true,
                    ..plain("gone")
                },
                plain("."),
            ]
        );
    }

    #[test]
    fn markers_nest() {
        let runs = from_markdown("***both*** and **bold *and italic* end**");
        assert_eq!(
            runs,
            vec![
                Run {
                    bold: true,
                    italic: true,
                    ..plain("both")
                },
                plain(" and "),
                Run {
                    bold: true,
                    ..plain("bold ")
                },
                Run {
                    bold: true,
                    italic: true,
                    ..plain("and italic")
                },
                Run {
                    bold: true,
                    ..plain(" end")
                },
            ]
        );
    }

    #[test]
    fn links_carry_their_target_and_may_be_formatted() {
        let runs = from_markdown("See [the **spec** (v1)](https://x.org/a_(b)) now");
        assert_eq!(
            runs,
            vec![
                plain("See "),
                Run {
                    href: Some("https://x.org/a_(b)".into()),
                    ..plain("the ")
                },
                Run {
                    href: Some("https://x.org/a_(b)".into()),
                    bold: true,
                    ..plain("spec")
                },
                Run {
                    href: Some("https://x.org/a_(b)".into()),
                    ..plain(" (v1)")
                },
                plain(" now"),
            ]
        );
    }

    #[test]
    fn escapes_and_entities_decode_and_code_is_literal() {
        let runs = from_markdown(r"a\_b &amp; `x &lt; *y*` \*not bold\*");
        assert_eq!(
            runs,
            vec![
                plain("a_b & "),
                Run {
                    code: true,
                    ..plain("x < *y*")
                },
                plain(" *not bold*"),
            ]
        );
    }

    #[test]
    fn unmatched_markers_stay_literal() {
        assert_eq!(from_markdown("2 * 3 ** 4"), vec![plain("2 * 3 ** 4")]);
        assert_eq!(from_markdown("a [b] (c)"), vec![plain("a [b] (c)")]);
        assert_eq!(from_markdown("**"), vec![plain("**")]);
        assert_eq!(from_markdown("a ** b**"), vec![plain("a ** b**")]);
    }

    #[test]
    fn inline_runs_convert_with_script() {
        let runs = from_inline_runs(&[InlineRun {
            text: "2".into(),
            script: Script::Super,
            ..Default::default()
        }]);
        assert_eq!(
            runs,
            vec![Run {
                script: Position::Super,
                ..plain("2")
            }]
        );
    }
}

/// The runs of a node that carries both structured runs and the Markdown
/// text docling built from them.
///
/// The runs are the text: exact characters, exact spacing, and the underline
/// and script that have no Markdown marker. The Markdown is where the
/// hyperlinks are, since [`InlineRun`] has no field for one, and its spacing
/// is docling's own, with a space inserted at every run boundary. So the runs
/// are kept as they are and the link targets are copied onto them, aligned
/// character by character with whitespace set aside; a space between two
/// links, or between a link and plain text, is linked only when both sides
/// are. If the two sides do not align, the runs are used as they are.
pub fn from_group(md_text: &str, runs: &[InlineRun]) -> Vec<Run> {
    let structured = from_inline_runs(runs);
    let scanned = from_markdown(md_text);
    let hrefs: Vec<(char, Option<&str>)> = scanned
        .iter()
        .flat_map(|r| {
            r.text
                .chars()
                .filter(|c| !c.is_whitespace())
                .map(move |c| (c, r.href.as_deref()))
        })
        .collect();
    let aligned = structured
        .iter()
        .flat_map(|r| r.text.chars().filter(|c| !c.is_whitespace()))
        .eq(hrefs.iter().map(|&(c, _)| c));
    if !aligned {
        return structured;
    }
    let mut out = Vec::new();
    let mut at = 0;
    // The link of the previous character, across run boundaries.
    let mut prev: Option<&str> = None;
    for run in structured {
        let mut piece = String::new();
        let mut current: Option<Option<&str>> = None;
        for c in run.text.chars() {
            let href = if c.is_whitespace() {
                let next = hrefs.get(at).and_then(|&(_, h)| h);
                if prev == next { prev } else { None }
            } else {
                let (_, h) = hrefs[at];
                at += 1;
                h
            };
            if current.is_some_and(|cur| cur != href) {
                push_piece(&mut out, &run, &mut piece, current.flatten());
            }
            current = Some(href);
            prev = href;
            piece.push(c);
        }
        push_piece(&mut out, &run, &mut piece, current.flatten());
    }
    out
}

fn push_piece(out: &mut Vec<Run>, run: &Run, piece: &mut String, href: Option<&str>) {
    if piece.is_empty() {
        return;
    }
    out.push(Run {
        href: href.map(str::to_string),
        ..run.with_text(std::mem::take(piece))
    });
}

#[cfg(test)]
mod group_tests {
    use super::*;

    #[test]
    fn links_come_from_the_text_and_underline_from_the_runs() {
        let runs = vec![
            InlineRun {
                text: "See ".into(),
                ..Default::default()
            },
            InlineRun {
                text: "the spec".into(),
                underline: true,
                ..Default::default()
            },
            InlineRun {
                text: " now".into(),
                ..Default::default()
            },
        ];
        let merged = from_group("See [the spec](https://x.org/) now", &runs);
        assert_eq!(
            merged,
            vec![
                Run {
                    text: "See ".into(),
                    ..Default::default()
                },
                Run {
                    text: "the spec".into(),
                    underline: true,
                    href: Some("https://x.org/".into()),
                    ..Default::default()
                },
                Run {
                    text: " now".into(),
                    ..Default::default()
                },
            ]
        );
    }

    #[test]
    fn misaligned_text_falls_back_to_the_runs() {
        let runs = vec![InlineRun {
            text: "other".into(),
            underline: true,
            ..Default::default()
        }];
        let merged = from_group("**bold**", &runs);
        assert_eq!(
            merged,
            vec![Run {
                text: "other".into(),
                underline: true,
                ..Default::default()
            }]
        );
    }

    #[test]
    fn a_space_inside_a_link_stays_linked_and_one_beside_it_does_not() {
        let runs = vec![
            InlineRun {
                text: "go ".into(),
                ..Default::default()
            },
            InlineRun {
                text: "there".into(),
                bold: true,
                ..Default::default()
            },
            InlineRun {
                text: " now ".into(),
                ..Default::default()
            },
        ];
        let merged = from_group("go [**there** now](u)", &runs);
        let shape: Vec<(&str, Option<&str>)> = merged
            .iter()
            .map(|r| (r.text.as_str(), r.href.as_deref()))
            .collect();
        assert_eq!(
            shape,
            vec![
                ("go ", None),
                ("there", Some("u")),
                (" now", Some("u")),
                (" ", None),
            ]
        );
    }
}
