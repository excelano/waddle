# waddle — design

The reasoning behind the crate, in the order the decisions were made, each
dated. `CLAUDE.md`, once there is code, is the short guide; this is the long
one. `git log` is the record of how each section came to say what it says.

Status: pre-code. Every claim about docling.rs below was checked on
2026-09-08 against the clone at `~/clones/docling.rs` at v1.37.4, which is
byte-identical to the published `docling` and `docling-core` crates of that
version. File references are to that tree.

## 1. What this repository is

A DoclingDocument in, an office package out. docling.rs converts DOCX, PPTX,
XLSX, ODT, ODS, ODP, PDF, HTML, EPUB and some forty other formats into a
`DoclingDocument` and exports it as Markdown, docling JSON, DocLang, a
DocLang archive or LaTeX. Nothing in that tree writes an office format:
its complete writer inventory is the CLI's `--to` list, and the only
`zip::ZipWriter` outside test helpers is the one that packs a `.dclx`. The
demand exists but is faint; the one trace upstream is docling#2243 on the
Python project, a user who translated a DOCX through the JSON export and
asked how to get back to DOCX, which was left unanswered and marked stale.
The practical route today is DocLang to Markdown to Pandoc, which drops the
structure DocLang carries and adds a Haskell binary to a Rust pipeline.

waddle is that missing writer: pure Rust, offline, one workspace of two
crates under `github.com/excelano`, MIT to match docling.rs.

`waddle-core` is the library. It depends on `docling-core` for the model and
on nothing else that is docling's. It writes an OpenDocument or Office Open
XML package from a `DoclingDocument` and returns the bytes. No CLI, no
window, no ML, no reader.

`waddle` is the binary. It reads a DocLang file, a DocLang archive or a
docling JSON file through docling.rs's own readers, hands the document to
the library, and writes one package. It follows the fleet's agent-facing CLI
contract in `~/notes/agent_cli_conventions.md`: three exit codes, `-` for
stdin, colour only on a terminal, a first line of error text that names the
correction. It never overwrites, and it says what it will drop before it
writes.

Duckling later depends on `waddle-core` and adds the new targets to its
output picker. Duckling's principle that it adds no conversion logic of its
own holds, because the conversion is this crate's. Segler is the sibling on
the other side: convert with Duckling, review and correct with Segler,
publish with waddle.

**2026-09-08, the name.** `waddle` and `waddle-core` are unclaimed on
crates.io and `excelano/waddle` does not exist; the GitHub repositories of
that name are a Club Penguin archive, a GPS parser and a Minecraft mod,
none in this space. The crate, the binary and the presented name are all
lowercase `waddle`, as for the rest of the CLI fleet; it is what a duckling
does.

**2026-09-08, not a fork and not a pull request.** docling.rs scopes itself
to parity with Python docling, which has no writer and no plan for one, and
its readers ignore what they do not recognise by design. A writer is a
different kind of artefact from a reader and belongs in its own crate. If
upstream ever wants the capability, the library is the thing to offer, and
keeping it a separate crate with docling-core as its only docling
dependency is what makes that offer cheap to accept.

## 2. What is being written

`DoclingDocument` (`crates/docling-core/src/document.rs`) is a struct with a
public `nodes: Vec<Node>` and a handful of document-level fields: name, two
Markdown flags, a `links` list only the PDF pipeline fills, and an optional
confidence report. `Node` is an enum of twenty-six variants. The ones that
carry content are heading with a level, paragraph, checkbox item, list item
with ordering, level, number and marker, code with an optional language,
table, picture with an optional caption and optional bytes, formula, chart,
group, form field region, inline group, text dump, page furniture, page
break and page info. The rest are wrappers: furniture, located, commented,
comment section and DocLang-only, each holding an inner node or an index.

The model is semantic and flat. It carries no styles, page geometry,
numbering definitions, themes, sections, fonts, or headers and footers as
such. A table is a dense grid of strings with an optional structure overlay
for spans and header bands, and `Table::derive_cells()` synthesises anchor
cells with row and column spans from that overlay; the declarative readers
never fill `cells` themselves. Inline formatting lives on `InlineRun`: bold,
italic, underline, strike, sub or superscript, inline code, inline formula.
A hyperlink is deliberately not on a run; it survives only on a list item,
on a picture caption, or as `[text](url)` inside a paragraph's Markdown
string.

