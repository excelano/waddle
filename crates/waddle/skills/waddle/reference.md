# waddle reference

The flag surface and the details. The binary (`waddle --help`) and the
[README](https://github.com/excelano/waddle/blob/main/README.md) are authoritative; a
flag here that `waddle --help` does not list means the installed copy predates it.

## Invocation

```sh
waddle <INPUT> --to <FORMAT> [-o PATH] [--dry-run] [--strict] [--color MODE]
waddle --install-skill | --uninstall-skill
```

| Flag | Effect |
|---|---|
| `<INPUT>` | a `.dclg`, `.dclx` or docling JSON file; `-` reads standard input |
| `--to <FORMAT>` | `odt` or `docx`, required |
| `-o, --output <PATH>` | a file to write, or an existing directory to write into; the default is beside the input with the target's extension; required with `-` |
| `--dry-run` | print `would write PATH` and the warnings; write nothing |
| `--strict` | refuse to write, exit 1, if any node would be dropped or degraded |
| `--color auto\|always\|never` | colour on stderr; `auto` needs a terminal and no `NO_COLOR`; `always` overrides `NO_COLOR` |
| `--install-skill`, `--uninstall-skill` | write or remove this skill under `~/.claude/skills/waddle/` |
| `-h`, `-V` | help, version |

## Input detection

The first bytes decide: `PK` is the DocLang archive (a zip), a `{` after optional
whitespace is docling JSON, a `<` is bare DocLang. A UTF-8 byte-order mark is skipped.
Anything else is exit 1 with `is not DocLang, a DocLang archive or docling JSON`. The
extension is never consulted, so a misnamed file still converts.

## Output path

Beside the input by default, same stem, the target's extension. `-o` naming an existing
directory writes inside it, named by the input's stem, or `waddle.<ext>` for standard
input. `-o` naming anything else is the file path, taken as given. A taken name is
numbered: `name (1).ext`, `name (2).ext`, the first free one. The path written is the
only thing on stdout.

## Warnings

One line per distinct warning, with a count when it occurred more than once, under a
first line that says how many nodes were, or under `--dry-run` and `--strict` would be,
dropped or degraded:

```
waddle: 3 nodes dropped or degraded:
  furniture: dropped, on the furniture layer (2)
  picture: written as a placeholder
```

The node names are DocLang's: `paragraph`, `list_item`, `picture`, `furniture`,
`comment_section`, `page_furniture`. The reasons are three: `dropped, on the <layer> layer`
for content on the furniture, notes or invisible layer; `dropped, not supported by this
target` for a node the release has no construct for; `written as a placeholder` for a
picture without bytes.

Asset problems are reported the same way: `N picture sources in the input and M pictures
in the document; pictures are left as placeholders` when the count does not pair, and
`picture N: <reason>` when one source cannot be read, is not an image format, or names a
path outside the input's directory.

## Exit codes

`0` success, including a dry run and a run with warnings. `1` bad input: unreadable,
unrecognised, unreadable by docling, unwritable destination, or `--strict` refused.
`2` bad command line: unknown flag, missing `--to`, `-` without `-o`.

## What each target keeps

Both: title and headings (docling's level 1 is the document title, level N is the target's
heading N-1, which is the convention both docling readers use), paragraphs with bold,
italic, underline, strikethrough, sub and superscript, inline code and links, nested
numbered and bulleted lists with their start numbers, tables with column and row spans and
a header band, pictures with their bytes or a placeholder, captions before their object,
code blocks, checkboxes, page breaks, key-value regions as two-column tables, charts as
their data table or a placeholder.

ODT only: a checkbox is a glyph prefix; inline code is a monospace span.

DOCX only: a checkbox is a `w14:checkbox` content control; a blank paragraph is kept as
one; a link inside a table cell is preceded by an empty underlined run, which is what
keeps docling's DOCX reader from flattening the cell and losing the link.

Neither target carries, in this release: page headers and footers, reviewer comments,
anything on the furniture, notes or invisible layers, or formulas as equations (the LaTeX
source is written as code).

## The library

```rust
let out = waddle_core::odt::write(&doc)?;   // or waddle_core::docx::write
std::fs::write("report.odt", &out.bytes)?;
for warning in &out.warnings { eprintln!("{warning}"); }
```

`waddle_core` re-exports `docling_core`, so a caller uses the model version the crate was
built against. `Output { bytes, warnings }`; `Warning { node, reason }` with
`Reason::{Layer(&str), Unsupported, Placeholder}`; `Target::{Odt, Docx}` with
`extension()` and `media_type()`.
