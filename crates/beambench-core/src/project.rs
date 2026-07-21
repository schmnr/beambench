use std::collections::HashMap;

use beambench_common::markers::ProjectMarker;
use beambench_common::{AnchorPoint, ColorTag, Id, StartFromMode, TransformLocks};
use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::asset::{Asset, AssetId};
use crate::layer::{
    CutEntry, CutEntryId, CutEntryPatch, CutEntryTemplate, Layer, LayerId, OperationType,
};
use crate::machine_profile::{MachineProfileId, MachineProfileSnapshot};
use crate::object::{ObjectData, ObjectId, ProjectObject};
use crate::optimization::ProjectOptimization;
use crate::workspace::Workspace;

/// Project file metadata.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectMetadata {
    pub format_version: String,
    pub app_version: String,
    pub project_id: Id<ProjectMarker>,
    pub project_name: String,
    pub created_at: String,
    pub modified_at: String,
}

impl ProjectMetadata {
    pub fn new(name: impl Into<String>) -> Self {
        let now = Utc::now().to_rfc3339();
        Self {
            format_version: "1.0".to_string(),
            app_version: env!("CARGO_PKG_VERSION").to_string(),
            project_id: Id::new(),
            project_name: name.into(),
            created_at: now.clone(),
            modified_at: now,
        }
    }
}

/// The root project container.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Project {
    pub metadata: ProjectMetadata,
    pub workspace: Workspace,
    pub layers: Vec<Layer>,
    pub objects: Vec<ProjectObject>,
    #[serde(default)]
    pub assets: Vec<Asset>,
    #[serde(default)]
    pub machine_profile_id: Option<MachineProfileId>,
    #[serde(default)]
    pub machine_profile_snapshot: Option<MachineProfileSnapshot>,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub start_from: StartFromMode,
    #[serde(default)]
    pub job_origin: AnchorPoint,
    #[serde(default)]
    pub transform_locks: TransformLocks,
    #[serde(default)]
    pub user_origin: Option<(f64, f64)>,
    #[serde(default)]
    pub optimization: ProjectOptimization,
    /// Material thickness used as the absolute-Z reference for Focus Test.
    #[serde(default)]
    pub material_height_mm: Option<f64>,
    #[serde(skip)]
    pub asset_data: HashMap<AssetId, Vec<u8>>,
    #[serde(skip)]
    pub dirty: bool,
}