Consequence: the writers are not the docling.rs readers run backwards. They
are fresh serialisers from a semantic model, the same category as
docling-core's Markdown and LaTeX exports, targeting a zipped XML package
instead of a text file. Output is deliberately plain, the document you would
get from a well-structured Markdown file opened in Writer or Word with a
stock style sheet. Fidelity to the source document's appearance is out of
scope because there is nothing in the model to recover it from.

The docling.rs readers for the same formats (`crates/docling/src/backend/`
`odf.rs`, `docx.rs`, `pptx.rs`, `xlsx.rs`) are the specification for which
XML constructs map to which node variants. The writer emits the constructs
the reader recognises, so that reading the output back yields the same
nodes. §6 records where that is impossible because the reader does not look.

## 3. Dependencies

**2026-09-08, `waddle-core` depends on `docling-core` alone.** The concept
had the library depending on `docling` for its readers. The readers are the
binary's concern: Duckling already holds a `DoclingDocument` and needs no
reader, and a library whose only docling dependency is the model crate is
the one upstream could adopt. `docling-core` carries `serde_json` and `sha2`
and nothing platform-specific; it has no `cfg(target_arch)` gate anywhere
and CI checks the declarative converters on `wasm32-unknown-unknown` on
every push (`.github/workflows/ci.yml`, the job named *wasm32*).

The binary takes `docling` with `default-features = false`, which leaves
the pure-Rust declarative converters. The DocLang and docling-JSON readers
are declared without feature gates (`backend/mod.rs`), and the converter's
dispatch to them is likewise ungated; the public path is
`DocumentConverter::new().convert(SourceDocument::from_file(path)?)?.document`
with the format sniffed from content and extension. Measured 2026-09-08 on
this machine with the shared target directory: a crate depending on
`docling` without defaults, `docling-core`, `quick-xml` and `zip` builds
its full dependency tree in about seventeen seconds, and `cargo tree`
shows no `-sys` crate, no `cc`, no pdfium, no ONNX Runtime, no oniguruma.
That satisfies `~/notes/pure_rust_preference.md` without an exception, and
`waddle-core` must keep it that way: a change that adds a C dependency is a
decision to take with David, not a cargo add.

`zip` assembles the package, without its default features, which pull
zstd, xz and bzip2, and with the zlib-rs deflate backend, which is Rust.
ODF requires the `mimetype` entry first and stored uncompressed, which
`zip` supports per entry.

**2026-09-08, the XML is pushed as strings.** The concept named `quick-xml`
for the writer. docling-core's own serialisers build their output as
strings, the vocabulary of each package is fixed and small, and what an XML
writer library would add over that is an escape function, which is ten
lines in `xml.rs`. The static parts, `styles.xml` and the manifest, are
files included at compile time and edited as XML.

**2026-09-08, keeping up with docling.rs.** Measured on the clone: seventy
tags in the thirty days to 2026-09-08, and `document.rs` changed seventeen
times in sixty days. Between 1.35 and 1.36 the `Group` variant gained two
fields and `CommentSection` gained one, inside a minor release, which
breaks any exhaustive match. Nothing in the model or the office readers
changed from 1.36.0 to 1.37.4; those releases were PDF pipeline work. Two
rules follow. The writers match `Node` exhaustively with no wildcard arm, so
a new variant or field upstream fails the build here rather than being
dropped silently; the compile error is the notification. And CI carries a
job that runs `cargo update -p docling-core` before building, so drift is
found the day a release lands rather than the day Duckling next bumps its
pin. Dependabot's weekly pass covers the lockfile. The pin is
`docling-core = "1.37"`, and it only moves forward.

## 4. Inputs, and what the readers leave behind

The CLI's inputs are docling.rs's three lossless serialisations: bare
DocLang (`.dclg`), the DocLang archive (`.dclx`, an OPC zip with
`document.xml`, optional `pages/N.png` and `assets/`), and docling JSON.
All three are read by docling.rs and reach waddle as a `DoclingDocument`.
Three facts about those readers shape the binary, all checked on
2026-09-08.

**Picture bytes do not come back.** The DocLang reader's `parse_picture`
reads the caption, the chart class and the layer and, by its own doc
comment, does not re-import the `<src>` payload; the JSON reader hard-codes
`image: None`. The ODF reader returns `image: None` for any picture inside
the package. Only the DOCX, PPTX, HTML and XLSX readers and the PDF pipeline
populate `PictureImage`. So a `DoclingDocument` that arrives through the
CLI has captions and placeholders and no pixels, while one that arrives
through Duckling has the bytes because Duckling converted the original.

