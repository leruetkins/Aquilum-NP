use crate::sync::decide::Action;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Count {
    pub done: usize,
    pub total: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Progress {
    pub done: usize,
    pub total: usize,
    pub down: Count,
    pub up: Count,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    Download(Count),
    Upload(Count),
    Other(Count),
}

pub fn counted(action: Action) -> bool {
    !matches!(action, Action::Match | Action::Forget)
}

impl Progress {
    pub fn plan(actions: impl Iterator<Item = Action>) -> Self {
        let mut progress = Progress::default();
        for action in actions.filter(|action| counted(*action)) {
            progress.total += 1;
            if let Some(side) = progress.side(action) {
                side.total += 1;
            }
        }
        progress
    }

    pub fn step(&mut self, action: Action) -> bool {
        if !counted(action) {
            return false;
        }
        self.done += 1;
        if let Some(side) = self.side(action) {
            side.done += 1;
        }
        true
    }

    pub fn stage(&self) -> Stage {
        if self.down.done < self.down.total {
            Stage::Download(self.down)
        } else if self.up.done < self.up.total {
            Stage::Upload(self.up)
        } else {
            Stage::Other(Count { done: self.done, total: self.total })
        }
    }

    fn side(&mut self, action: Action) -> Option<&mut Count> {
        match action {
            Action::Pull | Action::Conflict => Some(&mut self.down),
            Action::Push => Some(&mut self.up),
            _ => None,
        }
    }
}
