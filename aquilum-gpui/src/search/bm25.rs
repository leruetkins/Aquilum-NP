//! Hand-rolled BM25F over the in-memory index (stage B).
//!
//! Parameters come from the existing `Bm25fParams` settings. `k3` is not
//! used: term frequency in the *query* is always 1 (bag of words), so there
//! is nothing to saturate. Field weights: `title_weight` from settings for
//! the title, 2.0 for path, 1.0 for body — no double boosting. Both short
//! fields (title, path) use `b_title`; the body uses `b_body`.
//!
//! Scores are per PAGE (the reference indexes pages as separate documents):
//! title/path tokens live on a file's first page only, so their norms never
//! dilute; body norms use the page-level average.

/// BM25 parameters snapshot (from `settings::Bm25fParams`).
#[derive(Clone, Copy, Debug)]
pub struct Bm25 {
    pub k1: f32,
    pub b_title: f32,
    pub b_body: f32,
    pub title_weight: f32,
}

/// Path field weight and body field weight (fixed, see module docs).
const PATH_WEIGHT: f32 = 2.0;
const BODY_WEIGHT: f32 = 1.0;

/// Body tokens per field triplet `[title, path, body]` of one page.
pub type PageLengths = [u32; 3];

impl Bm25 {
    pub fn from_settings(p: &crate::settings::Bm25fParams) -> Self {
        Self {
            k1: p.k1,
            b_title: p.b_title,
            b_body: p.b_body,
            title_weight: p.title_weight,
        }
    }

    /// Inverse document frequency for `df` pages containing the term out of
    /// `n` indexed pages.
    fn idf(&self, n: usize, df: usize) -> f32 {
        let n = n as f32;
        let df = df as f32;
        (1.0 + (n - df + 0.5) / (df + 0.5)).ln()
    }

    /// Saturating term-frequency component for one field.
    fn tf_part(&self, tf: u16, doc_len: u32, avg_len: f32, b: f32) -> f32 {
        let tf = tf as f32;
        let norm = if avg_len > 0.0 {
            b * (doc_len as f32 / avg_len)
        } else {
            0.0
        };
        (tf * (self.k1 + 1.0)) / (tf + self.k1 * (1.0 - b + norm))
    }

    /// Field-weighted BM25F contribution of one query term for one page.
    ///
    /// `df` is the size of the term's candidate set (exact + prefix union).
    /// `exact` doubles the contribution: a page containing the term exactly
    /// outranks one only matching via a prefix expansion.
    pub fn score_term(
        &self,
        counts: [u16; 3],
        doc_lens: PageLengths,
        avg_lens: [f32; 3],
        n: usize,
        df: usize,
        exact: bool,
    ) -> f32 {
        let weighted = self.title_weight * self.tf_part(counts[0], doc_lens[0], avg_lens[0], self.b_title)
            + PATH_WEIGHT * self.tf_part(counts[1], doc_lens[1], avg_lens[1], self.b_title)
            + BODY_WEIGHT * self.tf_part(counts[2], doc_lens[2], avg_lens[2], self.b_body);
        let score = self.idf(n, df) * weighted;
        if exact {
            score * 2.0
        } else {
            score
        }
    }
}
