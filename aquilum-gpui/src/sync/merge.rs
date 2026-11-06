//! Line-based three-way merge, ported from the Aquilum document layer and
//! adapted to byte offsets (the GPUI editor works on UTF-8 byte offsets; the
//! merge never splits inside a character because it works line by line).

use std::cmp::Ordering;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextEdit {
    pub from: usize,
    pub to: usize,
    pub insert: String,
}

pub const COARSE_MERGE_THRESHOLD: usize = 1000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LineHunk {
    pub from: usize,
    pub to: usize,
    pub lines: Vec<String>,
}

pub struct LineDiff {
    pub hunks: Vec<LineHunk>,
    pub coarse: bool,
}

pub fn split_lines(text: &str) -> Vec<&str> {
    text.split('\n').collect()
}

struct CommonSubsequenceTable {
    columns: usize,
    cells: Vec<u32>,
}

impl CommonSubsequenceTable {
    fn build(left: &[&str], right: &[&str]) -> Self {
        let columns = right.len() + 1;
        let mut table = Self { columns, cells: vec![0; (left.len() + 1) * columns] };
        for row in (0..left.len()).rev() {
            for column in (0..right.len()).rev() {
                let value = if left[row] == right[column] {
                    table.at(row + 1, column + 1) + 1
                } else {
                    table.at(row + 1, column).max(table.at(row, column + 1))
                };
                table.cells[row * columns + column] = value;
            }
        }
        table
    }

    fn at(&self, row: usize, column: usize) -> u32 {
        self.cells[row * self.columns + column]
    }
}

fn empty_hunk(at: usize) -> LineHunk {
    LineHunk { from: at, to: at, lines: Vec::new() }
}

fn aligned_hunks(base: &[&str], next: &[&str], offset: usize) -> Vec<LineHunk> {
    let table = CommonSubsequenceTable::build(base, next);
    let mut hunks = Vec::new();
    let mut open: Option<LineHunk> = None;
    let (mut base_index, mut next_index) = (0, 0);

    while base_index < base.len() && next_index < next.len() {
        if base[base_index] == next[next_index] {
            hunks.extend(open.take());
            base_index += 1;
            next_index += 1;
        } else if table.at(base_index + 1, next_index) >= table.at(base_index, next_index + 1) {
            open.get_or_insert_with(|| empty_hunk(offset + base_index)).to = offset + base_index + 1;
            base_index += 1;
        } else {
            open.get_or_insert_with(|| empty_hunk(offset + base_index))
                .lines
                .push(next[next_index].to_owned());
            next_index += 1;
        }
    }
    if base_index < base.len() {
        open.get_or_insert_with(|| empty_hunk(offset + base_index)).to = offset + base.len();
    }
    if next_index < next.len() {
        open.get_or_insert_with(|| empty_hunk(offset + base_index))
            .lines
            .extend(next[next_index..].iter().map(|line| (*line).to_owned()));
    }
    hunks.extend(open);
    hunks
}

pub fn line_diff(base: &[&str], next: &[&str]) -> LineDiff {
    let limit = base.len().min(next.len());
    let head = (0..limit).take_while(|&index| base[index] == next[index]).count();
    let tail = (0..limit - head)
        .take_while(|&index| base[base.len() - 1 - index] == next[next.len() - 1 - index])
        .count();

    let base_middle = &base[head..base.len() - tail];
    let next_middle = &next[head..next.len() - tail];
    if base_middle.is_empty() && next_middle.is_empty() {
        return LineDiff { hunks: Vec::new(), coarse: false };
    }
    if base_middle.len() > COARSE_MERGE_THRESHOLD || next_middle.len() > COARSE_MERGE_THRESHOLD {
        let lines = next_middle.iter().map(|line| (*line).to_owned()).collect();
        return LineDiff {
            hunks: vec![LineHunk { from: head, to: base.len() - tail, lines }],
            coarse: true,
        };
    }
    LineDiff { hunks: aligned_hunks(base_middle, next_middle, head), coarse: false }
}

pub fn apply_hunks(base: &[&str], hunks: &[LineHunk], from: usize, to: usize) -> Vec<String> {
    let mut result = Vec::new();
    let mut cursor = from;
    for hunk in hunks {
        if hunk.from > cursor {
            result.extend(base[cursor..hunk.from].iter().map(|line| (*line).to_owned()));
        }
        result.extend(hunk.lines.iter().cloned());
        cursor = hunk.to;
    }
    if cursor < to {
        result.extend(base[cursor..to].iter().map(|line| (*line).to_owned()));
    }
    result
}

