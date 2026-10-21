//! Fast search subsystem (see `TODO/search-index-plan.md`).
//!
//! Stage A adds the folding matcher ([`fold`]) and the background scan
//! worker; stage B extends this module with the in-memory inverted index.

pub mod bm25;
pub mod cancel;
pub mod cache;
pub mod fold;
pub mod index;
pub mod locate;
pub mod pages;
pub mod query;
pub mod tokenize;
pub mod worker;
