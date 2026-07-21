use beambench_core::Project;
use serde::Serialize;

const MAX_HISTORY_DEPTH: usize = 50;

#[derive(Debug, Clone, Copy, Default, Serialize)]
pub struct UndoState {
    pub can_undo: bool,
    pub can_redo: bool,
}

#[derive(Debug, Default)]
pub struct ProjectHistory {
    undo_stack: Vec<Project>,
    redo_stack: Vec<Project>,
}

impl ProjectHistory {
    pub fn state(&self) -> UndoState {
        UndoState {
            can_undo: !self.undo_stack.is_empty(),
            can_redo: !self.redo_stack.is_empty(),
        }
    }

    pub fn clear(&mut self) {
        self.undo_stack.clear();
        self.redo_stack.clear();
    }

    pub fn push_snapshot(&mut self, project: &Project) {
        self.undo_stack.push(project.clone());
        if self.undo_stack.len() > MAX_HISTORY_DEPTH {
            self.undo_stack.remove(0);
        }
        self.redo_stack.clear();
    }

    pub fn undo(&mut self, current: &Project) -> Option<Project> {
        let previous = self.undo_stack.pop()?;
        self.redo_stack.push(current.clone());
        Some(previous)
    }

    pub fn redo(&mut self, current: &Project) -> Option<Project> {
        let next = self.redo_stack.pop()?;
        self.undo_stack.push(current.clone());
        Some(next)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_project(name: &str) -> Project {
        let mut project = Project::new(name.to_string());
        project.dirty = true;
        project
    }

    #[test]
    fn push_snapshot_enables_undo_and_clears_redo() {
        let mut history = ProjectHistory::default();
        let project = sample_project("A");
        history.push_snapshot(&project);
        assert!(history.state().can_undo);
        assert!(!history.state().can_redo);
    }

    #[test]
    fn undo_moves_current_to_redo_stack() {
        let mut history = ProjectHistory::default();
        let original = sample_project("Original");
        let current = sample_project("Current");
        history.push_snapshot(&original);

        let restored = history.undo(&current).unwrap();
        assert_eq!(restored.metadata.project_name, "Original");
        let state = history.state();
        assert!(!state.can_undo);
        assert!(state.can_redo);
    }

    #[test]
    fn redo_restores_next_snapshot() {
        let mut history = ProjectHistory::default();
        let original = sample_project("Original");
        let current = sample_project("Current");
        history.push_snapshot(&original);
        let _ = history.undo(&current).unwrap();

        let redone = history.redo(&original).unwrap();
        assert_eq!(redone.metadata.project_name, "Current");
    }

    #[test]
    fn clear_resets_state() {
        let mut history = ProjectHistory::default();
        history.push_snapshot(&sample_project("A"));
        history.clear();
        let state = history.state();
        assert!(!state.can_undo);
        assert!(!state.can_redo);
    }
}
