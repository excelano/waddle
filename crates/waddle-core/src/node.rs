//! Names for the model's node variants, for reports and diagnostics.
//!
//! Author: David M. Anderson
//! Built with AI assistance (Claude, Anthropic)

use docling_core::Node;

/// The DocLang-style name of a node's variant: `heading`, `list_item`,
/// `page_break`. Wrappers name themselves, not what they wrap.
///
/// This match has no wildcard arm, and that is the point: a variant docling.rs
/// adds fails the build here first.
pub fn kind(node: &Node) -> &'static str {
    match node {
        Node::Heading { .. } => "heading",
        Node::Paragraph { .. } => "paragraph",
        Node::CheckboxItem { .. } => "checkbox_item",
        Node::ListItem { .. } => "list_item",
        Node::Code { .. } => "code",
        Node::Table(_) => "table",
        Node::Picture { .. } => "picture",
        Node::Formula { .. } => "formula",
        Node::Caption { .. } => "caption",
        Node::Chart { .. } => "chart",
        Node::Group { .. } => "group",
        Node::FieldRegion { .. } => "field_region",
        Node::InlineGroup { .. } => "inline_group",
        Node::Furniture { .. } => "furniture",
        Node::CommentSection { .. } => "comment_section",
        Node::Commented { .. } => "commented",
        Node::Located { .. } => "located",
        Node::Prov { .. } => "prov",
        Node::PageFurniture { .. } => "page_furniture",
        Node::PageBreak => "page_break",
        Node::PageInfo { .. } => "page_info",
        Node::DoclangOnly(_) => "doclang_only",
        Node::TextDump(_) => "text_dump",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use docling_core::ContentLayer;

    #[test]
    fn names_follow_doclang() {
        assert_eq!(kind(&Node::PageBreak), "page_break");
        assert_eq!(
            kind(&Node::Heading {
                level: 1,
                text: "Title".into()
            }),
            "heading"
        );
    }

    /// `Group` carries `name` and `layer` from docling-core 1.36 on; building
    /// one with all four fields is what pins the floor of the dependency.
    #[test]
    fn group_has_its_1_36_fields() {
        let group = Node::Group {
            label: "sheet".into(),
            name: Some("Sheet1".into()),
            layer: Some(ContentLayer::Invisible),
            children: Vec::new(),
        };
        assert_eq!(kind(&group), "group");
    }
}
