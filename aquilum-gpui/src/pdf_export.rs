//! PDF export module powered by Typst.
//!
//! Provides native, 100% autonomous Markdown -> Typst -> PDF rendering without
//! external tools (no Chromium, Node.js, or LaTeX required).

mod export;
mod typst;
mod world;

pub use export::*;
pub use typst::*;
pub use world::*;

#[cfg(test)]
mod tests;
