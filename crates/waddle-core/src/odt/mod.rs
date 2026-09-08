//! OpenDocument Text.
//!
//! One `content.xml` written from the nodes, the stock `styles.xml` beside
//! it, a manifest, and `mimetype` first and stored. The constructs are the
//! ones docling.rs's ODF reader keys on, so that reading the package back
//! yields the nodes it was written from; `DESIGN.md` §5 is the mapping and
//! §6 the diffs the reader cannot close.
//!
//! Author: David M. Anderson
//! Built with AI assistance (Claude, Anthropic)

use docling_core::{DoclingDocument, Node};

use crate::inline::{self, Position, Run};
use crate::node::kind;
use crate::package::{self, Part};
use crate::report::{Output, Reason, Warning};
use crate::xml;
use crate::{Error, Target};

const STYLES: &str = include_str!("styles.xml");

const MANIFEST: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<manifest:manifest xmlns:manifest="urn:oasis:names:tc:opendocument:xmlns:manifest:1.0" manifest:version="1.3">
  <manifest:file-entry manifest:full-path="/" manifest:version="1.3" manifest:media-type="application/vnd.oasis.opendocument.text"/>
  <manifest:file-entry manifest:full-path="content.xml" manifest:media-type="text/xml"/>
  <manifest:file-entry manifest:full-path="styles.xml" manifest:media-type="text/xml"/>
</manifest:manifest>
"#;

const CONTENT_HEAD: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" xmlns:table="urn:oasis:names:tc:opendocument:xmlns:table:1.0" xmlns:draw="urn:oasis:names:tc:opendocument:xmlns:drawing:1.0" xmlns:fo="urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0" xmlns:xlink="http://www.w3.org/1999/xlink" xmlns:svg="urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0" office:version="1.3">
<office:font-face-decls>
<style:font-face style:name="Liberation Mono" svg:font-family="'Liberation Mono'" style:font-family-generic="modern" style:font-pitch="fixed"/>
</office:font-face-decls>
"#;

/// ODF's highest outline level.
const MAX_OUTLINE: u8 = 10;

/// Write the document as an ODT package.
pub fn write(doc: &DoclingDocument) -> Result<Output, Error> {
    let mut writer = Writer::default();
    for node in &doc.nodes {
        writer.node(node);
    }
    let content = writer.content();
    let bytes = package::zip(&[
        Part {
            name: "mimetype",
            bytes: Target::Odt.media_type().as_bytes(),
            stored: true,
        },
        Part {
            name: "content.xml",
            bytes: content.as_bytes(),
            stored: false,
        },
        Part {
            name: "styles.xml",
            bytes: STYLES.as_bytes(),
            stored: false,
        },
        Part {
            name: "META-INF/manifest.xml",
            bytes: MANIFEST.as_bytes(),
            stored: false,
        },
    ])?;
    Ok(Output {
        bytes,
        warnings: writer.warnings,
    })
}

/// The formatting of a `text:span`, without the text. One automatic style
/// is declared per distinct value.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Span {
    bold: bool,
    italic: bool,
    underline: bool,
    strike: bool,
    script: Position,
    code: bool,
}

impl Span {
    /// The span a run needs, or none for plain text.
    fn of(run: &Run) -> Option<Span> {
        let span = Span {
            bold: run.bold,
            italic: run.italic,
            underline: run.underline,
            strike: run.strike,
            script: run.script,
            code: run.code,
        };
        let plain = Span {
            bold: false,
            italic: false,
            underline: false,
            strike: false,
            script: Position::Baseline,
            code: false,
        };
        (span != plain).then_some(span)
    }