fn line_offsets(lines: &[&str]) -> Vec<usize> {
    let mut offsets = Vec::with_capacity(lines.len() + 1);
    let mut position = 0;
    for line in lines {
        offsets.push(position);
        position += line.len() + 1;
    }
    offsets.push(position);
    offsets
}

fn terminated_edits(offsets: &[usize], hunks: &[LineHunk]) -> Vec<TextEdit> {
    hunks
        .iter()
        .map(|hunk| TextEdit {
            from: offsets[hunk.from],
            to: offsets[hunk.to],
            insert: hunk.lines.iter().map(|line| format!("{line}\n")).collect(),
        })
        .collect()
}

fn drop_terminator(insert: &str) -> &str {
    &insert[..insert.len().saturating_sub(1)]
}

fn without_terminator(edit: TextEdit, text_length: usize) -> TextEdit {
    if edit.to <= text_length {
        return edit;
    }
    if edit.from > text_length {
        let insert = if edit.insert.is_empty() {
            String::new()
        } else {
            format!("\n{}", drop_terminator(&edit.insert))
        };
        return TextEdit { from: text_length, to: text_length, insert };
    }
    if !edit.insert.is_empty() {
        let insert = drop_terminator(&edit.insert).to_owned();
        return TextEdit { from: edit.from, to: text_length, insert };
    }
    TextEdit { from: edit.from.saturating_sub(1), to: text_length, insert: String::new() }
}

fn with_tail_joined(mut edits: Vec<TextEdit>, text: &str) -> Vec<TextEdit> {
    if edits.len() < 2 {
        return edits;
    }
    let tail = edits.pop().expect("at least two edits");
    let ahead = edits.pop().expect("at least two edits");
    if tail.from > ahead.to {
        edits.push(ahead);
        edits.push(tail);
        return edits;
    }
    let bridged = if ahead.to > tail.from {
        let kept = ahead.insert.len().saturating_sub(ahead.to - tail.from);
        ahead.insert[..kept].to_owned()
    } else {
        ahead.insert.clone() + &text[ahead.to..tail.from]
    };
    edits.push(TextEdit {
        from: ahead.from,
        to: ahead.to.max(tail.to),
        insert: bridged + &tail.insert,
    });
    edits
}

pub fn to_char_edits(lines: &[&str], hunks: &[LineHunk]) -> Vec<TextEdit> {
    if hunks.is_empty() {
        return Vec::new();
    }
    let text = lines.join("\n");
    let mut edits = terminated_edits(&line_offsets(lines), hunks);
    let last = edits.pop().expect("hunks are not empty");
    edits.push(without_terminator(last, text.len()));
    with_tail_joined(edits, &text)
}

