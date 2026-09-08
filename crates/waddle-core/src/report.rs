//! What a writer hands back: the package bytes and what it could not carry.
//!
//! Author: David M. Anderson
//! Built with AI assistance (Claude, Anthropic)

use std::fmt;

/// A written package and the warnings raised while writing it.
///
/// A warning is not a failure: the bytes are complete and the package opens.
/// A caller that wants none, such as the CLI under `--strict`, refuses a
/// non-empty list before writing the bytes anywhere.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Output {
    pub bytes: Vec<u8>,
    pub warnings: Vec<Warning>,
}

/// One node the target could not carry as it was, named by its variant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Warning {
    /// The variant name from [`crate::node::kind`].
    pub node: &'static str,
    pub reason: Reason,
}

/// Why a node was dropped or degraded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reason {
    /// The node sits on a content layer the target has no place for:
    /// `furniture`, `notes` or `invisible`.
    Layer(&'static str),
    /// The target has no construct for the node in this release.
    Unsupported,
    /// The node was written as a placeholder, such as a picture without bytes.
    Placeholder,
}

impl fmt::Display for Warning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.reason {
            Reason::Layer(layer) => write!(f, "{}: dropped, on the {layer} layer", self.node),
            Reason::Unsupported => {
                write!(f, "{}: dropped, not supported by this target", self.node)
            }
            Reason::Placeholder => write!(f, "{}: written as a placeholder", self.node),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn warnings_read_as_one_line() {
        let w = Warning {
            node: "picture",
            reason: Reason::Placeholder,
        };
        assert_eq!(w.to_string(), "picture: written as a placeholder");
        let w = Warning {
            node: "paragraph",
            reason: Reason::Layer("furniture"),
        };
        assert_eq!(w.to_string(), "paragraph: dropped, on the furniture layer");
    }
}