    fn properties(&self) -> String {
        let mut p = String::new();
        if self.bold {
            p.push_str(r#" fo:font-weight="bold" style:font-weight-asian="bold" style:font-weight-complex="bold""#);
        }
        if self.italic {
            p.push_str(r#" fo:font-style="italic" style:font-style-asian="italic" style:font-style-complex="italic""#);
        }
        if self.underline {
            p.push_str(r#" style:text-underline-style="solid" style:text-underline-width="auto" style:text-underline-color="font-color""#);
        }
        if self.strike {
            p.push_str(
                r#" style:text-line-through-style="solid" style:text-line-through-type="single""#,
            );
        }
        match self.script {
            Position::Baseline => {}
            Position::Sub => p.push_str(r#" style:text-position="sub 58%""#),
            Position::Super => p.push_str(r#" style:text-position="super 58%""#),
        }
        if self.code {
            p.push_str(r#" style:font-name="Liberation Mono""#);
        }
        p
    }
}

#[derive(Default)]
struct Writer {
    body: String,
    spans: Vec<Span>,
    warnings: Vec<Warning>,
}

impl Writer {
    /// The whole `content.xml`.
    fn content(&self) -> String {
        let mut out = String::from(CONTENT_HEAD);
        out.push_str("<office:automatic-styles>\n");
        for (i, span) in self.spans.iter().enumerate() {
            out.push_str(&format!(
                "<style:style style:name=\"T{}\" style:family=\"text\"><style:text-properties{}/></style:style>\n",
                i + 1,
                span.properties()
            ));
        }
        out.push_str("</office:automatic-styles>\n<office:body>\n<office:text>\n");
        out.push_str(&self.body);
        out.push_str("</office:text>\n</office:body>\n</office:document-content>\n");
        out
    }

    fn warn(&mut self, node: &Node, reason: Reason) {
        self.warnings.push(Warning {
            node: kind(node),
            reason,
        });
    }

    /// One node. Exhaustive on purpose; see the crate documentation.
    fn node(&mut self, node: &Node) {
        match node {
            Node::Heading { level, text } => self.heading(*level, &inline::from_markdown(text)),
            Node::Paragraph { text } => {
                self.paragraph("Text_20_body", &inline::from_markdown(text))
            }
            Node::InlineGroup { runs, md_text, .. } => {
                self.paragraph("Text_20_body", &inline::from_group(md_text, runs))
            }
            Node::TextDump(text) => {
                for block in text.split("\n\n") {
                    let run = Run {
                        text: block.to_string(),
                        ..Run::default()
                    };
                    self.paragraph("Text_20_body", &[run]);
                }
            }
            Node::Group {
                layer: Some(layer), ..
            } => self.warn(node, Reason::Layer(layer.value())),
            Node::Group {
                layer: None,
                children,
                ..
            } => {
                for child in children {
                    self.node(child);
                }
            }
            Node::Located { inner, .. }
            | Node::Commented { inner, .. }
            | Node::DoclangOnly(inner) => self.node(inner),
            Node::Furniture { layer, .. } => self.warn(node, Reason::Layer(layer.value())),
            // Page geometry has no content to carry and is not worth a warning.
            Node::PageInfo { .. } => {}
            Node::CheckboxItem { .. }
            | Node::ListItem { .. }
            | Node::Code { .. }
            | Node::Table(_)
            | Node::Picture { .. }
            | Node::Formula { .. }
            | Node::Chart { .. }
            | Node::FieldRegion { .. }
            | Node::CommentSection { .. }
            | Node::PageFurniture { .. }
            | Node::PageBreak => self.warn(node, Reason::Unsupported),
        }
    }

    /// Level 1 is the document title; every other level is the target's
    /// heading one below it. Both readers agree; `DESIGN.md` §5.
    fn heading(&mut self, level: u8, runs: &[Run]) {
        if runs.iter().all(|r| r.text.trim().is_empty()) {
            return;
        }
        if level <= 1 {
            self.paragraph("Title", runs);
            return;
        }
        let outline = (level - 1).min(MAX_OUTLINE);
        self.body.push_str(&format!(
            "<text:h text:style-name=\"Heading_20_{outline}\" text:outline-level=\"{outline}\">"
        ));
        self.runs(runs);
        self.body.push_str("</text:h>\n");
    }

    fn paragraph(&mut self, style: &str, runs: &[Run]) {
        if runs.iter().all(|r| r.text.trim().is_empty()) {
            return;
        }
        self.body
            .push_str(&format!("<text:p text:style-name=\"{style}\">"));
        self.runs(runs);
        self.body.push_str("</text:p>\n");
    }

    /// The runs of one paragraph. Consecutive runs with the same link share
    /// one `text:a`.
    fn runs(&mut self, runs: &[Run]) {
        let mut at_start = true;
        let mut open_href: Option<&str> = None;
        for run in runs {
            if run.href.as_deref() != open_href {
                if open_href.is_some() {
                    self.body.push_str("</text:a>");
                }
                open_href = run.href.as_deref();
                if let Some(href) = open_href {
                    self.body.push_str(&format!(
                        "<text:a xlink:type=\"simple\" xlink:href=\"{}\" text:style-name=\"Internet_20_link\">",
                        xml::attr(href)
                    ));
                }
            }
            match Span::of(run) {
                Some(span) => {
                    let name = self.span_name(span);
                    self.body
                        .push_str(&format!("<text:span text:style-name=\"{name}\">"));
                    self.text(&run.text, &mut at_start);
                    self.body.push_str("</text:span>");
                }
                None => self.text(&run.text, &mut at_start),
            }
        }
        if open_href.is_some() {
            self.body.push_str("</text:a>");
        }
    }

    fn span_name(&mut self, span: Span) -> String {
        let index = match self.spans.iter().position(|s| *s == span) {
            Some(i) => i,
            None => {
                self.spans.push(span);
                self.spans.len() - 1
            }
        };
        format!("T{}", index + 1)
    }

    /// Character data with ODF's whitespace rules: a newline is a line
    /// break, a tab is a tab, and a run of spaces, or any space at the start
    /// of a line, is counted in `text:s` because the format collapses it
    /// otherwise.
    fn text(&mut self, s: &str, at_start: &mut bool) {
        let mut spaces = 0usize;
        for c in s.chars() {
            if c == ' ' {
                spaces += 1;
                continue;
            }
            self.spaces(spaces, *at_start);
            spaces = 0;
            match c {
                '\n' => {
                    self.body.push_str("<text:line-break/>");
                    *at_start = true;
                }
                '\t' => {
                    self.body.push_str("<text:tab/>");
                    *at_start = false;
                }
                _ => {
                    self.body.push_str(&xml::text(&c.to_string()));
                    *at_start = false;
                }
            }
        }
        self.spaces(spaces, *at_start);
        if spaces > 0 {
            *at_start = false;
        }
    }

    fn spaces(&mut self, count: usize, at_start: bool) {
        if count == 0 {
            return;
        }
        if at_start {
            self.body.push_str(&format!("<text:s text:c=\"{count}\"/>"));
        } else {
            self.body.push(' ');
            if count > 1 {
                self.body
                    .push_str(&format!("<text:s text:c=\"{}\"/>", count - 1));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn content_of(doc: &DoclingDocument) -> String {
        let mut writer = Writer::default();
        for node in &doc.nodes {
            writer.node(node);
        }
        writer.content()
    }

    #[test]
    fn title_and_headings_follow_the_reader_convention() {
        let mut doc = DoclingDocument::new("t");
        doc.add_heading(1, "Report");
        doc.add_heading(2, "Intro");
        let xml = content_of(&doc);
        assert!(xml.contains(r#"<text:p text:style-name="Title">Report</text:p>"#));
        assert!(xml.contains(
            r#"<text:h text:style-name="Heading_20_1" text:outline-level="1">Intro</text:h>"#
        ));
    }

    #[test]
    fn spans_are_declared_once_per_formatting() {
        let mut doc = DoclingDocument::new("t");
        doc.add_paragraph("**a** and **b** and *c*");
        let xml = content_of(&doc);
        assert_eq!(xml.matches("<style:style style:name=\"T").count(), 2);
        assert!(xml.contains(r#"<text:span text:style-name="T1">a</text:span>"#));
        assert!(xml.contains(r#"<text:span text:style-name="T1">b</text:span>"#));
        assert!(xml.contains(r#"<text:span text:style-name="T2">c</text:span>"#));
    }

    #[test]
    fn whitespace_is_counted_and_breaks_are_elements() {
        let mut doc = DoclingDocument::new("t");
        doc.add_paragraph("a  b\n c\td");
        let xml = content_of(&doc);
        assert!(xml.contains(
            r#"a <text:s text:c="1"/>b<text:line-break/><text:s text:c="1"/>c<text:tab/>d"#
        ));
    }

    #[test]
    fn unsupported_nodes_warn_and_layers_drop() {
        let mut doc = DoclingDocument::new("t");
        doc.push(Node::PageBreak);
        doc.push(Node::Furniture {
            layer: docling_core::ContentLayer::Furniture,
            inner: Box::new(Node::Paragraph {
                text: "running head".into(),
            }),
        });
        let mut writer = Writer::default();
        for node in &doc.nodes {
            writer.node(node);
        }
        assert_eq!(
            writer.warnings,
            vec![
                Warning {
                    node: "page_break",
                    reason: Reason::Unsupported
                },
                Warning {
                    node: "furniture",
                    reason: Reason::Layer("furniture")
                },
            ]
        );
        assert!(!writer.body.contains("running head"));
    }
}
