//! Undo and redo stacks of file snapshots (ADR 0003 #5, #7).

use std::collections::VecDeque;

/// The most steps kept; the oldest are dropped first.
pub const UNDO_LIMIT: usize = 1000;

/// One file changed by a step: its bytes before and after (`None` = absent).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Change {
    pub path: String,
    pub before: Option<Vec<u8>>,
    pub after: Option<Vec<u8>>,
}

/// One command's effect.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Step {
    pub command: String,
    pub changes: Vec<Change>,
}

#[derive(Debug, Default)]
pub(crate) struct History {
    pub undo: VecDeque<Step>,
    pub redo: VecDeque<Step>,
}

fn bounded_push(stack: &mut VecDeque<Step>, step: Step) {
    stack.push_back(step);
    while stack.len() > UNDO_LIMIT {
        stack.pop_front();
    }
}

impl History {
    /// Records a new step and clears redo. An empty step is ignored.
    pub fn push(&mut self, step: Step) {
        if step.changes.is_empty() {
            return;
        }
        self.redo.clear();
        bounded_push(&mut self.undo, step);
    }

    /// Moves the newest undo step to redo.
    pub fn undone(&mut self) {
        if let Some(s) = self.undo.pop_back() {
            bounded_push(&mut self.redo, s);
        }
    }

    /// Moves the newest redo step back to undo.
    pub fn redone(&mut self) {
        if let Some(s) = self.redo.pop_back() {
            bounded_push(&mut self.undo, s);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn step(n: usize) -> Step {
        Step { command: "x.y".into(), changes: vec![Change { path: format!("{n}"), before: None, after: Some(vec![1]) }] }
    }

    #[test]
    fn push_clears_redo_and_ignores_empty_steps() {
        let mut h = History::default();
        h.push(step(1));
        h.undone();
        assert_eq!(h.redo.len(), 1);
        h.push(Step { command: "x.y".into(), changes: vec![] });
        assert_eq!(h.redo.len(), 1);
        h.push(step(2));
        assert!(h.redo.is_empty());
    }

    #[test]
    fn bounded() {
        let mut h = History::default();
        for i in 0..UNDO_LIMIT + 3 {
            h.push(step(i));
        }
        assert_eq!(h.undo.len(), UNDO_LIMIT);
        assert_eq!(h.undo.front().and_then(|s| s.changes.first()).map(|c| c.path.as_str()), Some("3"));
    }
}
