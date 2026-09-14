//! The node walk both spreadsheet targets share: which nodes become a sheet,
//! what its cells are, and what it is called.
//!
//! A spreadsheet holds cells, so the tabular nodes become sheets and every
//! other node is reported as dropped. `DESIGN.md` §7 settles that, and §5 has
//! the mapping. ODS and XLSX differ in their packages and their XML, not in
//! this; keeping the walk here is what stops the two drifting apart, and the
//! match on `Node` below is the one that fails the build when docling.rs adds
//! a variant.
//!
//! Author: David M. Anderson
//! Built with AI assistance (Claude, Anthropic)

use docling_core::{Node, Table};

use crate::report::{Reason, Warning};

/// One sheet: its name, its cells as a rectangular grid of strings, and how
/// many of its leading rows the source marked as a header band.
pub struct Sheet {
    pub name: String,
    pub rows: Vec<Vec<String>>,
    pub header_rows: usize,
}

/// The sheets a document makes, and the nodes that had no cell to go in.
/// `name_limit` is the longest sheet name the target allows.
pub fn collect(nodes: &[Node], name_limit: usize) -> (Vec<Sheet>, Vec<Warning>) {
    let mut walk = Walk {
        sheets: Vec::new(),
        warnings: Vec::new(),
        name_limit,
        group_name: None,
    };
    walk.nodes(nodes);
    (walk.sheets, walk.warnings)
}

/// The leading rows a table's structure marks as a header band. Both targets
/// want it; only ODS does anything with it.
pub fn header_rows(table: &Table) -> usize {
    let cells = table.derive_cells();
    let is_header = |r: usize| {
        cells
            .iter()
            .filter(|c| c.start_row == r)
            .all(|c| c.column_header)
    };
    (0..table.rows.len()).take_while(|&r| is_header(r)).count()
}

struct Walk {
    sheets: Vec<Sheet>,
    warnings: Vec<Warning>,
    name_limit: usize,
    /// The name of the sheet group being walked, if any. The XLSX reader
    /// returns a sheet's name on a `Group` labelled `sheet` rather than as the
    /// table's caption, so without this a second trip would have no name left
    /// to give the sheet and would not land where the first one did.
    group_name: Option<String>,
}

impl Walk {
    fn nodes(&mut self, nodes: &[Node]) {
        for node in nodes {
            self.node(node);
        }
    }

    fn node(&mut self, node: &Node) {
        match node {
            Node::Table(table) => {
                self.sheet(&table.rows, table.caption.as_deref(), header_rows(table))
            }
            // A chart with its data is that data. One without has no cells to
            // put in a sheet, so it goes the way of the prose.
            Node::Chart { table, caption, .. } if !table.rows.is_empty() => {
                self.sheet(&table.rows, caption.as_deref(), header_rows(table))
            }
            // A field region is a table of keys and values in every other
            // target, and the corpus counts it as one, so it is a sheet here.
            Node::FieldRegion { items } => {
                let rows: Vec<Vec<String>> = items
                    .iter()
                    .map(|item| {
                        vec![
                            item.key.clone().unwrap_or_default(),
                            item.value.clone().unwrap_or_default(),
                        ]
                    })
                    .collect();
                self.sheet(&rows, None, 0);
            }
            Node::Group {
                layer: None,
                children,
                label,
                name,
            } => {
                // A sheet group names the sheet it came from; anything else
                // is transparent and leaves the name as it was.
                let outer = if label == "sheet" && name.is_some() {
                    std::mem::replace(&mut self.group_name, name.clone())
                } else {
                    self.group_name.clone()
                };
                self.nodes(children);
                self.group_name = outer;
            }
            Node::Located { inner, .. }
            | Node::Prov { inner, .. }
            | Node::Commented { inner, .. }
            | Node::DoclangOnly(inner) => self.node(inner),
            Node::Furniture { layer, .. } => self.warn(node, Reason::Layer(layer.value())),
            Node::Group { layer, .. } => self.warn(
                node,
                Reason::Layer(layer.as_ref().map_or("furniture", |l| l.value())),
            ),
            // Everything a spreadsheet has no cell for. Named rather than
            // swept up, so a variant docling.rs adds fails the build here.
            Node::Heading { .. }
            | Node::Paragraph { .. }
            | Node::CheckboxItem { .. }
            | Node::ListItem { .. }
            | Node::Code { .. }
            | Node::Picture { .. }
            | Node::Formula { .. }
            | Node::Caption { .. }
            | Node::Chart { .. }
            | Node::InlineGroup { .. }
            | Node::CommentSection { .. }
            | Node::PageFurniture { .. }
            | Node::PageBreak
            | Node::PageInfo { .. }
            | Node::TextDump(_) => self.warn(node, Reason::Unsupported),
        }
    }

    fn warn(&mut self, node: &Node, reason: Reason) {
        self.warnings.push(Warning {
            node: crate::node::kind(node),
            reason,
        });
    }

    /// One sheet, padded to a rectangle. A ragged row would leave a hole, and
    /// both readers rebuild a table by flood-filling the non-empty cells of a
    /// sheet, so a hole is a place the region parts on.
    fn sheet(&mut self, rows: &[Vec<String>], caption: Option<&str>, header_rows: usize) {
        let columns = rows.iter().map(Vec::len).max().unwrap_or(0);
        if columns == 0 {
            return;
        }
        let name = self.name_for(caption);
        let rows = rows
            .iter()
            .map(|row| {
                (0..columns)
                    .map(|c| row.get(c).cloned().unwrap_or_default())
                    .collect()
            })
            .collect();
        self.sheets.push(Sheet {
            name,
            rows,
            header_rows,
        });
    }

    /// A sheet name: the caption where there is one, else `Sheet<n>`. Both
    /// formats require the name to be unique in the document and forbid the
    /// same handful of characters in it, and both cap its length; a forbidden
    /// character becomes a space and a collision takes a numeric suffix.
    fn name_for(&self, caption: Option<&str>) -> String {
        let caption = caption.or(self.group_name.as_deref());
        let base = caption
            .map(|c| {
                c.chars()
                    .map(|ch| if "[]*?:/\\'".contains(ch) { ' ' } else { ch })
                    .collect::<String>()
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .filter(|c| !c.is_empty())
            .map(|c| c.chars().take(self.name_limit).collect::<String>())
            .unwrap_or_else(|| format!("Sheet{}", self.sheets.len() + 1));
        let taken = |n: &String| self.sheets.iter().any(|s| &s.name == n);
        let mut name = base.clone();
        let mut n = 2;
        while taken(&name) {
            name = format!("{base} {n}");
            n += 1;
        }
        name
    }
}