impl Project {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            metadata: ProjectMetadata::new(name),
            workspace: Workspace::default(),
            layers: Vec::new(),
            objects: Vec::new(),
            assets: Vec::new(),
            machine_profile_id: None,
            machine_profile_snapshot: None,
            notes: String::new(),
            start_from: StartFromMode::default(),
            job_origin: AnchorPoint::default(),
            transform_locks: TransformLocks::default(),
            user_origin: None,
            optimization: ProjectOptimization::default(),
            material_height_mm: None,
            asset_data: HashMap::new(),
            dirty: false,
        }
    }

    /// Create a default black (C00) Line layer if no layers exist.
    /// Returns the layer id of the first layer.
    pub fn ensure_default_layer(&mut self) -> LayerId {
        if let Some(first) = self.layers.first() {
            return first.id;
        }
        let mut layer = Layer::new_single_entry("Line", OperationType::Line);
        layer.color_tag = ColorTag("#000000".to_string());
        let id = layer.id;
        self.add_layer(layer);
        id
    }

    // --- Layer CRUD ---

    pub fn find_layer(&self, id: LayerId) -> Option<&Layer> {
        self.layers.iter().find(|l| l.id == id)
    }

    pub fn find_layer_mut(&mut self, id: LayerId) -> Option<&mut Layer> {
        self.layers.iter_mut().find(|l| l.id == id)
    }

    pub fn add_layer(&mut self, mut layer: Layer) -> &Layer {
        layer.order_index = self.layers.len() as u32;
        self.layers.push(layer);
        self.dirty = true;
        self.layers.last().unwrap()
    }

    pub fn remove_layer(&mut self, id: LayerId) -> bool {
        let len_before = self.layers.len();
        self.layers.retain(|l| l.id != id);
        if self.layers.len() != len_before {
            let removed_ids: std::collections::HashSet<ObjectId> = self
                .objects
                .iter()
                .filter(|o| o.layer_id == id)
                .map(|o| o.id)
                .collect();
            self.objects.retain(|o| o.layer_id != id);
            self.prune_image_mask_refs(&removed_ids);
            self.reindex_layers();
            self.dirty = true;
            true
        } else {
            false
        }
    }

    pub fn reorder_layer(&mut self, id: LayerId, new_index: usize) -> bool {
        let Some(current_pos) = self.layers.iter().position(|l| l.id == id) else {
            return false;
        };
        let clamped = new_index.min(self.layers.len().saturating_sub(1));
        let layer = self.layers.remove(current_pos);
        self.layers.insert(clamped, layer);
        self.reindex_layers();
        self.dirty = true;
        true
    }

    fn reindex_layers(&mut self) {
        for (i, layer) in self.layers.iter_mut().enumerate() {
            layer.order_index = i as u32;
        }
    }

    /// Remove layers that have no objects assigned to them.
    pub fn clean_empty_layers(&mut self) {
        let layers_with_objects: std::collections::HashSet<LayerId> =
            self.objects.iter().map(|o| o.layer_id).collect();
        let before = self.layers.len();
        self.layers.retain(|l| layers_with_objects.contains(&l.id));
        if self.layers.len() != before {
            self.reindex_layers();
            self.dirty = true;
        }
    }

    /// M4: replace a layer's `entries[]` with a fresh stack built from clipboard templates.
    ///
    /// Returns the new entry list on success. Layer shell fields (id, name, color_tag, enabled,
    /// visible, order_index, is_tool_layer) are unchanged. Each new entry gets a freshly minted
    /// `CutEntryId` so the same clipboard can be pasted onto N layers without aliasing.
    pub fn replace_layer_entries(
        &mut self,
        layer_id: LayerId,
        templates: Vec<CutEntryTemplate>,
    ) -> Option<Vec<CutEntry>> {
        if templates.is_empty() {
            // A layer must always have at least one entry — refuse the empty case rather than
            // breaking the invariant via a clipboard that was somehow stripped to zero.
            return None;
        }
        let layer = self.find_layer_mut(layer_id)?;
        if layer.is_tool_layer {
            return Some(layer.entries.clone());
        }
        let new_entries: Vec<CutEntry> = templates.into_iter().map(|t| t.into_entry()).collect();
        layer.entries = new_entries.clone();
        self.dirty = true;
        Some(new_entries)
    }

    pub fn add_cut_entry(
        &mut self,
        layer_id: LayerId,
        after_entry_id: Option<CutEntryId>,
    ) -> Option<CutEntry> {
        let layer = self.find_layer_mut(layer_id)?;
        if layer.is_tool_layer {
            return None;
        }
        if layer.entries.is_empty() {
            return None;
        }

        let insert_at = match after_entry_id {
            Some(entry_id) => layer
                .entries
                .iter()
                .position(|entry| entry.id == entry_id)
                .map(|idx| idx + 1)?,
            None => layer.entries.len(),
        };

        let duplicate_from = insert_at.saturating_sub(1).min(layer.entries.len() - 1);
        let mut new_entry = layer.entries[duplicate_from].clone();
        new_entry.id = CutEntryId::new();
        layer.entries.insert(insert_at, new_entry.clone());
        self.dirty = true;
        Some(new_entry)
    }

    pub fn remove_cut_entry(&mut self, layer_id: LayerId, entry_id: CutEntryId) -> bool {
        let Some(layer) = self.find_layer_mut(layer_id) else {
            return false;
        };
        if layer.is_tool_layer {
            return false;
        }
        if layer.entries.len() <= 1 {
            return false;
        }
        let len_before = layer.entries.len();
        layer.entries.retain(|entry| entry.id != entry_id);
        if layer.entries.len() != len_before {
            self.dirty = true;
            true
        } else {
            false
        }
    }

    pub fn reorder_cut_entry(
        &mut self,
        layer_id: LayerId,
        entry_id: CutEntryId,
        new_index: usize,
    ) -> bool {
        let Some(layer) = self.find_layer_mut(layer_id) else {
            return false;
        };
        if layer.is_tool_layer {
            return false;
        }
        let Some(current_pos) = layer.entries.iter().position(|entry| entry.id == entry_id) else {
            return false;
        };
        let clamped = new_index.min(layer.entries.len().saturating_sub(1));
        let entry = layer.entries.remove(current_pos);
        layer.entries.insert(clamped, entry);
        self.dirty = true;
        true
    }

    pub fn update_cut_entry(
        &mut self,
        layer_id: LayerId,
        entry_id: CutEntryId,
        patch: &CutEntryPatch,
    ) -> Option<bool> {
        let layer = self.find_layer_mut(layer_id)?;
        if layer.is_tool_layer {
            return Some(false);
        }
        let entry = layer
            .entries
            .iter_mut()
            .find(|entry| entry.id == entry_id)?;
        let changed = entry.apply_patch(patch);
        if changed {
            self.dirty = true;
        }
        Some(changed)
    }

    // --- Object CRUD ---

    pub fn find_object(&self, id: ObjectId) -> Option<&ProjectObject> {
        self.objects.iter().find(|o| o.id == id)
    }

    pub fn find_object_mut(&mut self, id: ObjectId) -> Option<&mut ProjectObject> {
        self.objects.iter_mut().find(|o| o.id == id)
    }

    pub fn add_object(&mut self, obj: ProjectObject) -> &ProjectObject {
        self.objects.push(obj);
        self.dirty = true;
        self.objects.last().unwrap()
    }

    pub fn remove_object(&mut self, id: ObjectId) -> bool {
        let removed_ids = std::collections::HashSet::from([id]);
        // Before removing, auto-unlink any VirtualClones referencing this object
        let clone_ids: Vec<ObjectId> = self
            .objects
            .iter()
            .filter(
                |o| matches!(&o.data, ObjectData::VirtualClone { source_id } if *source_id == id),
            )
            .map(|o| o.id)
            .collect();
        for clone_id in clone_ids {
            let _ = self.resolve_clone_in_place(clone_id);
        }

        let len_before = self.objects.len();
        self.objects.retain(|o| o.id != id);
        if self.objects.len() != len_before {
            self.prune_image_mask_refs(&removed_ids);
            self.dirty = true;
            true
        } else {
            false
        }
    }

    pub fn remove_objects(&mut self, ids: &[ObjectId]) -> usize {
        let id_set: std::collections::HashSet<ObjectId> = ids.iter().copied().collect();
        // Auto-unlink VirtualClones referencing any of the objects being removed
        let clone_ids: Vec<ObjectId> = self
            .objects
            .iter()
            .filter(|o| {
                if let ObjectData::VirtualClone { source_id } = &o.data {
                    id_set.contains(source_id) && !id_set.contains(&o.id)
                } else {
                    false
                }
            })
            .map(|o| o.id)
            .collect();
        for clone_id in clone_ids {
            let _ = self.resolve_clone_in_place(clone_id);
        }
        let len_before = self.objects.len();
        self.objects.retain(|o| !id_set.contains(&o.id));
        let removed = len_before - self.objects.len();
        if removed > 0 {
            self.prune_image_mask_refs(&id_set);
            self.dirty = true;
        }
        removed
    }

    /// Remove non-destructive image mask references that point at deleted objects.
    pub fn prune_image_mask_refs(
        &mut self,
        removed_ids: &std::collections::HashSet<ObjectId>,
    ) -> usize {
        if removed_ids.is_empty() {
            return 0;
        }
        let mut pruned = 0;
        for object in &mut self.objects {
            if let ObjectData::RasterImage { masks, .. } = &mut object.data {
                let before = masks.len();
                masks.retain(|mask| !removed_ids.contains(&mask.object_id));
                pruned += before - masks.len();
            }
        }
        if pruned > 0 {
            self.dirty = true;
        }
        pruned
    }

    pub fn objects_in_layer(&self, layer_id: LayerId) -> Vec<&ProjectObject> {
        self.objects
            .iter()
            .filter(|o| o.layer_id == layer_id)
            .collect()
    }

    // --- VirtualClone Resolution ---

    /// Resolve a VirtualClone to a temporary concrete ProjectObject (read-only, no mutation).
    /// Recursively follows VirtualClone chains (depth limit 10).
    /// Returns `None` if obj is not a VirtualClone, source is missing, or depth exceeded.
    pub fn resolve_clone(&self, obj: &ProjectObject) -> Option<ProjectObject> {
        let mut current_source = obj;
        let mut depth = 0;
        while let ObjectData::VirtualClone { source_id } = &current_source.data {
            if depth > 10 {
                return None;
            }
            current_source = self.find_object(*source_id)?;
            depth += 1;
        }
        if depth == 0 {
            return None;
        }
        let mut resolved = obj.clone();
        resolved.data = current_source.data.clone();
        resolved.start_point_edits = current_source.start_point_edits.clone();
        Some(resolved)
    }

    /// Resolve a VirtualClone in-place — converts it to a real object (auto-unlink).
    /// No-op if the object is not a VirtualClone or not found.
    pub fn resolve_clone_in_place(&mut self, id: ObjectId) -> Result<(), String> {
        // First, resolve the data + start_point_edits by following the chain
        let (resolved_data, resolved_edits) = {
            let obj = self.find_object(id).ok_or("Object not found")?;
            let mut current_source = obj;
            let mut depth = 0;
            while let ObjectData::VirtualClone { source_id } = &current_source.data {
                if depth > 10 {
                    return Err("VirtualClone chain too deep".to_string());
                }
                current_source = self
                    .find_object(*source_id)
                    .ok_or("VirtualClone source not found")?;
                depth += 1;
            }
            if depth == 0 {
                return Ok(()); // Not a clone, no-op
            }
            (
                current_source.data.clone(),
                current_source.start_point_edits.clone(),
            )
        };
        // Now mutate
        if let Some(obj) = self.find_object_mut(id) {
            obj.data = resolved_data;
            obj.start_point_edits = resolved_edits;
            self.dirty = true;
        }
        Ok(())
    }

    /// If the object is a VirtualClone, resolve it to a concrete object in-place.
    /// No-op if the object is already concrete or not found.
    pub fn ensure_resolved(&mut self, id: ObjectId) -> Result<(), String> {
        let is_clone = self
            .find_object(id)
            .is_some_and(|o| matches!(o.data, ObjectData::VirtualClone { .. }));
        if is_clone {
            self.resolve_clone_in_place(id)?;
        }
        Ok(())
    }

    // --- Asset CRUD ---

    pub fn add_asset(&mut self, asset: Asset, data: Vec<u8>) -> &Asset {
        let id = asset.id;
        self.assets.push(asset);
        self.asset_data.insert(id, data);
        self.dirty = true;
        self.assets.last().unwrap()
    }

    pub fn find_asset(&self, id: AssetId) -> Option<&Asset> {
        self.assets.iter().find(|a| a.id == id)
    }

    pub fn remove_asset(&mut self, id: AssetId) -> bool {
        let len_before = self.assets.len();
        self.assets.retain(|a| a.id != id);
        if self.assets.len() != len_before {
            self.asset_data.remove(&id);
            self.dirty = true;
            true
        } else {
            false
        }
    }

    pub fn get_asset_data(&self, id: AssetId) -> Option<&[u8]> {
        self.asset_data.get(&id).map(|v| v.as_slice())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::asset::AssetMediaType;
    use crate::object::{ImageMaskPolarity, ImageMaskRef, ObjectData, ShapeKind};
    use crate::workspace::WorkspaceOrigin;
    use beambench_common::{Bounds, Point2D};

    #[test]
    fn new_project_starts_with_no_layers() {
        let project = Project::new("Test Project");
        assert!(project.layers.is_empty());
    }

    #[test]
    fn new_project_has_no_default_operations() {
        let project = Project::new("Test");
        assert!(project.layers.is_empty());
    }

    #[test]
    fn new_project_has_no_layers_for_colors() {
        let project = Project::new("Test");
        assert!(project.layers.is_empty());
    }

    #[test]
    fn new_project_has_no_layers_for_order_indices() {
        let project = Project::new("Test");
        assert!(project.layers.is_empty());
    }

    #[test]
    fn add_layer_increments_order() {
        let mut project = Project::new("Test");
        project.ensure_default_layer();
        let new_layer = Layer::new("Custom", OperationType::Line);
        project.add_layer(new_layer);
        assert_eq!(project.layers.len(), 2);
        assert_eq!(project.layers[1].order_index, 1);
        assert!(project.dirty);
    }

    #[test]
    fn remove_layer_reindexes() {
        let mut project = Project::new("Test");
        project.ensure_default_layer();
        project.add_layer(Layer::new("Second", OperationType::Fill));
        let id = project.layers[1].id;
        assert!(project.remove_layer(id));
        assert_eq!(project.layers.len(), 1);
        for (i, layer) in project.layers.iter().enumerate() {
            assert_eq!(layer.order_index, i as u32);
        }
    }

    #[test]
    fn remove_layer_also_removes_its_objects() {
        let mut project = Project::new("Test");
        let layer_id = project.ensure_default_layer();
        let second = Layer::new("Second", OperationType::Fill);
        let other_layer_id = second.id;
        project.add_layer(second);

        project.add_object(ProjectObject::new(
            "on_removed",
            layer_id,
            sample_bounds(),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 10.0,
                height: 10.0,
                corner_radius: 0.0,
            },
        ));
        project.add_object(ProjectObject::new(
            "on_kept",
            other_layer_id,
            sample_bounds(),
            ObjectData::Shape {
                kind: ShapeKind::Ellipse,
                width: 20.0,
                height: 20.0,
                corner_radius: 0.0,
            },
        ));

        assert_eq!(project.objects.len(), 2);
        project.remove_layer(layer_id);
        assert_eq!(project.objects.len(), 1);
        assert_eq!(project.objects[0].name, "on_kept");
    }

    #[test]
    fn remove_layer_prunes_image_mask_refs_to_layer_objects() {
        let mut project = Project::new("Test");
        let image_layer_id = project.ensure_default_layer();
        let mask_layer = Layer::new("Mask", OperationType::Line);
        let mask_layer_id = mask_layer.id;
        project.add_layer(mask_layer);

        let mask = ProjectObject::new(
            "mask",
            mask_layer_id,
            sample_bounds(),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 10.0,
                height: 10.0,
                corner_radius: 0.0,
            },
        );
        let mask_id = mask.id;
        let image = ProjectObject::new(
            "image",
            image_layer_id,
            sample_bounds(),
            ObjectData::RasterImage {
                asset_key: "asset".to_string(),
                original_width_px: 10,
                original_height_px: 10,
                adjustments: None,
                masks: vec![ImageMaskRef {
                    object_id: mask_id,
                    polarity: ImageMaskPolarity::KeepInside,
                }],
            },
        );
        let image_id = image.id;
        project.add_object(mask);
        project.add_object(image);

        assert!(project.remove_layer(mask_layer_id));
        let image = project.find_object(image_id).unwrap();
        match &image.data {
            ObjectData::RasterImage { masks, .. } => assert!(masks.is_empty()),
            _ => panic!("Expected RasterImage"),
        }
    }

    #[test]
    fn remove_nonexistent_layer_returns_false() {
        let mut project = Project::new("Test");
        assert!(!project.remove_layer(LayerId::new()));
    }

    #[test]
    fn reorder_layer_moves_correctly() {
        let mut project = Project::new("Test");
        let id = project.ensure_default_layer();
        project.add_layer(Layer::new("Second", OperationType::Fill));
        project.add_layer(Layer::new("Third", OperationType::Score));
        project.add_layer(Layer::new("Fourth", OperationType::Cut));
        project.reorder_layer(id, 3);
        assert_eq!(project.layers[3].id, id);
        for (i, layer) in project.layers.iter().enumerate() {
            assert_eq!(layer.order_index, i as u32);
        }
    }

    #[test]
    fn reorder_nonexistent_layer_returns_false() {
        let mut project = Project::new("Test");
        assert!(!project.reorder_layer(LayerId::new(), 0));
    }

    fn sample_bounds() -> Bounds {
        Bounds::new(Point2D::zero(), Point2D::new(50.0, 50.0))
    }

    #[test]
    fn add_and_find_object() {
        let mut project = Project::new("Test");
        let layer_id = project.ensure_default_layer();
        let obj = ProjectObject::new(
            "rect",
            layer_id,
            sample_bounds(),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 50.0,
                height: 50.0,
                corner_radius: 0.0,
            },
        );
        let obj_id = obj.id;
        project.add_object(obj);
        assert!(project.find_object(obj_id).is_some());
        assert!(project.dirty);
    }

    #[test]
    fn remove_object() {
        let mut project = Project::new("Test");
        let layer_id = project.ensure_default_layer();
        let obj = ProjectObject::new(
            "rect",
            layer_id,
            sample_bounds(),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 50.0,
                height: 50.0,
                corner_radius: 0.0,
            },
        );
        let obj_id = obj.id;
        project.add_object(obj);
        assert!(project.remove_object(obj_id));
        assert!(project.find_object(obj_id).is_none());
    }

    #[test]
    fn remove_object_prunes_image_mask_refs() {
        let mut project = Project::new("Test");
        let layer_id = project.ensure_default_layer();
        let mask = ProjectObject::new(
            "mask",
            layer_id,
            sample_bounds(),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 10.0,
                height: 10.0,
                corner_radius: 0.0,
            },
        );
        let mask_id = mask.id;
        let image = ProjectObject::new(
            "image",
            layer_id,
            sample_bounds(),
            ObjectData::RasterImage {
                asset_key: "asset".to_string(),
                original_width_px: 10,
                original_height_px: 10,
                adjustments: None,
                masks: vec![ImageMaskRef {
                    object_id: mask_id,
                    polarity: ImageMaskPolarity::KeepInside,
                }],
            },
        );
        let image_id = image.id;
        project.add_object(mask);
        project.add_object(image);

        assert!(project.remove_object(mask_id));
        let image = project.find_object(image_id).unwrap();
        match &image.data {
            ObjectData::RasterImage { masks, .. } => assert!(masks.is_empty()),
            _ => panic!("Expected RasterImage"),
        }
    }

    #[test]
    fn remove_objects_batch() {
        let mut project = Project::new("Test");
        let layer_id = project.ensure_default_layer();
        let obj1 = ProjectObject::new(
            "a",
            layer_id,
            sample_bounds(),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 10.0,
                height: 10.0,
                corner_radius: 0.0,
            },
        );
        let obj2 = ProjectObject::new(
            "b",
            layer_id,
            sample_bounds(),
            ObjectData::Shape {
                kind: ShapeKind::Ellipse,
                width: 20.0,
                height: 20.0,
                corner_radius: 0.0,
            },
        );
        let obj3 = ProjectObject::new(
            "c",
            layer_id,
            sample_bounds(),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 5.0,
                height: 5.0,
                corner_radius: 0.0,
            },
        );
        let id1 = obj1.id;
        let id2 = obj2.id;
        let id3 = obj3.id;
        project.add_object(obj1);
        project.add_object(obj2);
        project.add_object(obj3);
        project.dirty = false;

        let removed = project.remove_objects(&[id1, id3]);
        assert_eq!(removed, 2);
        assert_eq!(project.objects.len(), 1);
        assert_eq!(project.objects[0].id, id2);
        assert!(project.dirty);
    }

    #[test]
    fn remove_objects_empty_ids_no_change() {
        let mut project = Project::new("Test");
        let layer_id = project.ensure_default_layer();
        project.add_object(ProjectObject::new(
            "a",
            layer_id,
            sample_bounds(),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 10.0,
                height: 10.0,
                corner_radius: 0.0,
            },
        ));
        project.dirty = false;

        let removed = project.remove_objects(&[]);
        assert_eq!(removed, 0);
        assert!(!project.dirty);
    }

    #[test]
    fn objects_in_layer_filters_correctly() {
        let mut project = Project::new("Test");
        let layer_a = project.ensure_default_layer();
        let second = Layer::new("Second", OperationType::Fill);
        let layer_b = second.id;
        project.add_layer(second);

        project.add_object(ProjectObject::new(
            "obj_a",
            layer_a,
            sample_bounds(),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 10.0,
                height: 10.0,
                corner_radius: 0.0,
            },
        ));
        project.add_object(ProjectObject::new(
            "obj_b",
            layer_b,
            sample_bounds(),
            ObjectData::Shape {
                kind: ShapeKind::Ellipse,
                width: 20.0,
                height: 20.0,
                corner_radius: 0.0,
            },
        ));

        assert_eq!(project.objects_in_layer(layer_a).len(), 1);
        assert_eq!(project.objects_in_layer(layer_b).len(), 1);
    }

    #[test]
    fn project_roundtrips_through_json() {
        let project = Project::new("Roundtrip Test");
        let json = serde_json::to_string(&project).unwrap();
        let restored: Project = serde_json::from_str(&json).unwrap();
        // dirty is skipped in serde, so compare other fields
        assert_eq!(project.metadata, restored.metadata);
        assert_eq!(project.workspace, restored.workspace);
        assert_eq!(project.layers, restored.layers);
        assert_eq!(project.objects, restored.objects);
    }

    #[test]
    fn metadata_has_timestamps() {
        let meta = ProjectMetadata::new("Test");
        assert!(!meta.created_at.is_empty());
        assert!(!meta.modified_at.is_empty());
        assert_eq!(meta.format_version, "1.0");
    }

    #[test]
    fn clean_empty_layers_removes_orphans() {
        let mut project = Project::new("Test");
        let layer_a = project.ensure_default_layer();
        let layer_b_raw = Layer::new("Red", OperationType::Line);
        project.add_layer(layer_b_raw);

        // Add object only to layer_a
        project.add_object(ProjectObject::new(
            "rect",
            layer_a,
            sample_bounds(),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 10.0,
                height: 10.0,
                corner_radius: 0.0,
            },
        ));

        assert_eq!(project.layers.len(), 2);
        project.clean_empty_layers();
        // layer_b had no objects — should be removed
        assert_eq!(project.layers.len(), 1);
        assert_eq!(project.layers[0].id, layer_a);
        // order_index reindexed
        assert_eq!(project.layers[0].order_index, 0);
    }

    #[test]
    fn clean_empty_layers_keeps_populated() {
        let mut project = Project::new("Test");
        let layer_a = project.ensure_default_layer();
        let layer_b_raw = Layer::new("Red", OperationType::Line);
        let layer_b = layer_b_raw.id;
        project.add_layer(layer_b_raw);

        // Both layers have objects
        project.add_object(ProjectObject::new(
            "a",
            layer_a,
            sample_bounds(),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 10.0,
                height: 10.0,
                corner_radius: 0.0,
            },
        ));
        project.add_object(ProjectObject::new(
            "b",
            layer_b,
            sample_bounds(),
            ObjectData::Shape {
                kind: ShapeKind::Ellipse,
                width: 20.0,
                height: 20.0,
                corner_radius: 0.0,
            },
        ));

        project.dirty = false;
        project.clean_empty_layers();
        // Both layers have objects — nothing removed
        assert_eq!(project.layers.len(), 2);
        assert!(!project.dirty);
    }

    #[test]
    fn remove_object_then_clean_removes_empty_layer() {
        let mut project = Project::new("Test");
        let layer_a = project.ensure_default_layer();
        let layer_b_raw = Layer::new("Red", OperationType::Line);
        let layer_b = layer_b_raw.id;
        project.add_layer(layer_b_raw);

        let obj = ProjectObject::new(
            "rect",
            layer_b,
            sample_bounds(),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 10.0,
                height: 10.0,
                corner_radius: 0.0,
            },
        );
        let obj_id = obj.id;
        // layer_a has an object, layer_b has one object
        project.add_object(ProjectObject::new(
            "keep",
            layer_a,
            sample_bounds(),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 5.0,
                height: 5.0,
                corner_radius: 0.0,
            },
        ));
        project.add_object(obj);

        assert_eq!(project.layers.len(), 2);
        project.remove_object(obj_id);
        project.clean_empty_layers();
        // layer_b lost its only object — should be removed
        assert_eq!(project.layers.len(), 1);
        assert_eq!(project.layers[0].id, layer_a);
    }

    #[test]
    fn new_project_is_not_dirty() {
        let project = Project::new("Test");
        assert!(!project.dirty);
    }

    #[test]
    fn mutations_set_dirty() {
        let mut project = Project::new("Test");
        let layer = Layer::new("Extra", OperationType::Line);
        project.add_layer(layer);
        assert!(project.dirty);
    }

    // --- Asset tests ---

    #[test]
    fn add_and_find_asset() {
        let mut project = Project::new("Test");
        let asset = Asset::new("photo.png", AssetMediaType::Png, 1024, Some(800), Some(600));
        let asset_id = asset.id;
        let data = vec![0x89, 0x50, 0x4E, 0x47]; // PNG magic bytes
        project.add_asset(asset, data.clone());

        assert!(project.find_asset(asset_id).is_some());
        assert_eq!(
            project.find_asset(asset_id).unwrap().original_filename,
            "photo.png"
        );
        assert_eq!(project.get_asset_data(asset_id), Some(data.as_slice()));
        assert!(project.dirty);
    }

    #[test]
    fn remove_asset() {
        let mut project = Project::new("Test");
        let asset = Asset::new("photo.png", AssetMediaType::Png, 1024, None, None);
        let asset_id = asset.id;
        project.add_asset(asset, vec![1, 2, 3]);

        assert!(project.remove_asset(asset_id));
        assert!(project.find_asset(asset_id).is_none());
        assert!(project.get_asset_data(asset_id).is_none());
    }

    #[test]
    fn remove_nonexistent_asset_returns_false() {
        let mut project = Project::new("Test");
        assert!(!project.remove_asset(AssetId::new()));
    }

    #[test]
    fn assets_survive_json_roundtrip() {
        let mut project = Project::new("Test");
        let asset = Asset::new("img.jpg", AssetMediaType::Jpeg, 2048, Some(640), Some(480));
        project.add_asset(asset, vec![0xFF, 0xD8]);

        let json = serde_json::to_string(&project).unwrap();
        let restored: Project = serde_json::from_str(&json).unwrap();

        assert_eq!(restored.assets.len(), 1);
        assert_eq!(restored.assets[0].original_filename, "img.jpg");
        // asset_data is serde(skip), so it's empty after deserialization
        assert!(restored.asset_data.is_empty());
    }

    #[test]
    fn new_project_has_empty_assets() {
        let project = Project::new("Test");
        assert!(project.assets.is_empty());
        assert!(project.asset_data.is_empty());
    }

    // --- Machine profile binding tests ---

    #[test]
    fn new_project_has_no_machine_profile() {
        let project = Project::new("Test");
        assert!(project.machine_profile_id.is_none());
        assert!(project.machine_profile_snapshot.is_none());
    }

    #[test]
    fn machine_profile_snapshot_survives_json_roundtrip() {
        use crate::machine_profile::MachineProfile;

        let mut project = Project::new("Snapshot Test");
        let profile = MachineProfile {
            name: "My Laser".to_string(),
            bed_width_mm: 300.0,
            bed_height_mm: 400.0,
            max_speed_mm_min: 5000.0,
            ..Default::default()
        };
        project.machine_profile_id = Some(profile.id);
        project.machine_profile_snapshot = Some(profile.snapshot());

        let json = serde_json::to_string(&project).unwrap();
        let restored: Project = serde_json::from_str(&json).unwrap();

        assert_eq!(restored.machine_profile_id, Some(profile.id));
        let snap = restored.machine_profile_snapshot.unwrap();
        assert_eq!(snap.profile_name, "My Laser");
        assert_eq!(snap.bed_width_mm, 300.0);
        assert_eq!(snap.bed_height_mm, 400.0);
        assert_eq!(snap.max_speed_mm_min, 5000.0);
    }

    #[test]
    fn old_project_json_without_profile_fields_deserializes() {
        // Backward compat: old JSON without machine_profile_id/machine_profile_snapshot
        let json = r#"{
            "metadata": {
                "format_version": "1.0",
                "app_version": "0.1.0",
                "project_id": "00000000-0000-0000-0000-000000000001",
                "project_name": "Old Project",
                "created_at": "2024-01-01T00:00:00Z",
                "modified_at": "2024-01-01T00:00:00Z"
            },
            "workspace": {
                "bed_width_mm": 200.0,
                "bed_height_mm": 200.0,
                "origin": "top_left"
            },
            "layers": [],
            "objects": [],
            "assets": []
        }"#;
        let restored: Project = serde_json::from_str(json).unwrap();
        assert!(restored.machine_profile_id.is_none());
        assert!(restored.machine_profile_snapshot.is_none());
    }

    // --- Project field tests ---

    #[test]
    fn project_p1_fields_roundtrip() {
        let mut project = Project::new("Project Field Test");
        project.notes = "Some project notes".to_string();
        project.start_from = StartFromMode::AbsoluteCoords;
        project.job_origin = AnchorPoint::Center;
        project.transform_locks = TransformLocks {
            move_enabled: false,
            size_enabled: true,
            rotate_enabled: false,
            shear_enabled: true,
        };

        let json = serde_json::to_string(&project).unwrap();
        let restored: Project = serde_json::from_str(&json).unwrap();

        assert_eq!(restored.notes, "Some project notes");
        assert_eq!(restored.start_from, StartFromMode::AbsoluteCoords);
        assert_eq!(restored.job_origin, AnchorPoint::Center);
        assert!(!restored.transform_locks.move_enabled);
        assert!(restored.transform_locks.size_enabled);
        assert!(!restored.transform_locks.rotate_enabled);
        assert!(restored.transform_locks.shear_enabled);
    }

    #[test]
    fn old_project_without_p1_fields_deserializes() {
        // Backward compat: old JSON without notes/start_from/job_origin/transform_locks
        let json = r#"{
            "metadata": {
                "format_version": "1.0",
                "app_version": "0.1.0",
                "project_id": "00000000-0000-0000-0000-000000000001",
                "project_name": "Old Project",
                "created_at": "2024-01-01T00:00:00Z",
                "modified_at": "2024-01-01T00:00:00Z"
            },
            "workspace": {
                "bed_width_mm": 200.0,
                "bed_height_mm": 200.0,
                "origin": "top_left"
            },
            "layers": [],
            "objects": [],
            "assets": []
        }"#;
        let restored: Project = serde_json::from_str(json).unwrap();
        assert_eq!(restored.notes, "");
        assert_eq!(restored.start_from, StartFromMode::default());
        assert_eq!(restored.job_origin, AnchorPoint::default());
        assert_eq!(restored.transform_locks, TransformLocks::default());
    }

    #[test]
    fn new_project_p1_field_defaults() {
        let project = Project::new("Test");
        assert_eq!(project.workspace.origin, WorkspaceOrigin::BottomLeft);
        assert_eq!(project.notes, "");
        assert_eq!(project.start_from, StartFromMode::default());
        assert_eq!(project.job_origin, AnchorPoint::default());
        assert_eq!(project.transform_locks, TransformLocks::default());
    }

    #[test]
    fn project_p1_field_setters() {
        let mut project = Project::new("Test");
        project.notes = "Updated notes".to_string();
        project.start_from = StartFromMode::CurrentPosition;
        project.job_origin = AnchorPoint::BottomRight;
        project.transform_locks.move_enabled = false;

        assert_eq!(project.notes, "Updated notes");
        assert_eq!(project.start_from, StartFromMode::CurrentPosition);
        assert_eq!(project.job_origin, AnchorPoint::BottomRight);
        assert!(!project.transform_locks.move_enabled);
    }

    #[test]
    fn project_notes_field_persists() {
        let mut project = Project::new("Test");
        project.notes = "Important project notes\nWith multiple lines".to_string();

        let json = serde_json::to_string(&project).unwrap();
        let restored: Project = serde_json::from_str(&json).unwrap();

        assert_eq!(
            restored.notes,
            "Important project notes\nWith multiple lines"
        );
    }

    #[test]
    fn user_origin_roundtrip() {
        let mut project = Project::new("User Origin Test");
        project.user_origin = Some((42.5, 99.0));

        let json = serde_json::to_string(&project).unwrap();
        let restored: Project = serde_json::from_str(&json).unwrap();

        assert_eq!(restored.user_origin, Some((42.5, 99.0)));
    }

    #[test]
    fn user_origin_defaults_to_none() {
        let project = Project::new("Test");
        assert_eq!(project.user_origin, None);
    }

    #[test]
    fn old_project_without_user_origin_deserializes() {
        let json = r#"{
            "metadata": {
                "format_version": "1.0",
                "app_version": "0.1.0",
                "project_id": "00000000-0000-0000-0000-000000000001",
                "project_name": "Old Project",
                "created_at": "2024-01-01T00:00:00Z",
                "modified_at": "2024-01-01T00:00:00Z"
            },
            "workspace": {
                "bed_width_mm": 200.0,
                "bed_height_mm": 200.0,
                "origin": "top_left"
            },
            "layers": [],
            "objects": [],
            "assets": []
        }"#;
        let restored: Project = serde_json::from_str(json).unwrap();
        assert_eq!(restored.user_origin, None);
    }

    // ── VirtualClone resolution tests ──

    fn make_rect_obj(project: &mut Project) -> ObjectId {
        let layer_id = project.ensure_default_layer();
        let obj = ProjectObject::new(
            "rect",
            layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(10.0, 10.0)),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 10.0,
                height: 10.0,
                corner_radius: 0.0,
            },
        );
        let id = obj.id;
        project.add_object(obj);
        id
    }

    fn make_clone(project: &mut Project, source_id: ObjectId, offset_x: f64) -> ObjectId {
        let layer_id = project.layers[0].id;
        let mut clone = ProjectObject::new(
            "clone",
            layer_id,
            Bounds::new(
                Point2D::new(offset_x, 0.0),
                Point2D::new(offset_x + 10.0, 10.0),
            ),
            ObjectData::VirtualClone { source_id },
        );
        clone.transform = beambench_common::Transform2D::identity();
        let id = clone.id;
        project.add_object(clone);
        id
    }

    #[test]
    fn resolve_clone_returns_source_data() {
        let mut project = Project::new("test");
        let src_id = make_rect_obj(&mut project);
        let clone_id = make_clone(&mut project, src_id, 20.0);
        let clone_obj = project.find_object(clone_id).unwrap();
        let resolved = project.resolve_clone(clone_obj).unwrap();
        assert!(matches!(resolved.data, ObjectData::Shape { .. }));
        assert_eq!(resolved.id, clone_id); // Keeps clone's ID
        assert!((resolved.bounds.min.x - 20.0).abs() < 0.01); // Keeps clone's bounds
    }

    #[test]
    fn resolve_clone_recursive_chain() {
        let mut project = Project::new("test");
        let src_id = make_rect_obj(&mut project);
        let mid_id = make_clone(&mut project, src_id, 20.0);
        let end_id = make_clone(&mut project, mid_id, 40.0);
        let end_obj = project.find_object(end_id).unwrap();
        let resolved = project.resolve_clone(end_obj).unwrap();
        assert!(matches!(resolved.data, ObjectData::Shape { .. }));
        assert_eq!(resolved.id, end_id);
    }

    #[test]
    fn resolve_clone_depth_limit() {
        let mut project = Project::new("test");
        let first_id = make_rect_obj(&mut project);
        let mut prev_id = first_id;
        for i in 0..12 {
            prev_id = make_clone(&mut project, prev_id, (i + 1) as f64 * 20.0);
        }
        let deep_obj = project.find_object(prev_id).unwrap();
        // Chain is 12 deep (exceeds limit of 10)
        assert!(project.resolve_clone(deep_obj).is_none());
    }

    #[test]
    fn resolve_clone_in_place_converts_to_real() {
        let mut project = Project::new("test");
        let src_id = make_rect_obj(&mut project);
        let clone_id = make_clone(&mut project, src_id, 20.0);
        assert!(matches!(
            project.find_object(clone_id).unwrap().data,
            ObjectData::VirtualClone { .. }
        ));
        project.resolve_clone_in_place(clone_id).unwrap();
        assert!(matches!(
            project.find_object(clone_id).unwrap().data,
            ObjectData::Shape { .. }
        ));
    }

    #[test]
    fn ensure_resolved_noop_for_concrete_object() {
        let mut project = Project::new("test");
        let src_id = make_rect_obj(&mut project);
        project.ensure_resolved(src_id).unwrap();
        assert!(matches!(
            project.find_object(src_id).unwrap().data,
            ObjectData::Shape { .. }
        ));
    }

    #[test]
    fn ensure_resolved_converts_virtual_clone() {
        let mut project = Project::new("test");
        let src_id = make_rect_obj(&mut project);
        let clone_id = make_clone(&mut project, src_id, 20.0);
        project.ensure_resolved(clone_id).unwrap();
        assert!(matches!(
            project.find_object(clone_id).unwrap().data,
            ObjectData::Shape { .. }
        ));
    }

    #[test]
    fn remove_object_auto_unlinks_clones() {
        let mut project = Project::new("test");
        let src_id = make_rect_obj(&mut project);
        let clone_id = make_clone(&mut project, src_id, 20.0);
        // Removing the source should auto-unlink the clone
        project.remove_object(src_id);
        assert!(project.find_object(src_id).is_none());
        let clone_obj = project.find_object(clone_id).unwrap();
        // Clone should now be a concrete Shape, not a VirtualClone
        assert!(matches!(clone_obj.data, ObjectData::Shape { .. }));
    }

    #[test]
    fn remove_objects_batch_auto_unlinks_clones() {
        let mut project = Project::new("test");
        let src_id = make_rect_obj(&mut project);
        let clone_id = make_clone(&mut project, src_id, 20.0);
        let clone2_id = make_clone(&mut project, src_id, 40.0);
        // Batch-remove the source — both clones should be auto-unlinked
        project.remove_objects(&[src_id]);
        assert!(project.find_object(src_id).is_none());
        assert!(matches!(
            project.find_object(clone_id).unwrap().data,
            ObjectData::Shape { .. }
        ));
        assert!(matches!(
            project.find_object(clone2_id).unwrap().data,
            ObjectData::Shape { .. }
        ));
    }

    #[test]
    fn resolve_clone_returns_none_for_concrete() {
        let mut project = Project::new("test");
        let src_id = make_rect_obj(&mut project);
        let obj = project.find_object(src_id).unwrap();
        assert!(project.resolve_clone(obj).is_none());
    }
}
