//! Lists: the flat, levelled item sequence rebuilt as nested `text:list`
//! elements, and the list styles that say which levels are numbered.
//!
//! The reader decides numbered or bullet per level from the list style
//! named on the `text:list` element, looks the nested list's own
//! `style-name` up for the nested level, takes the start value from the
//! level's style, and restarts numbering at every `text:list` that does not
//! continue the previous one. So every list element here names a style, a
//! style is generated per distinct vector of levels, and a new element opens
//! wherever the model says a list begins or the kind changes.
//!
//! Author: David M. Anderson
//! Built with AI assistance (Claude, Anthropic)

use docling_core::Node;

use super::{Writer, collapse_spaces, is_blank};
use crate::inline::{self, Run};
use crate::report::{Reason, Warning};

/// ODF's deepest list level.
const LEVELS: usize = 10;

/// One level of a list style: numbered or bullet, and where numbering
/// starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Level {
    pub ordered: bool,
    pub start: u64,
}

/// A generated `text:list-style`, one per distinct vector of levels.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Style {
    levels: Vec<Level>,
}

pub(super) fn style_name(index: usize) -> String {
    format!("L{}", index + 1)
}

impl Style {
    pub(super) fn xml(&self, name: &str) -> String {
        let mut out = format!("<text:list-style style:name=\"{name}\">\n");
        for (i, level) in self.levels.iter().enumerate() {
            let depth = i + 1;
            let indent = 0.5 * depth as f64;
            let properties = format!(
                "<style:list-level-properties text:list-level-position-and-space-mode=\"label-alignment\"><style:list-level-label-alignment text:label-followed-by=\"listtab\" text:list-tab-stop-position=\"{indent:.2}in\" fo:text-indent=\"-0.25in\" fo:margin-left=\"{indent:.2}in\"/></style:list-level-properties>"
            );
            if level.ordered {
                let start = if level.start > 1 {
                    format!(" text:start-value=\"{}\"", level.start)
                } else {
                    String::new()
                };
                // Writer shows the enclosing numbers too (`1.1.`); the reader
                // reads only the prefix and suffix, so the marker returns as `1.`.
                out.push_str(&format!(
                    "<text:list-level-style-number text:level=\"{depth}\" style:num-suffix=\".\" style:num-format=\"1\" text:display-levels=\"{depth}\"{start}>{properties}</text:list-level-style-number>\n"
                ));
            } else {
                let bullet = ["•", "◦", "▪"][i % 3];
                out.push_str(&format!(
                    "<text:list-level-style-bullet text:level=\"{depth}\" text:bullet-char=\"{bullet}\">{properties}</text:list-level-style-bullet>\n"
                ));
            }
        }
        out.push_str("</text:list-style>\n");
        out
    }
}

/// One item as the writer needs it.
struct Item {
    ordered: bool,
    number: u64,
    first_in_list: bool,
    level: usize,
    runs: Vec<Run>,
}

/// An open `text:list` element.
struct Open {
    level: Level,
    item_open: bool,
    has_items: bool,
}

impl Writer {
    /// A run of consecutive list items.
    pub(super) fn list(&mut self, nodes: &[Node]) {
        let items: Vec<Item> = nodes.iter().filter_map(|n| self.item(n)).collect();
        // The kind each depth was first seen with, so the outermost style,
        // which is the one Writer renders every level from, matches the
        // common case.
        let mut seen: Vec<Option<Level>> = vec![None; LEVELS];
        for item in &items {
            let slot = &mut seen[item.level.min(LEVELS - 1)];
            if slot.is_none() {
                *slot = Some(Level {
                    ordered: item.ordered,
                    start: if item.ordered { item.number.max(1) } else { 1 },
                });
            }
        }
        let mut stack: Vec<Open> = Vec::new();
        for item in &items {
            // A level deeper than one below the current one has no parent
            // item to nest under, and the reader would flatten it anyway.
            let depth = item.level.min(stack.len()).min(LEVELS - 1);
            while stack.len() > depth + 1 {
                self.close_list(&mut stack);
            }
            if stack.len() == depth + 1 {
                let top = stack.last_mut().expect("a list is open");
                let restart =
                    (item.first_in_list && top.has_items) || top.level.ordered != item.ordered;
                if restart {
                    self.close_list(&mut stack);
                } else if top.item_open {
                    self.body.push_str("</text:list-item>\n");
                    top.item_open = false;
                }
            }
            if stack.len() == depth {
                let level = Level {
                    ordered: item.ordered,
                    start: if item.ordered { item.number.max(1) } else { 1 },
                };
                let mut levels: Vec<Level> = stack.iter().map(|o| o.level).collect();
                levels.push(level);
                for slot in seen.iter().skip(levels.len()) {
                    let last = *levels.last().expect("at least one level");
                    levels.push(slot.unwrap_or(Level { start: 1, ..last }));
                }
                let name = self.list_style(Style { levels });
                self.body
                    .push_str(&format!("<text:list text:style-name=\"{name}\">\n"));
                stack.push(Open {
                    level,
                    item_open: false,
                    has_items: false,
                });
            }
            self.body
                .push_str("<text:list-item><text:p text:style-name=\"List_20_Paragraph\">");
            self.runs(&item.runs);
            self.body.push_str("</text:p>\n");
            let top = stack.last_mut().expect("a list is open");
            top.item_open = true;
            top.has_items = true;
        }
        while !stack.is_empty() {
            self.close_list(&mut stack);
        }
    }