**2026-09-08, the binary resolves picture assets itself.** After docling's
reader has produced the document, the CLI reads the picture sources from
the input in document order and pairs them with the `Picture` nodes by
ordinal: a `data:` URI decodes in place, an `assets/` path resolves beside
a bare `.dclg` or as an entry inside a `.dclx`, and a JSON input's
`pictures[i].image.uri` is a data URI. Pairing by ordinal is the weak
point, and it holds because the reader emits pictures in document order;
the test suite pins that. This is a workaround for an upstream gap and the
issue to raise is that `DoclangBackend` should read `<src>`; when it does,
this code is deleted. None of this is the library's. It writes the
bytes a `Picture` carries and a placeholder when it carries none.

**Inline formatting arrives as Markdown markers.** The DocLang reader
flattens `<bold>` to `**`, `<italic>` to `*`, `<strikethrough>` to `~~`,
`<code>` to backticks, and a link to `[text](url)`; underline, subscript and
superscript flatten to plain text with no marker at all, and it constructs
no `InlineRun`. The DOCX, ODF, HTML and JATS readers do construct runs,
wrapped in `Node::InlineGroup { runs, md_text }`; a paragraph with a single
plain run collapses to `Node::Paragraph`. The Markdown and PPTX readers
construct none.

**2026-09-08, a scanner of docling's dialect, and the runs are the text.**
The concept's first answer was `pulldown-cmark`. Building the writer showed
why not: a paragraph string is inline by definition, and a Markdown parser
reads block structure, raw HTML and entities into it. docling-core's own
`inline_runs_from_markdown` was the other candidate; it keeps no link
target and trims every run. So `inline.rs` is a scanner of the dialect
docling emits and nothing more: `***`, `**`, `*`, `~~`, backticks,
`[text](url)`, backslash escapes and the HTML entities docling writes for
`&`, `<` and `>`, with CommonMark's rule that a marker opens against
non-space and closes after it, so that `2 * 3 ** 4` stays arithmetic.

A node that carries structured runs also carries the Markdown docling built
from them, and each side knows something the other does not. The runs have
the exact text and the underline and script that have no marker; the
Markdown has the hyperlinks, since `InlineRun` has no field for one, and
its spacing is docling's, with a space inserted at every run boundary. The
first merge took the text from the Markdown and was caught by the
fixed-point test: each trip through the reader added spaces. The rule now
is that the runs are the text and only the link targets are copied onto
them, aligned character by character with whitespace set aside. A space
between two runs is linked only when both sides are.

**2026-09-08, Segler's DOM is the second front end, later, if ever.**
`segler-core` is the fleet's lossless DocLang reader and would return
underline, links and picture sources that docling's reader drops. Taking it
would mean a second input model beside `DoclingDocument` and would sever the
Duckling path, so it is not the first release. It is the answer if the
DocLang path ever needs more than docling's reader gives and upstream will
not take the fixes.

## 5. Node to target mapping

Heading levels follow docling's convention, which both office readers
share: level 1 is the document title and level N above 1 is the target's
heading N-1. The ODF reader maps a `Title`-styled paragraph to level 1,
a `Subtitle` to level 2, and `text:h` at `outline-level` L to L+1. The DOCX
reader maps a `Title` style to level 1 and both the `Heading1` style name
and `w:outlineLvl` 0 to level 2. A writer that emitted `Heading 1` for
level 1 would read back one level deeper than it went in.

### ODT

