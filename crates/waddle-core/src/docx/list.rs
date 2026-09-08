//! Lists in DOCX: numbering definitions, and the numbering reference each
//! item's paragraph carries.
//!
//! The reader decides numbered or bullet from the level's `numFmt` in
//! `numbering.xml`, which is the only signal it uses; nesting is the
//! paragraph's `ilvl` above the first item's; numbering counts per `numId`,
//! so a new `w:num` is a restart. A definition is generated per distinct
//! vector of levels and a `w:num` per list the model says begins.
//!
//! Author: David M. Anderson
//! Built with AI assistance (Claude, Anthropic)

use docling_core::Node;

use super::Writer;
use crate::list::{Level, Style, items, seen_levels, style};

/// OOXML's deepest list level.
const LEVELS: usize = 9;

/// The `numbering.xml` part.
pub(super) fn numbering_xml(styles: &[Style], nums: &[usize]) -> String {
    let mut out = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<w:numbering xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\">\n",
    );
    for (i, style) in styles.iter().enumerate() {
        out.push_str(&format!(
            "<w:abstractNum w:abstractNumId=\"{i}\"><w:multiLevelType w:val=\"hybridMultilevel\"/>\n"
        ));
        for (ilvl, level) in style.levels.iter().enumerate() {
            let left = 720 * (ilvl + 1);
            let (format, text) = if level.ordered {
                ("decimal".to_string(), format!("%{}.", ilvl + 1))
            } else {
                ("bullet".to_string(), ["•", "◦", "▪"][ilvl % 3].to_string())
            };
            out.push_str(&format!(
                "<w:lvl w:ilvl=\"{ilvl}\"><w:start w:val=\"{}\"/><w:numFmt w:val=\"{format}\"/><w:lvlText w:val=\"{text}\"/><w:lvlJc w:val=\"left\"/><w:pPr><w:ind w:left=\"{left}\" w:hanging=\"360\"/></w:pPr></w:lvl>\n",
                level.start
            ));
        }
        out.push_str("</w:abstractNum>\n");
    }
    for (i, abstract_index) in nums.iter().enumerate() {
        out.push_str(&format!(
            "<w:num w:numId=\"{}\"><w:abstractNumId w:val=\"{abstract_index}\"/></w:num>\n",
            i + 1
        ));
    }
    out.push_str("</w:numbering>\n");
    out
}

impl Writer {
    /// A run of consecutive list items.
    pub(super) fn list(&mut self, nodes: &[Node]) {
        let items = items(nodes, &mut self.warnings);
        let seen = seen_levels(&items, LEVELS);
        // The current `w:num`: its id and its first level.
        let mut current: Option<(usize, Level)> = None;
        let mut has_items = false;
        // The last number at level 0, because the DOCX reader marks no item
        // as first in its list and a break in the count is the other sign.
        let mut last_number: Option<u64> = None;
        for item in &items {
            let ilvl = item.level.min(LEVELS - 1);
            let restart = match current {
                None => true,
                Some((_, first)) => {
                    ilvl == 0
                        && ((item.first_in_list && has_items)
                            || first.ordered != item.ordered
                            || (item.ordered && last_number.is_some_and(|n| item.number != n + 1)))
                }
            };
            if restart {
                let first = if ilvl == 0 {
                    item.level_style()
                } else {
                    seen[0].unwrap_or(item.level_style())
                };
                let style = style(&[first], &seen);
                let abstract_index = match self.numbering.iter().position(|s| *s == style) {
                    Some(i) => i,
                    None => {
                        self.numbering.push(style);
                        self.numbering.len() - 1
                    }
                };
                self.nums.push(abstract_index);
                current = Some((self.nums.len(), first));
            }
            let (num_id, _) = current.expect("a num is open");
            self.body.push_str(&format!(
                "<w:p><w:pPr><w:pStyle w:val=\"ListParagraph\"/><w:numPr><w:ilvl w:val=\"{ilvl}\"/><w:numId w:val=\"{num_id}\"/></w:numPr></w:pPr>"
            ));
            self.runs(&item.runs);
            self.body.push_str("</w:p>\n");
            has_items = true;
            if ilvl == 0 {
                last_number = item.ordered.then_some(item.number);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use docling_core::DoclingDocument;

    fn item(ordered: bool, number: u64, first: bool, level: u8, text: &str) -> Node {
        Node::ListItem {
            ordered,
            number,
            first_in_list: first,
            text: text.into(),
            level,
            marker: None,
            location: None,
            dclx: None,
            href: None,
            layer: None,
        }
    }

    #[test]
    fn levels_are_ilvl_and_a_new_list_is_a_new_num() {
        let mut writer = Writer::default();
        let doc = DoclingDocument {
            nodes: vec![
                item(true, 3, true, 0, "three"),
                item(false, 0, false, 1, "under"),
                item(true, 4, false, 0, "four"),
                item(true, 1, true, 0, "restart"),
            ],
            ..DoclingDocument::new("t")
        };
        writer.nodes(&doc.nodes);
        assert_eq!(writer.body.matches("<w:numId w:val=\"1\"/>").count(), 3);
        assert_eq!(writer.body.matches("<w:numId w:val=\"2\"/>").count(), 1);
        assert!(writer.body.contains("<w:ilvl w:val=\"1\"/>"));
        assert_eq!(
            writer.numbering.len(),
            2,
            "start 3 and start 1 are different definitions"
        );
        let xml = numbering_xml(&writer.numbering, &writer.nums);
        assert!(xml.contains("<w:start w:val=\"3\"/><w:numFmt w:val=\"decimal\"/>"));
        assert!(xml.contains("<w:numFmt w:val=\"bullet\"/>"));
        assert!(xml.contains("<w:num w:numId=\"2\"><w:abstractNumId w:val=\"1\"/></w:num>"));
    }
}