    fn close_list(&mut self, stack: &mut Vec<Open>) {
        if let Some(open) = stack.pop() {
            if open.item_open {
                self.body.push_str("</text:list-item>\n");
            }
            self.body.push_str("</text:list>\n");
        }
    }

    fn list_style(&mut self, style: Style) -> String {
        let index = match self.lists.iter().position(|s| *s == style) {
            Some(i) => i,
            None => {
                self.lists.push(style);
                self.lists.len() - 1
            }
        };
        style_name(index)
    }

    /// The item's content, or none when it is off the body layer or blank.
    fn item(&mut self, node: &Node) -> Option<Item> {
        let Node::ListItem {
            ordered,
            number,
            first_in_list,
            text,
            level,
            dclx,
            href,
            layer,
            marker: _,
            location: _,
        } = node
        else {
            return None;
        };
        if let Some(layer) = layer {
            self.warnings.push(Warning {
                node: "list_item",
                reason: Reason::Layer(layer.value()),
            });
            return None;
        }
        // The DocLang-only form has the clean text and the runs where the
        // flat text carries a marker prefix or formatting Markdown cannot.
        let (ordered, mut runs) = match dclx {
            Some(d) if !d.runs.is_empty() => (d.ordered, inline::from_group(&d.text, &d.runs)),
            Some(d) => (d.ordered, inline::from_markdown(&d.text)),
            None => (*ordered, inline::from_markdown(text)),
        };
        // The item's own target applies when its text carries no link of
        // its own; the HTML reader sets both, and the text's markup wins.
        if let Some(href) = href
            && runs.iter().all(|r| r.href.is_none())
        {
            for run in &mut runs {
                run.href = Some(href.clone());
            }
        }
        let runs = collapse_spaces(runs);
        if is_blank(&runs) {
            return None;
        }
        Some(Item {
            ordered,
            number: *number,
            first_in_list: *first_in_list,
            level: *level as usize,
            runs,
        })
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

    fn body_of(nodes: Vec<Node>) -> (String, Vec<Style>) {
        let mut writer = Writer::default();
        let doc = DoclingDocument {
            nodes,
            ..DoclingDocument::new("t")
        };
        writer.nodes(&doc.nodes);
        (writer.body, writer.lists)
    }

    #[test]
    fn nesting_is_rebuilt_from_levels() {
        let (body, styles) = body_of(vec![
            item(false, 0, true, 0, "a"),
            item(false, 0, false, 1, "a.1"),
            item(false, 0, false, 1, "a.2"),
            item(false, 0, false, 0, "b"),
        ]);
        let expected = "<text:list text:style-name=\"L1\">\n\
<text:list-item><text:p text:style-name=\"List_20_Paragraph\">a</text:p>\n\
<text:list text:style-name=\"L1\">\n\
<text:list-item><text:p text:style-name=\"List_20_Paragraph\">a.1</text:p>\n\
</text:list-item>\n\
<text:list-item><text:p text:style-name=\"List_20_Paragraph\">a.2</text:p>\n\
</text:list-item>\n\
</text:list>\n\
</text:list-item>\n\
<text:list-item><text:p text:style-name=\"List_20_Paragraph\">b</text:p>\n\
</text:list-item>\n\
</text:list>\n";
        assert_eq!(body, expected);
        assert_eq!(styles.len(), 1);
        assert!(styles[0].levels.iter().all(|l| !l.ordered));
    }

    #[test]
    fn a_new_list_or_a_kind_change_opens_a_new_element() {
        let (body, styles) = body_of(vec![
            item(true, 3, true, 0, "three"),
            item(true, 4, false, 0, "four"),
            item(false, 0, false, 0, "bullet"),
            item(true, 1, true, 0, "one again"),
        ]);
        assert_eq!(body.matches("<text:list ").count(), 3);
        assert_eq!(styles.len(), 3);
        assert_eq!(
            styles[0].levels[0],
            Level {
                ordered: true,
                start: 3
            }
        );
        assert!(styles[0].xml("L1").contains("text:start-value=\"3\""));
        assert!(!styles[1].levels[0].ordered);
        assert_eq!(
            styles[2].levels[0],
            Level {
                ordered: true,
                start: 1
            }
        );
    }

    #[test]
    fn a_level_jump_nests_one_deeper_only() {
        let (body, _) = body_of(vec![
            item(false, 0, true, 0, "a"),
            item(false, 0, false, 3, "deep"),
        ]);
        assert_eq!(body.matches("<text:list ").count(), 2);
    }

    #[test]
    fn a_mixed_nesting_gets_its_own_style_for_the_nested_element() {
        let (body, styles) = body_of(vec![
            item(true, 1, true, 0, "one"),
            item(false, 0, false, 1, "bullet under one"),
        ]);
        // Both elements name the same style: its level 2 is a bullet, taken
        // from the first item seen at that depth.
        assert_eq!(body.matches("text:style-name=\"L1\"").count(), 2);
        assert_eq!(styles.len(), 1);
        assert!(styles[0].levels[0].ordered && !styles[0].levels[1].ordered);
    }
}
