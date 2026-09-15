# CLAUDE.md

A DoclingDocument in, an office package out. docling.rs reads some forty formats into a
`DoclingDocument` and writes Markdown, JSON, DocLang and LaTeX from it; nothing in that
tree writes an office format, and this is the writer. `crates/waddle-core` is the library —
it takes a `docling_core::DoclingDocument` and returns the bytes of an ODT or DOCX package
with the warnings raised on the way, depends on `docling-core` and on nothing else of
docling's, and is `forbid(unsafe_code)`. `crates/waddle` is the CLI. docling.rs's own
office *readers* are the authority on which XML constructs map to which node, and so on
what a writer must emit; `DESIGN.md` is the authority on this crate, §5 being the mapping
and §6 what cannot round-trip.

## Commands

    cargo build --workspace
    cargo test --workspace     # the corpus tests skip without a docling.rs checkout
    cargo clippy --workspace --all-targets -- -D warnings   # must be silent
    cargo fmt --check
    cargo run -p waddle -- FILE --to odt
    cargo update -p docling-core && cargo test --workspace   # after a docling.rs release

`WADDLE_DUMP=dir` keeps every package a test writes, named by the test, for opening in
Writer by hand. `WADDLE_CORPUS_WRITER=1` opens every corpus package in headless Writer, a
second per document. The round-trip corpus is `crates/docling/tests/data/<format>/sources/`
in a checkout of docling.rs, which moves several times a week, so pull before reading it;
where that checkout lives is a fact about a machine and is not written down here. The build
directory is the shared one in `~/.cargo/config.toml`. Releases: run `ship waddle`.

## Rules

**Every match on `Node` is exhaustive.** No wildcard arm, anywhere, ever: a variant or
field docling.rs adds must fail the build here and be decided, not fall through and be
dropped. `node::kind` is the model for the rest. The library depends on `docling-core`
alone and not on `docling`, which is the binary's for reading input; a change that adds a C
dependency, or a crate that fails on `wasm32-unknown-unknown`, to `waddle-core` is David's
decision (`~/notes/pure_rust_preference.md`). **The writer emits what the reader
recognises** — read the docling.rs reader for that format and emit the construct it keys
on, the style ids, the outline levels, the numbering formats. The test is the round trip,
Markdown equality the floor and DocLang equality the target, and where the reader does not
look (§6) the diff is listed by name in the test rather than tolerated in general. **Level
1 is the title**: both office readers map a document title to heading level 1 and Word's
Heading 1 or ODF outline-level 1 to level 2, and the writer does the inverse, so a heading
that reads back one level deeper than it went in is this rule broken. **Never overwrite** —
the binary numbers a taken name and the library writes nothing to disk at all. **Warnings
are not failures**: a run that dropped a furniture-layer paragraph exits 0 and says so on
stderr, and `--strict` is the caller asking for refusal. Exit codes are the fleet's, 0, 1
for bad input, 2 for a bad command line (`~/notes/agent_cli_conventions.md`). Every source
file header carries `Author: David M. Anderson` and `Built with AI assistance (Claude,
Anthropic)`; commits carry a `Co-Authored-By` for the model and a `Signed-off-by` for
David, and no session URL.