| Node | ODT |
|---|---|
| Heading level 1 | `text:p` with the `Title` paragraph style |
| Heading level N > 1 | `text:h text:outline-level="N-1"` with `Heading_20_N-1` |
| Paragraph, InlineGroup | `text:p`; runs as `text:span` on automatic text styles; `text:a xlink:href` for links; `text:line-break`, `text:tab`, `text:s` for whitespace |
| ListItem | nested `text:list` / `text:list-item`, rebuilt from the flat level sequence; ordered or bullet chosen by the list style's `text:list-level-style-number` or `-bullet`; `text:start-value` when the number restarts; a list closes at the first non-list node |
| CheckboxItem | `text:p` prefixed `☐ ` or `☑ `; ODF text has no checkbox and the reader detects none |
| Code | one `text:p` per line in the `Preformatted_20_Text` style on a monospace font; the reader has no code detection, see §6 |
| Table | `table:table` with `table:table-column` per column, `table:table-header-rows` around the rows the structure marks as header, spans from `derive_cells()` as `table:number-columns-spanned` / `-rows-spanned` with `table:covered-table-cell` under them; `office:value-type="string"` on every cell |
| Picture | `draw:frame` anchored as paragraph with `draw:image xlink:href="Pictures/…"` and the bytes in the package, or an empty frame with `svg:desc` when the node carries no bytes; caption as a following `text:p` in the `Caption` style |
| Chart | its table, then the caption |
| Formula | the LaTeX source as a `text:p` in the code style; MathML is a later release |
| FieldRegion | a two-column table of key and value |
| TextDump | paragraphs split on blank lines |
| PageBreak | an empty `text:p` in an automatic style with `fo:break-before="page"` |
| Group | transparent, unless its layer is furniture, notes or invisible, in which case it and its children are dropped |
| Located, Commented, DoclangOnly | the inner node |
| Furniture, PageFurniture, CommentSection, PageInfo | dropped |

### DOCX

| Node | DOCX |
|---|---|
| Heading level 1 | `w:pStyle w:val="Title"` |
| Heading level N > 1 | `w:pStyle w:val="HeadingN-1"` with `styles.xml` naming it `heading N-1` and carrying `w:outlineLvl w:val="N-2"` |
| Paragraph, InlineGroup | `w:p`; runs as `w:r` with `w:rPr` (`w:b`, `w:i`, `w:u`, `w:strike`, `w:vertAlign`); links as `w:hyperlink r:id` with a relationship; `w:br`, `w:tab` |
| ListItem | `w:numPr` with `w:ilvl` and a `w:numId`; ordered or bullet decided in `numbering.xml` by `w:numFmt`, which is the only signal the reader uses; one `w:num` per list so numbering restarts |
| CheckboxItem | a `w:sdt` with `w14:checkbox` and `w14:checked`, which is what the reader detects |
| Code | one `w:p` per line in a `SourceCode` style whose `w:rFonts` names Consolas; the reader keys on that style id and on the font |
| Table | `w:tbl` with `w:tblGrid`; `w:gridSpan` and `w:vMerge` from `derive_cells()`; `w:tblHeader` on header rows for Word's sake, though the reader ignores it |
| Picture | `w:drawing` with `a:blip r:embed` and the bytes under `word/media/`, or a placeholder paragraph when the node carries no bytes; caption as a following `Caption` paragraph |
| Chart, Formula, FieldRegion, TextDump, Group and the wrappers | as for ODT |
| PageBreak | `w:br w:type="page"` |

Both packages carry a stock style sheet written by waddle: the heading
family, Title, Subtitle, Caption, the code style, one bullet and one
numbered list definition, a plain table style, and a body font. Nothing is
inherited from any template in the first release; §9 has the template
option.

## 6. Round trip, and what cannot round-trip

The conformance test is a round trip through docling.rs's readers: build a
`DoclingDocument`, write the package, read it back with
`DocumentConverter` as `InputFormat::Odt` or `InputFormat::Docx`, and
compare. docling.rs's own `dclx_roundtrip.rs` compares the two Markdown
exports. waddle's harness makes three comparisons, weakest to strongest,
settled on 2026-09-08 when the first trip ran. The Markdown exports agree
once whitespace is set aside, and no closer, because the ODF reader rebuilds
a paragraph's Markdown from its runs with docling's own spacing. The
structured runs come back with their text and formatting, which is where
underline and superscript are checked. And a second trip is a fixed point:
what the reader produced, written and read again, is equal node for node.
The fixed point is the test that catches a writer quietly changing a
document, and it caught the first one.

Some diffs are inherent because the reader does not look, and the test
suite lists them by name rather than tolerating diffs in general. Read on
2026-09-08 from the readers:

Neither the DOCX nor the ODF reader associates a caption with a picture or
table; every `Picture` and `Table` comes back with `caption: None` and a
`Caption`-styled paragraph comes back as a paragraph. Only the JATS reader
sets a table caption anywhere in the tree.

Neither reader reads `w:tblHeader` or `table:table-header-rows`; the header
band is always row 0 and only row 0. A table whose structure marks two
header rows loses the second.

The ODF reader has no code detection and no checkbox detection, so a
`Code` node written to ODT returns as paragraphs and a `CheckboxItem` as a
paragraph beginning with a box character. The DOCX reader recovers both.

