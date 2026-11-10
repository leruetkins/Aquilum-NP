//! The desktop full-text engine, reused by path so ranking cannot drift
//! between the two front ends: tokenizer, page splitter, inverted index,
//! BM25F and the ranked query. The GPUI worker, content cache and the
//! persisted `VBIX` snapshot stay desktop-only; the mobile session in
//! `search_session` feeds this index from the repository instead.
//!
//! `#[path]` is relative to this directory (`core/src/search/`), so the
//! shared sources sit four levels up.

#[path = "../../../../aquilum-gpui/src/search/fold.rs"]
pub mod fold;

#[path = "../../../../aquilum-gpui/src/search/pages.rs"]
pub mod pages;

#[path = "../../../../aquilum-gpui/src/search/tokenize.rs"]
pub mod tokenize;

#[path = "../../../../aquilum-gpui/src/search/bm25.rs"]
pub mod bm25;

#[path = "../../../../aquilum-gpui/src/search/index.rs"]
pub mod index;

#[path = "../../../../aquilum-gpui/src/search/query.rs"]
pub mod query;
