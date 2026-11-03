#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodeBlockKind {
    OpeningFence,
    InsideCode,
    ClosingFence,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeBlockState {
    pub kind: CodeBlockKind,
    pub lang: Option<String>,
    pub block_id: usize,
}

/// Fence state *before* a line: everything the scanner needs to resume.
///
/// The scanner is a two-variable machine, so this is the whole of its memory.
/// That is what makes a mid-document restart possible: a window can be told the
/// state at its first line instead of being scanned from the top of the file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CodeScanState {
    /// Whether the scanner is inside a fenced block.
    pub in_code_block: bool,
    /// Id the next fence pair will carry.
    pub block_id: usize,
}

/// Resumable fence scanner — the machine behind [`analyze_code_blocks`].
///
/// Identical logic to the old one-shot loop, but the state lives in a struct
/// that can be saved (see [`crate::code_scan::CodeCheckpoints`]) and restored,
/// so the document can be scanned in pieces instead of all at once.
#[derive(Debug, Clone, Copy, Default)]
pub struct CodeScanner {
    state: CodeScanState,
}

impl CodeScanner {
    pub fn new() -> Self {
        Self::default()
    }

    /// Resume from a saved state.
    pub fn from_state(state: CodeScanState) -> Self {
        Self { state }
    }

    /// The state that applies to the next line.
    pub fn state(&self) -> CodeScanState {
        self.state
    }

    /// Consume one line, returning the state that applies to it.
    pub fn step(&mut self, line: &str) -> Option<CodeBlockState> {
        let trimmed = line.trim();
        if trimmed.starts_with("```") {
            if self.state.in_code_block {
                let block_id = self.state.block_id;
                self.state.in_code_block = false;
                self.state.block_id += 1;
                Some(CodeBlockState {
                    kind: CodeBlockKind::ClosingFence,
                    lang: None,
                    block_id,
                })
            } else {
                let block_id = self.state.block_id;
                let lang = trimmed.trim_start_matches('`').trim();
                let lang_opt = if lang.is_empty() {
                    None
                } else {
                    Some(lang.to_string())
                };
                self.state.in_code_block = true;
                Some(CodeBlockState {
                    kind: CodeBlockKind::OpeningFence,
                    lang: lang_opt,
                    block_id,
                })
            }
        } else if self.state.in_code_block {
            Some(CodeBlockState {
                kind: CodeBlockKind::InsideCode,
                lang: None,
                block_id: self.state.block_id,
            })
        } else {
            None
        }
    }
}

pub fn analyze_code_blocks(lines: &[&str]) -> Vec<Option<CodeBlockState>> {
    let mut scanner = CodeScanner::new();
    lines.iter().map(|line| scanner.step(line)).collect()
}
