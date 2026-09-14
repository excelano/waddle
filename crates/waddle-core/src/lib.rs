//! waddle's core: office packages written from a docling `DoclingDocument`.
//!
//! A `DoclingDocument` in, the bytes of an OpenDocument or Office Open XML
//! package out. Nothing here reads a file, prints to a terminal or draws a
//! window; the command-line tool and any application are callers over this
//! crate. `DESIGN.md` in the repository is the reasoning.
//!
//! The only docling dependency is `docling-core`, the model crate. Every
//! match on [`Node`] in this crate is exhaustive on purpose: when docling.rs
//! adds a variant or a field, the build fails here and the writer decides
//! what to do with it, rather than dropping it without a word.
//!
//! Author: David M. Anderson
//! Built with AI assistance (Claude, Anthropic)

#![forbid(unsafe_code)]

pub mod docx;
mod error;
mod inline;
mod list;
mod media;
pub mod node;
pub mod ods;
pub mod odt;
mod package;
pub mod report;
pub mod target;
mod xml;

/// The model crate, re-exported so a caller uses the version this crate
/// was built against.
pub use docling_core;
pub use docling_core::{DoclingDocument, Node};
pub use error::Error;
pub use report::{Output, Reason, Warning};
pub use target::Target;
