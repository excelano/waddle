# waddle — DoclingDocument to ODT and DOCX

waddle writes an office document from a docling `DoclingDocument`. DocLang, a DocLang archive or docling JSON goes in; an OpenDocument Text or Word package comes out, offline, in pure Rust, with nothing overwritten. [docling.rs](https://github.com/docling-project/docling.rs) reads Word, PowerPoint, Excel, PDF, HTML and some forty other formats into that model and writes Markdown, JSON, DocLang and LaTeX from it. It writes no office format. waddle is the missing direction.

```text
$ waddle report.dclg --to odt
report.odt

$ waddle report.json --to docx -o out/
out/report.docx

$ waddle report.dclg --to odt --dry-run
would write report.odt
  picture: written as a placeholder
  paragraph: dropped, on the furniture layer
```

## Why

The route from a DoclingDocument back to Word today is Markdown and then Pandoc, which drops the structure DocLang carries, headers and spans and captions among it, and adds a Haskell binary to a Rust pipeline. The model docling builds is semantic and flat: headings with levels, paragraphs with runs, lists with levels and numbering, tables with spans and header rows, pictures with captions, code, checkboxes. That is enough to write a plain, well-structured office document directly, the one a careful Markdown file would give you opened in Writer or Word with a stock style sheet.

That plainness is the design. The model carries no styles, page geometry, fonts or themes, so waddle does not try to reproduce how the source document looked; there is nothing to reproduce it from. What it does is put every node the model holds into the construct the format has for it, and say on stderr what it could not carry. `--strict` turns that into a refusal.

## Install

    curl --proto '=https' --tlsv1.2 -LsSf https://raw.githubusercontent.com/excelano/waddle/main/install.sh | sh

Or `cargo install waddle`, or `apt install waddle` from the Excelano apt repository, or `brew install excelano/tap/waddle`. The uninstaller is `uninstall.sh` at the same URL shape.

## Use it from Rust

The library is `waddle-core`, and it depends on `docling-core` alone: no readers, no ML, no C. Give it a `DoclingDocument` and it returns the package bytes and the warnings it raised. [Duckling](https://github.com/excelano/duckling), the desktop converter over docling.rs, is the first caller.

## The family

waddle is one of three tools around DocLang and docling.rs. Duckling converts documents with docling.rs and shows the result. [Segler](https://github.com/excelano/segler) reviews and corrects DocLang. waddle writes the office document. Convert, correct, publish.

## Licence

MIT, matching docling.rs.
