---
name: waddle
description: >-
  Write an ODT or DOCX from a document docling.rs has already read, with `waddle`. Match on
  the situation rather than the verb: "turn this DocLang back into Word", "I have docling
  JSON and need a .docx", "Duckling gave me a .dclg, I need something I can send", "get this
  into LibreOffice", "convert the docling output to an office document". Input is bare
  DocLang (`.dclg`), a DocLang archive (`.dclx`) or docling's JSON export; output is one
  OpenDocument Text or Word package, plain and well structured, never overwriting a file.
  Not for reading office documents into a model (docling.rs, or Duckling on the desktop),
  not for editing an existing .docx, and not for reproducing how a source document looked:
  the model carries no styles or layout, and waddle writes a stock look on purpose.
---

# waddle — a DoclingDocument to ODT or DOCX

`waddle` takes what docling.rs produced and writes an office package from it. docling.rs
reads Word, PowerPoint, Excel, PDF, HTML and some forty other formats into a
`DoclingDocument` and writes Markdown, JSON, DocLang and LaTeX from it, but no office
format; waddle is the other direction. Every node the model holds goes into the construct
the target has for it, and what the target cannot hold is said on stderr.

The authoritative sources for waddle's behaviour are the binary itself (`waddle --help`)
and the [README](https://github.com/excelano/waddle/blob/main/README.md); if anything here
conflicts with them, they win. If a flag described here is missing from `waddle --help`,
the installed copy predates it: upgrade with `sudo apt install --only-upgrade waddle`
(Debian/Ubuntu), `brew upgrade waddle` (macOS), or by re-running the install one-liner
from the README.

## Where waddle sits

Three tools share the DocLang model. [Duckling](https://github.com/excelano/duckling)
converts documents with docling.rs on the desktop and writes DocLang, JSON, Markdown and
LaTeX. [Segler](https://github.com/excelano/segler) reviews and corrects DocLang. waddle
writes the office document. Convert, correct, publish. Reading an office file is never
waddle's job; hand that to docling.rs (`docling file.docx --to dclx`) or Duckling.

## Running it

```sh
waddle report.dclg --to odt              # writes report.odt beside the input
waddle report.json --to docx -o out/     # into an existing directory
waddle report.dclx --to docx -o final.docx
cat report.json | waddle - --to odt -o report.odt   # stdin needs -o
waddle report.dclg --to odt --dry-run    # names the path and the warnings, writes nothing
waddle report.dclg --to odt --strict     # refuses if anything would be dropped or degraded
```

The input's format is told from its first bytes, not its extension. The path written goes
to **stdout**, one line; every diagnostic goes to **stderr**. Nothing is ever overwritten:
if `report.odt` exists the result is `report (1).odt`, and stdout says so.

Exit `0` is success, and a run that dropped or degraded nodes is a success: the warnings
are on stderr, grouped by kind with a count, and the package is complete. `1` is bad input,
a file that cannot be read or is none of the three formats, or, under `--strict`, a document
that cannot be carried whole. `2` is a bad command line, including `-` without `-o`.

## Pictures

docling's DocLang and JSON readers hand back every picture without its bytes; waddle finds
them itself. A `data:` URI in the document decodes in place, an `assets/` path is read
beside a bare `.dclg` or out of a `.dclx`, and a path that leaves the input's own
directory is refused. So keep a `.dclg` next to its `assets/` folder, as Duckling writes
them. A picture that cannot be found is written as a small grey placeholder and reported as
`picture: written as a placeholder`; that line with a count is how a document with forty
missing images reads.

## What degrades, and why

waddle emits the construct docling.rs's own reader for the target keys on, so a package
reads back into the document it came from as far as that reader allows. Where the model or
the reader has no room, waddle says so:

- `furniture: dropped, on the furniture layer` (also `notes`, `invisible`): page headers
  and footers, navigation chrome, reviewer comments and hidden sheets have no place in the
  first release.
- `picture: written as a placeholder`: no bytes were found for it.
- `comment_section: dropped, not supported by this target`, `page_furniture: …`: reviewer
  comments and page furniture, later.

A checkbox is a real content control in DOCX and a `☐`/`☑` prefix in ODT. Code is a
monospace block in both. Captions go before their table or picture. Numbered lists start
where the document says they start. The full list of what each target keeps is in the
repository's `DESIGN.md`.

## Under --strict

`--strict` is for a caller who needs the whole document or nothing: it prints the
warnings, names the path it did not write, and exits 1 with nothing written. Without it,
read stderr and decide; the package is still complete for everything it does hold.

## From Rust

The library is `waddle-core` (`waddle_core::odt::write(&doc)` and
`waddle_core::docx::write(&doc)`), which takes a `docling_core::DoclingDocument` and
returns the package bytes and the warnings. It depends on `docling-core` alone: no
readers, no ML, no C. See `reference.md` for the flag surface and the details.