pub fn apply_edits(text: &str, edits: &[TextEdit]) -> String {
    let mut ordered: Vec<&TextEdit> = edits.iter().collect();
    ordered.sort_by_key(|edit| edit.from);
    let mut result = String::with_capacity(text.len());
    let mut cursor = 0usize;
    for edit in ordered {
        let from = edit.from.min(text.len());
        let to = edit.to.min(text.len());
        if from > cursor {
            result.push_str(&text[cursor..from]);
        }
        result.push_str(&edit.insert);
        cursor = to.max(cursor);
    }
    if cursor < text.len() {
        result.push_str(&text[cursor..]);
    }
    result
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct MergeResult {
    pub edits: Vec<TextEdit>,
    pub displaced: Vec<String>,
    pub coarse: bool,
}

struct SidedHunk {
    hunk: LineHunk,
    external: bool,
}

fn overlaps(left: &LineHunk, right: &LineHunk) -> bool {
    left.from < right.to && right.from < left.to
}

fn contains(region: &LineHunk, hunk: &LineHunk) -> bool {
    hunk.from >= region.from && hunk.to <= region.to
}

fn by_position(left: &LineHunk, right: &LineHunk) -> Ordering {
    left.from.cmp(&right.from).then(left.to.cmp(&right.to))
}

fn regions(mut hunks: Vec<SidedHunk>) -> Vec<Vec<SidedHunk>> {
    hunks.sort_by(|left, right| by_position(&left.hunk, &right.hunk));
    let mut grouped: Vec<Vec<SidedHunk>> = Vec::new();
    let mut reach = 0;
    for sided in hunks {
        match grouped.last_mut() {
            Some(region) if sided.hunk.from < reach => {
                reach = reach.max(sided.hunk.to);
                region.push(sided);
            }
            _ => {
                reach = sided.hunk.to;
                grouped.push(vec![sided]);
            }
        }
    }
    grouped
}

fn side(region: &[SidedHunk], external: bool) -> Vec<LineHunk> {
    region.iter().filter(|sided| sided.external == external).map(|sided| sided.hunk.clone()).collect()
}

pub fn merge_external_change(base: &str, external: &str, current: &str) -> MergeResult {
    if external == current {
        return MergeResult::default();
    }
    let base_lines = split_lines(base);
    let external_diff = line_diff(&base_lines, &split_lines(external));
    if external_diff.hunks.is_empty() {
        return MergeResult::default();
    }
    let current_lines = split_lines(current);
    let local_diff = line_diff(&base_lines, &current_lines);

    let sided = external_diff
        .hunks
        .iter()
        .map(|hunk| SidedHunk { hunk: hunk.clone(), external: true })
        .chain(local_diff.hunks.iter().map(|hunk| SidedHunk { hunk: hunk.clone(), external: false }))
        .collect();

    let mut displaced = Vec::new();
    let mut resolved = Vec::new();
    for region in regions(sided) {
        let external = side(&region, true);
        if external.is_empty() {
            continue;
        }
        let from = region.iter().map(|sided| sided.hunk.from).min().unwrap_or(0);
        let to = region.iter().map(|sided| sided.hunk.to).max().unwrap_or(0);
        let local = side(&region, false);
        if !local.is_empty() {
            displaced.extend(apply_hunks(&base_lines, &local, from, to));
        }
        resolved.push(LineHunk { from, to, lines: apply_hunks(&base_lines, &external, from, to) });
    }

    let untouched_local = local_diff
        .hunks
        .iter()
        .filter(|hunk| !resolved.iter().any(|region| overlaps(region, hunk) || contains(region, hunk)))
        .cloned();
    let mut kept: Vec<LineHunk> = resolved.iter().cloned().chain(untouched_local).collect();
    kept.sort_by(by_position);
    let merged = apply_hunks(&base_lines, &kept, 0, base_lines.len());
    let merged: Vec<&str> = merged.iter().map(String::as_str).collect();

    MergeResult {
        edits: to_char_edits(&current_lines, &line_diff(&current_lines, &merged).hunks),
        displaced: displaced.into_iter().filter(|line| !line.trim().is_empty()).collect(),
        coarse: external_diff.coarse || local_diff.coarse,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn merge(base: &str, external: &str, current: &str) -> (String, Vec<String>, bool) {
        let result = merge_external_change(base, external, current);
        (apply_edits(current, &result.edits), result.displaced, result.coarse)
    }

    #[test]
    fn keeps_local_and_external_edits_on_different_lines() {
        let (text, displaced, coarse) = merge("один\nдва\nтри\n", "один\nдва\nтри!\n", "один!\nдва\nтри\n");
        assert_eq!(text, "один!\nдва\nтри!\n");
        assert!(displaced.is_empty());
        assert!(!coarse);
    }

    #[test]
    fn external_wins_a_shared_line_and_preserves_the_local_one() {
        let (text, displaced, _) = merge("один\nдва\nтри\n", "один\nдва от агента\nтри\n", "один\nдва от меня\nтри\n");
        assert_eq!(text, "один\nдва от агента\nтри\n");
        assert_eq!(displaced, vec!["два от меня".to_owned()]);
    }

    #[test]
    fn an_empty_document_takes_the_external_file_exactly() {
        let (text, displaced, _) = merge("", "первая\nвторая", "");
        assert_eq!(text, "первая\nвторая");
        assert!(displaced.is_empty());
    }

    #[test]
    fn handles_the_last_line_without_a_trailing_newline() {
        let (text, _, _) = merge("один\nдва", "один\nдва и хвост", "один\nдва");
        assert_eq!(text, "один\nдва и хвост");
    }

    #[test]
    fn does_nothing_when_the_external_side_did_not_move() {
        let result = merge_external_change("один\nдва\n", "один\nдва\n", "один\nдва изменённый\n");
        assert!(result.edits.is_empty());
        assert!(!result.coarse);
    }

    #[test]
    fn coarse_merge_never_drops_local_lines() {
        let base = (0..20).map(|index| format!("строка {index}")).collect::<Vec<_>>().join("\n");
        let external = (0..COARSE_MERGE_THRESHOLD + 5)
            .map(|index| format!("совсем другое {index}"))
            .collect::<Vec<_>>()
            .join("\n");
        let current = base.replace("строка 5", "моя правка");
        let (_, displaced, coarse) = merge(&base, &external, &current);
        assert!(coarse);
        assert!(displaced.contains(&"моя правка".to_owned()));
    }
}
