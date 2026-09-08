//! Escaping for the XML the writers push as text.
//!
//! docling-core's own serialisers build their output as strings, and so do
//! these: the vocabulary of each package is fixed and small, and an escape
//! function is the whole of what a writer library would add.
//!
//! Author: David M. Anderson
//! Built with AI assistance (Claude, Anthropic)

/// Escape character data.
pub fn text(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(c),
        }
    }
    out
}

/// Escape an attribute value, for use inside double quotes.
pub fn attr(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\n' => out.push_str("&#10;"),
            '\t' => out.push_str("&#9;"),
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_what_xml_needs() {
        assert_eq!(text("a < b & c > d"), "a &lt; b &amp; c &gt; d");
        assert_eq!(attr("say \"hi\"\n"), "say &quot;hi&quot;&#10;");
    }
}
