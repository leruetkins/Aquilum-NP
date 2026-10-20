//! Единственный механизм undo/redo: атомарные шаги без склеек
//! (семантика AECO_DETAILEDUNDO из AkelPad). Ядро не знает о rope/String.

use std::collections::VecDeque;

/// Ёмкость undo-истории; выбрасывается старейший шаг (FIFO).
/// Redo-стек капа не имеет: шаг попадает туда только через take.
pub const HISTORY_CAP: usize = 10_000;

/// Позиция курсора и, если есть, якорь выделения.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Selection {
    pub cursor: usize,
    pub anchor: Option<usize>,
}

impl Selection {
    pub fn cursor_only(cursor: usize) -> Self {
        Self { cursor, anchor: None }
    }
}

/// Правка: заменить байты `at..at + removed.len()` на `inserted`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextChange {
    /// Byte offset where the change occurred.
    pub at: usize,
    /// Text removed by this change (empty for pure insertion).
    pub removed: String,
    /// Text inserted by this change (empty for pure deletion).
    pub inserted: String,
}

impl TextChange {
    fn is_pure_insertion(&self) -> bool {
        self.removed.is_empty() && !self.inserted.is_empty()
    }

    fn is_pure_deletion(&self) -> bool {
        self.inserted.is_empty() && !self.removed.is_empty()
    }
}

/// Один шаг undo: одна или несколько правок плюс выделение до и после.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    pub changes: Vec<TextChange>,
    pub sel_before: Selection,
    pub sel_after: Selection,
}

/// Стеки undo/redo. Все шаги атомарны и записываются как есть — склейки нет.
#[derive(Default)]
pub struct History {
    undo: VecDeque<Step>,
    redo: VecDeque<Step>,
}

impl History {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record(&mut self, step: Step) {
        // A pure deletion that exactly reverts the last step annuls it:
        // typed and immediately erased leaves no trace (AkelPad
        // AE_StackUndoGroupStop removes the mutually exclusive pair).
        // The deletion itself still invalidates redo.
        if let (Some([last_change]), [change]) = (self.undo.back().map(|last| last.changes.as_slice()), step.changes.as_slice()) {
            if last_change.is_pure_insertion()
                && change.is_pure_deletion()
                && change.at == last_change.at
                && change.removed == last_change.inserted
            {
                self.undo.pop_back();
                self.redo.clear();
                return;
            }
        }
        self.redo.clear();
        self.undo.push_back(step);
        while self.undo.len() > HISTORY_CAP {
            self.undo.pop_front();
        }
    }

    /// Снимает шаг с верхушки и переносит на противоположный стек.
    pub fn take(&mut self, redo: bool) -> Option<Step> {
        let (from, to) = if redo {
            (&mut self.redo, &mut self.undo)
        } else {
            (&mut self.undo, &mut self.redo)
        };
        let step = from.pop_back()?;
        to.push_back(step.clone());
        Some(step)
    }

    pub fn undo_len(&self) -> usize {
        self.undo.len()
    }

    pub fn redo_len(&self) -> usize {
        self.redo.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn insert(at: usize, text: &str) -> TextChange {
        TextChange { at, removed: String::new(), inserted: text.to_owned() }
    }

    fn delete(at: usize, text: &str) -> TextChange {
        TextChange { at, removed: text.to_owned(), inserted: String::new() }
    }

    fn step(change: TextChange) -> Step {
        Step {
            changes: vec![change],
            sel_before: Selection::cursor_only(0),
            sel_after: Selection::cursor_only(0),
        }
    }

    #[test]
    fn recording_clears_redo() {
        let mut history = History::new();
        history.record(step(insert(0, "a")));
        assert_eq!(history.take(false).is_some(), true);
        assert_eq!(history.redo_len(), 1);
        history.record(step(insert(1, "b")));
        assert_eq!(history.redo_len(), 0);
    }

    #[test]
    fn take_moves_steps_between_stacks() {
        let mut history = History::new();
        history.record(step(insert(0, "a")));
        history.record(step(insert(1, "b")));
        let undo_step = history.take(false).unwrap();
        assert_eq!(undo_step.changes[0], insert(1, "b"));
        assert_eq!(history.undo_len(), 1);
        assert_eq!(history.redo_len(), 1);
        let redo_step = history.take(true).unwrap();
        assert_eq!(redo_step.changes[0], insert(1, "b"));
        assert_eq!(history.undo_len(), 2);
        assert_eq!(history.redo_len(), 0);
    }

    #[test]
    fn take_on_empty_stack_is_none() {
        let mut history = History::new();
        assert!(history.take(false).is_none());
        assert!(history.take(true).is_none());
    }

    #[test]
    fn oldest_step_is_dropped_when_cap_is_reached() {
        let mut history = History::new();
        for at in 0..(HISTORY_CAP + 1) {
            history.record(step(insert(at, "x")));
        }
        assert_eq!(history.undo_len(), HISTORY_CAP);
        // The oldest step (at = 0) is gone; the first undo returns the newest.
        let newest = history.take(false).unwrap();
        assert_eq!(newest.changes[0].at, HISTORY_CAP);
    }

    #[test]
    fn pure_deletion_right_after_its_insertion_annuls_both() {
        let mut history = History::new();
        history.record(step(insert(3, "b")));
        history.record(step(delete(3, "b")));
        assert_eq!(history.undo_len(), 0);
        assert!(history.take(false).is_none());
    }

    #[test]
    fn non_matching_pairs_survive() {
        let mut history = History::new();
        // Different text: stays.
        history.record(step(insert(3, "b")));
        history.record(step(delete(3, "c")));
        assert_eq!(history.undo_len(), 2);
        // Different position: stays.
        history.record(step(insert(5, "x")));
        history.record(step(delete(4, "x")));
        assert_eq!(history.undo_len(), 4);
    }

    #[test]
    fn deletion_of_both_typed_characters_leaves_one_step() {
        let mut history = History::new();
        history.record(step(insert(3, "b")));
        history.record(step(insert(4, "c")));
        history.record(step(delete(4, "c")));
        // "c" is annulled, "b" remains undoable.
        assert_eq!(history.undo_len(), 1);
        history.record(step(delete(3, "b")));
        assert_eq!(history.undo_len(), 0);
    }

    #[test]
    fn selection_round_trips_through_take() {
        let mut history = History::new();
        history.record(Step {
            changes: vec![insert(0, "word")],
            sel_before: Selection::cursor_only(0),
            sel_after: Selection { cursor: 4, anchor: Some(0) },
        });
        let step = history.take(false).unwrap();
        assert_eq!(step.sel_before, Selection::cursor_only(0));
        assert_eq!(step.sel_after, Selection { cursor: 4, anchor: Some(0) });
    }

    #[test]
    fn multi_change_step_stays_atomic() {
        let mut history = History::new();
        history.record(Step {
            changes: vec![insert(0, "a"), insert(5, "b")],
            sel_before: Selection::cursor_only(0),
            sel_after: Selection::cursor_only(6),
        });
        assert_eq!(history.undo_len(), 1);
        let step = history.take(false).unwrap();
        assert_eq!(step.changes.len(), 2);
    }
}
