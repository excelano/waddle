//! What both targets share about lists: the levels a list style declares,
//! and the items as the writers need them.
//!
//! The model's list is a flat sequence of items with a level. Both readers
//! decide numbered or bullet per level from a style, take the start of
//! numbering from that style, and restart numbering at every list they see
//! begin. So a style is a vector of levels, one is generated per distinct
//! vector, and the writers open a new list where the model says one begins
//! or where the kind changes.
//!
//! Author: David M. Anderson
//! Built with AI assistance (Claude, Anthropic)

use docling_core::Node;

use crate::inline::{self, Run, collapse_spaces, is_blank};
use crate::report::{Reason, Warning};

/// One level of a list style: numbered or bullet, and where numbering
/// starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Level {
    pub ordered: bool,
    pub start: u64,
}

/// A generated list style: one per distinct vector of levels.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Style {
    pub levels: Vec<Level>,
}

/// One item as the writers need it.
pub struct Item {
    pub ordered: bool,
    pub number: u64,
    pub first_in_list: bool,
    pub level: usize,
    pub runs: Vec<Run>,
}

impl Item {
    /// The level this item opens, when it opens one.
    pub fn level_style(&self) -> Level {
        Level {
            ordered: self.ordered,
            start: if self.ordered { self.number.max(1) } else { 1 },
        }
    }
}

/// The items of a run of list-item nodes, off-layer and blank ones left out
/// with a warning for the former.
pub fn items(nodes: &[Node], warnings: &mut Vec<Warning>) -> Vec<Item> {
    nodes.iter().filter_map(|n| item(n, warnings)).collect()
}

fn item(node: &Node, warnings: &mut Vec<Warning>) -> Option<Item> {
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
        warnings.push(Warning {
            node: "list_item",
            reason: Reason::Layer(layer.value()),
        });
        return None;
    }
    // The DocLang-only form has the clean text and the runs where the flat
    // text carries a marker prefix or formatting Markdown cannot.
    let (ordered, mut runs) = match dclx {
        Some(d) if !d.runs.is_empty() => (d.ordered, inline::from_group(&d.text, &d.runs)),
        Some(d) => (d.ordered, inline::from_markdown(&d.text)),
        None => (*ordered, inline::from_markdown(text)),
    };
    // The item's own target applies when its text carries no link of its
    // own; the HTML reader sets both, and the text's markup wins.
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

/// The kind each depth was first seen with in a run of items, so the
/// outermost style, which is the one an application renders every level
/// from, matches the common case.
pub fn seen_levels(items: &[Item], depth: usize) -> Vec<Option<Level>> {
    let mut seen: Vec<Option<Level>> = vec![None; depth];
    for item in items {
        let slot = &mut seen[item.level.min(depth - 1)];
        if slot.is_none() {
            *slot = Some(item.level_style());
        }
    }
    seen
}

/// A style whose first levels are `chain` and whose deeper levels are
/// what was seen at that depth, else the last known kind starting at 1.
pub fn style(chain: &[Level], seen: &[Option<Level>]) -> Style {
    let mut levels = chain.to_vec();
    for slot in seen.iter().skip(levels.len()) {
        let last = *levels.last().expect("at least one level");
        levels.push(slot.unwrap_or(Level { start: 1, ..last }));
    }
    Style { levels }
}