Underline, subscript and superscript survive the DOCX and ODF readers as
runs and are lost by the DocLang reader, so they hold through an
office round trip and not through a DocLang one.

The ODF reader resolves a link's character style like any span's, so the
stock `Internet link` style carries colour and no underline; an underline
there would read back as an underlined run. Inline code is a monospace
span that the reader has no field for, so it returns as plain text.

Everything on the furniture, notes and invisible layers is dropped on the
way out and cannot return. Reviewer comments are in that set for the first
release; §9 has them as the obvious second.

The writer reports each of these as a warning naming the node and the
reason, and `--strict` turns any of them into a refusal.

## 7. Targets, in order

ODT first. One `content.xml`, a small `styles.xml`, `META-INF/manifest.xml`,
`mimetype` stored first, `Pictures/` when there are bytes. Least ceremony
of the four, and the spec is readable.

DOCX second. `[Content_Types].xml`, `_rels/.rels`, `word/document.xml`,
`word/_rels/document.xml.rels`, `word/styles.xml`, `word/numbering.xml`,
`word/media/`. The package plumbing from ODT carries over; the XML does not.

ODS and XLSX third, and only for tables: one sheet per `Table` or `Chart`
node, the caption or a generated name as the sheet name, header rows bold.
Cheap once the two package writers exist, low priority.

ODP and PPTX last and not in the first release. A slide needs a splitting
rule. Documents that came from PPTX, XLSX or a PDF carry `PageBreak` nodes
and split there; documents that did not have only headings to split on.
The rule is undecided; §9.

## 8. The binary

    waddle input.dclg --to odt              # writes input.odt beside the source
    waddle input.json --to docx -o out/     # into a folder
    waddle - --to odt -o report.odt         # from stdin, to a named file
    waddle input.dclg --to odt --dry-run    # lists what would be written and dropped
    waddle input.dclg --to odt --strict     # refuse rather than drop

Input format is detected from content, which docling's converter already
does. Output never overwrites: `report.odt` present means `report (1).odt`,
the rule Duckling's library implements. Exit 0 on success, and a run that
dropped nodes with warnings is a success, as is a dry run; exit 1 when the
input cannot be read or, under `--strict`, cannot be represented; exit 2
when the command line is wrong. `--color auto|always|never` with `NO_COLOR`
honoured, diagnostics on stderr, nothing on stdout but the path written.
The contract is the fleet's and `--help` states it.

## 9. Open threads

The upstream issues waddle's tests will produce, David's to raise on
docling.rs: the DocLang reader ignores `<src>`; no office reader associates
captions; `w:tblHeader` and `table:table-header-rows` are ignored; the ODF
reader detects neither code nor checkboxes. Each is a round-trip diff this
repository documents until it is fixed there.

Reviewer comments. `CommentSection` and `Commented` are in the model since
1.34 and 1.36, DOCX has `comments.xml` and ODF has `office:annotation`,
and the DOCX reader already reads them back. The first release drops them
with a warning; the second should write them.

Headers and footers. `PageFurniture` carries footer text and a location.
ODT's `styles.xml` master page and DOCX's `header1.xml` can hold it. Not in
the first release.

A template. "Use this `.odt`'s `styles.xml`" or a `reference.docx` in the
Pandoc manner is the step that makes the output institutional, and
`~/notes/office_convert_branding.md` already argues for prepared reference
files over generated themes. Worth doing once the plain output is right.

The slide rule for ODP and PPTX, per §7.

Formulas as MathML in ODT and OMML in DOCX rather than LaTeX source.

## 10. Non-goals

Reproducing the look of the original document. Editing or merging existing
office files; input is a `DoclingDocument`, output is a new package.
Reading office formats, which is docling.rs's job. A window, which is
Duckling's.

## 11. Duckling integration

Duckling adds `waddle-core` as a dependency at a version whose
`docling-core` unifies with its own `docling`'s. Its output picker gains
ODT and DOCX, with ODS and XLSX when a batch is tables. The queue already
accepts DocLang and JSON through docling.rs, so nothing changes on input.
Pictures invert: the bytes go into the package rather than beside the
file, and Duckling's own asset pairing is not involved because the
`DoclingDocument` it holds already carries them. `DESIGN.md` there gets one
paragraph: Duckling composes docling.rs and waddle, and the principle that
it adds no conversion logic is unchanged.
