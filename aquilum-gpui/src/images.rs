//! Image attachments for notes.
//!
//! A note keeps only `![[file-name.png]]`; the bytes live in an `attachments`
//! folder next to the note. The folder name is configurable (`files.folder`).
//! Resolution order when a reference is opened, copied or exported is the one
//! from `TODO/images/images.md`.



mod config;
mod import;
mod resolve;

pub use config::*;
pub use import::*;
pub use resolve::*;

mod notes;
mod reference;
mod ref_index;

pub use reference::*;
pub use ref_index::*;

mod annotate;
mod edit;

pub use annotate::*;
pub use edit::*;
mod resolved_image;
pub use resolved_image::*;

mod clipboard;
mod trash;

pub use clipboard::*;
pub use trash::*;


// ───────────────────────────── References ─────────────────────────────


// ───────────────────────────── Annotations ─────────────────────────────


// ───────────────────────────── Edit sidecar ─────────────────────────────



#[cfg(test)]
mod tests;

