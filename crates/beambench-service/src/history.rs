use beambench_core::Project;
use serde::Serialize;

const MAX_HISTORY_DEPTH: usize = 50;
/// Approximate memory the undo history may hold. Assets are shared between
/// snapshots, but path and text data are copied, so a very large design
/// keeps fewer steps rather than exhausting memory.
const MAX_HISTORY_BYTES: usize = 512 * 1024 * 1024;

#[derive(Debug, Clone, Copy, Default, Serialize)]
pub struct UndoState {
    pub can_undo: bool,
    pub can_redo: bool,
}

#[derive(Debug, Default)]
pub struct ProjectHistory {
    undo_stack: Vec<Project>,
    redo_stack: Vec<Project>,
    /// Redo entries cleared by the latest snapshot, restored if that edit fails.
    cleared_redo: Vec<Project>,
    /// Undo depth before the serialized edit; eviction waits until commit.
    edit_start: Option<usize>,
}

/// Rough retained size of a snapshot: the copied geometry and text.
fn approximate_snapshot_bytes(project: &Project) -> usize {
    use beambench_core::ObjectData;
    project
        .objects
        .iter()
        .map(|object| {
            512 + match &object.data {
                ObjectData::VectorPath { path_data, .. } => path_data.len(),
                ObjectData::Text {
                    content,
                    resolved_path_data,
                    ..
                } => content.len() + resolved_path_data.as_ref().map_or(0, String::len),
                _ => 0,
            }
        })
        .sum()
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
        self.cleared_redo.clear();
        self.edit_start = None;
    }

    pub fn begin_edit(&mut self) {
        debug_assert!(self.edit_start.is_none(), "nested document transaction");
        self.cleared_redo.clear();
        self.edit_start = Some(self.undo_stack.len());
    }

    pub fn has_edit_snapshot(&self) -> bool {
        self.edit_start
            .is_some_and(|start| self.undo_stack.len() > start)
    }

    pub fn push_snapshot(&mut self, project: &Project) {
        // Keep redo until the pending edit succeeds, and clear it only once
        // even if that edit records more than one intermediate snapshot.
        if self.edit_start == Some(self.undo_stack.len()) {
            self.cleared_redo = std::mem::take(&mut self.redo_stack);
        } else if self.edit_start.is_none() {
            self.redo_stack.clear();
        }
        self.undo_stack.push(project.clone());
        if self.edit_start.is_none() {
            self.enforce_limits();
        }
    }

    pub fn commit_edit(&mut self, before: Option<Project>) {
        if let Some(start) = self.edit_start.take()
            && self.undo_stack.len() > start
        {
            self.undo_stack.truncate(start);
            if let Some(before) = before {
                self.undo_stack.push(before);
            }
        }
        self.cleared_redo.clear();
        self.enforce_limits();
    }

    pub fn rollback_edit(&mut self) {
        if let Some(start) = self.edit_start.take()
            && self.undo_stack.len() > start
        {
            self.undo_stack.truncate(start);
            self.redo_stack = std::mem::take(&mut self.cleared_redo);
        }
    }

    fn enforce_limits(&mut self) {
        while self.undo_stack.len() > MAX_HISTORY_DEPTH {
            self.undo_stack.remove(0);
        }
        let mut retained: usize = self.undo_stack.iter().map(approximate_snapshot_bytes).sum();
        while self.undo_stack.len() > 1 && retained > MAX_HISTORY_BYTES {
            retained -= approximate_snapshot_bytes(&self.undo_stack.remove(0));
        }
    }

    pub fn undo(&mut self, current: &Project) -> Option<Project> {
        let mut previous = self.undo_stack.pop()?;
        self.redo_stack.push(current.clone());
        // A historical clean flag describes a different save point. Until
        // revisions are compared with disk, conservatively protect the edit.
        previous.dirty = true;
        Some(previous)
    }

    pub fn redo(&mut self, current: &Project) -> Option<Project> {
        let mut next = self.redo_stack.pop()?;
        self.undo_stack.push(current.clone());
        next.dirty = true;
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
    fn rollback_restores_redo_after_multiple_pending_snapshots() {
        let mut history = ProjectHistory::default();
        history.push_snapshot(&sample_project("before"));
        history.undo(&sample_project("after")).unwrap();
        history.begin_edit();
        history.push_snapshot(&sample_project("before"));
        history.push_snapshot(&sample_project("intermediate"));
        history.rollback_edit();
        assert!(!history.state().can_undo);
        assert_eq!(
            history
                .redo(&sample_project("before"))
                .unwrap()
                .metadata
                .project_name,
            "after"
        );
    }

    #[test]
    fn pending_history_is_trimmed_only_after_commit() {
        let mut history = ProjectHistory::default();
        for index in 0..MAX_HISTORY_DEPTH {
            history.push_snapshot(&sample_project(&index.to_string()));
        }
        history.begin_edit();
        history.push_snapshot(&sample_project("pending"));
        assert_eq!(history.undo_stack.len(), MAX_HISTORY_DEPTH + 1);
        history.commit_edit(Some(sample_project("before edit")));
        assert_eq!(history.undo_stack.len(), MAX_HISTORY_DEPTH);
        assert_eq!(
            history.undo_stack.first().unwrap().metadata.project_name,
            "1"
        );
        assert_eq!(
            history.undo_stack.last().unwrap().metadata.project_name,
            "before edit"
        );
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
    #[test]
    fn snapshots_share_asset_bytes_and_preserve_replacements() {
        use beambench_core::{Asset, AssetMediaType};
        use std::sync::Arc;
        let mut project = sample_project("image");
        let asset = Asset::new("image.png", AssetMediaType::Png, 1024, Some(32), Some(32));
        let id = asset.id;
        project.add_asset(asset, vec![1; 1024]);
        let original = Arc::clone(&project.asset_data[&id]);
        let mut history = ProjectHistory::default();
        for _ in 0..MAX_HISTORY_DEPTH {
            history.push_snapshot(&project);
        }
        for snapshot in &history.undo_stack {
            assert!(Arc::ptr_eq(&original, &snapshot.asset_data[&id]));
        }
        project.asset_data.insert(id, Arc::new(vec![2; 1024]));
        let restored = history.undo(&project).unwrap();
        assert_eq!(restored.get_asset_data(id).unwrap(), &[1; 1024]);
        assert_eq!(project.get_asset_data(id).unwrap(), &[2; 1024]);
    }
}
