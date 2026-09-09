# waddle — DoclingDocument to ODT and DOCX

waddle writes an office document from a docling `DoclingDocument`. DocLang, a DocLang archive or docling JSON goes in; an OpenDocument Text or Word package comes out, offline, in pure Rust, with nothing overwritten. [docling.rs](https://github.com/docling-project/docling.rs) reads Word, PowerPoint, Excel, PDF, HTML and some forty other formats into that model and writes Markdown, JSON, DocLang and LaTeX from it. It writes no office format. waddle is the missing direction.

```text
$ waddle report.dclg --to odt
report.odt

$ waddle report.json --to docx -o out/
out/report.docx

$ waddle report.dclg --to odt --dry-run
would write report.odt
waddle: 3 nodes would be dropped or degraded:
  furniture: dropped, on the furniture layer (2)
  picture: written as a placeholder
```

## Why

The route from a DoclingDocument back to Word today is Markdown and then Pandoc, which drops the structure DocLang carries, headers and spans and captions among it, and adds a Haskell binary to a Rust pipeline. The model docling builds is semantic and flat: headings with levels, paragraphs with runs, lists with levels and numbering, tables with spans and header rows, pictures with captions, code, checkboxes. That is enough to write a plain, well-structured office document directly, the one a careful Markdown file would give you opened in Writer or Word with a stock style sheet.

That plainness is the design. The model carries no styles, page geometry, fonts or themes, so waddle does not try to reproduce how the source document looked; there is nothing to reproduce it from. What it does is put every node the model holds into the construct the format has for it, and say on stderr what it could not carry. `--strict` turns that into a refusal.

The construct chosen for each node is the one docling.rs's own reader for that format recognises, so a package waddle writes reads back into the document it was written from, as far as the reader allows; the design document in the repository lists where it does not.

## Install

Every install line ends with `waddle --install-skill`. That installs the [Claude Code skill](#use-it-from-claude-code) beside the binary. Drop it if you do not use Claude Code; the tool itself does not need it.

    curl --proto '=https' --tlsv1.2 -LsSf https://raw.githubusercontent.com/excelano/waddle/main/install.sh | sh && waddle --install-skill

Or `cargo install waddle && waddle --install-skill`, or `sudo apt install waddle && waddle --install-skill` from the Excelano apt repository, or `brew install excelano/tap/waddle && waddle --install-skill`. The uninstaller is `uninstall.sh` at the same URL shape.

## Use it from Rust

The library is `waddle-core`, and it depends on `docling-core` alone: no readers, no ML, no C. Give it a `DoclingDocument` and it returns the package bytes and the warnings it raised. [Duckling](https://github.com/excelano/duckling), the desktop converter over docling.rs, is the first caller.

## Use it from Claude Code

waddle was built for AI coding agents as much as for people, so the repository ships a [Claude Code](https://docs.claude.com/en/docs/claude-code) skill under `skills/waddle/`. It teaches an agent when waddle is the tool, how the input's pictures are found, what the warnings mean, and what each target keeps. The binary installs it:

    waddle --install-skill

That writes `~/.claude/skills/waddle/` and stamps in the version it came from, so a later run reports whether the skill has fallen behind the binary. It is safe to re-run: an unchanged skill reports `already current` and nothing is written. `waddle --uninstall-skill` removes it. Restart Claude Code afterwards, since skills are discovered at session start. The skill is compiled into the binary, so this works however waddle was installed.

## The family

waddle is one of three tools around DocLang and docling.rs. Duckling converts documents with docling.rs and shows the result. [Segler](https://github.com/excelano/segler) reviews and corrects DocLang. waddle writes the office document. Convert, correct, publish.

## Licence

MIT, matching docling.rs.
