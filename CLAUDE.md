# CLAUDE.md

Guidance for Claude Code working in `waddle`. Short because `DESIGN.md` is
where the reasoning lives; read that before touching anything.
This repo's prose documents follow the fleet documentation standard in
`~/notes/doc_standards.md`.

---

## What this is

A DoclingDocument in, an office package out. docling.rs reads some forty
document formats into a `DoclingDocument` and writes Markdown, JSON, DocLang
and LaTeX from it; nothing in that tree writes an office format. This is the
writer. Two crates in one workspace:

- `crates/waddle-core` — the library. Takes a `docling_core::DoclingDocument`,
  returns the bytes of an ODT or DOCX package and the warnings raised on the
  way. Depends on `docling-core` and on nothing else of docling's.
  `#![forbid(unsafe_code)]`.
- `crates/waddle` — the CLI, binary `waddle`. Reads DocLang, DocLang archives
  and docling JSON through docling.rs's own readers and writes one package.

**Three documents, three authorities.** docling.rs's office *readers*
(`crates/docling/src/backend/{odf,docx,pptx,xlsx}.rs` in the clone) are the
authority on which XML constructs map to which node, and so on what a writer
must emit for its output to read back as the same document. `DESIGN.md` here
is the authority on this crate: §5 is the mapping, §6 what cannot round-trip
and why. `git log` is the record of why everything is the way it is, and it is
written to be read.

The round-trip corpus is `crates/docling/tests/data/<format>/sources/` in a
checkout of docling.rs, which moves several times a week — pull before reading
it. Where that checkout lives is a fact about a machine and not about this
repository, so it is not written down here.

---

## Commands

    cargo build --workspace
    cargo test --workspace                  # the corpus tests skip without the clone
    cargo clippy --workspace --all-targets -- -D warnings   # must be silent
    cargo fmt --check
    cargo run -p waddle -- FILE --to odt

`WADDLE_DUMP=dir` keeps every package a test writes, named by the test, for
opening in Writer by hand. `WADDLE_CORPUS_WRITER=1` opens every corpus
package in headless Writer, a second per document. The build directory is
the shared one in `~/.cargo/config.toml`, not `target/`. `cargo update -p docling-core` followed by the build is what
`docling-latest` in CI does; run it when docling.rs has released.

---

## Rules

**Every match on `Node` is exhaustive.** No wildcard arm, anywhere, ever. A
variant or field docling.rs adds must fail the build here and be decided, not
fall through and be dropped. `node::kind` is the first such match and the
model for the rest.

**The library depends on `docling-core` alone.** Not on `docling`, which is
the binary's for reading input. A change that adds a C dependency, or a crate
that fails on `wasm32-unknown-unknown`, to `waddle-core` is a decision to take
with David; the fleet's stance is `~/notes/pure_rust_preference.md`.

**The writer emits what the reader recognises.** Before mapping a node to
XML, read the docling.rs reader for that format and emit the construct it
keys on: the style ids, the outline levels, the numbering formats. The test
is the round trip, and Markdown equality is the floor, DocLang equality the
target. Where the reader does not look (§6), the diff is listed by name in the
test, not tolerated in general.

**Level 1 is the title.** Both office readers map a document title to heading
level 1 and Word's Heading 1 or ODF outline-level 1 to level 2. The writer
does the inverse. A heading that reads back one level deeper than it went in
is this rule broken.

**Never overwrite.** The binary numbers a taken name and it is tested;
nothing writes around it. The library writes nothing to disk at all.

**Warnings are not failures.** A run that dropped a furniture-layer paragraph
exits 0 and says so on stderr; `--strict` is the caller asking for refusal.
Exit codes are the fleet's: 0, 1 for bad input, 2 for a bad command line.
`~/notes/agent_cli_conventions.md` is the contract.

---

## Conventions

Every source file carries `Author: David M. Anderson` and `Built with AI
assistance (Claude, Anthropic)` in its header comment. Commits carry a
`Co-Authored-By` trailer for the Claude model in use and a `Signed-off-by`
trailer for David, and no session URL.

The release lane is cargo-dist, cloned from `excelano/xshape`; `RELEASING.md`
carries what is waddle's. CI is the fleet's `excelano/.github` Rust workflow
plus this repository's own `docling-latest` job.
