//! Structural sort-key passes over `ProjectObject` references.
//!
//! Unlike the other `optimize/*` modules, these passes run **before**
//! polylines are generated — at the point where each layer's object
//! list has already been filtered and priority-sorted. They shape the
//! sequence of objects that subsequent polyline passes will see.
//!
//! `apply_order_by_group` clusters objects
//! that share a group ancestor so the group's members emit together.
//! Precedence:
//!
//! - Layer ordering (structural, always the outermost key) **wins**
//!   over Group ordering. A group whose children span multiple
//!   layers has its members split across each layer's emission
//!   window. The pass only clusters members within the caller's
//!   provided layer slice.
//!
//! Ordering rule for clusters:
//!
//! - Intra-cluster: members keep their input order (which is already
//!   priority-sorted by the caller).
//! - Inter-cluster: clusters sort by the minimum `z_index` of their
//!   members. This preserves author-stack intent — the
//!   cluster whose "topmost" (lowest-z-index) member would have
//!   emitted first still emits first, but all of its siblings come
//!   with it rather than interleaving with other clusters.
//!
//! Ungrouped objects each become their own singleton cluster, so they
//! participate in the same z_index inter-cluster ordering as any real
//! group.

use std::collections::HashMap;

use beambench_core::object::{ObjectData, ObjectId, ProjectObject};

/// Cluster the already-filtered + priority-sorted `layer_objects` into
/// group-aware order, using `all_objects` to walk group membership.
///
/// Returns the objects in the new order. No-op when `enabled` is false
/// or there are fewer than two objects.
pub fn apply_order_by_group<'a>(
    layer_objects: Vec<&'a ProjectObject>,
    all_objects: &'a [ProjectObject],
    enabled: bool,
) -> Vec<&'a ProjectObject> {
    if !enabled || layer_objects.len() < 2 {
        return layer_objects;
    }

    let top_anc = build_top_ancestor_map(all_objects);

    // Each object's cluster key: the top-level ancestor's ObjectId, or
    // the object's own ObjectId when it has no group ancestor.
    let cluster_key =
        |obj: &ProjectObject| -> ObjectId { top_anc.get(&obj.id).copied().unwrap_or(obj.id) };

    // Preserve input order within each cluster, record first-seen cluster
    // order so same-z_index ties break deterministically.
    let mut clusters: HashMap<ObjectId, Vec<&'a ProjectObject>> = HashMap::new();
    let mut cluster_order: Vec<ObjectId> = Vec::new();
    for obj in &layer_objects {
        let key = cluster_key(obj);
        if !clusters.contains_key(&key) {
            cluster_order.push(key);
        }
        clusters.entry(key).or_default().push(*obj);
    }

    // Sort clusters by min(z_index) ascending; break ties by first-seen
    // order to keep the result deterministic.
    cluster_order.sort_by(|a, b| {
        let za = clusters[a].iter().map(|o| o.z_index).min().unwrap_or(0);
        let zb = clusters[b].iter().map(|o| o.z_index).min().unwrap_or(0);
        za.cmp(&zb)
    });

    let mut out: Vec<&'a ProjectObject> = Vec::with_capacity(layer_objects.len());
    for key in cluster_order {
        if let Some(members) = clusters.remove(&key) {
            out.extend(members);
        }
    }
    out
}

/// Walk every `ObjectData::Group` in the project and produce a map:
/// `child_id → outermost_group_id`. Objects that never appear as a
/// group child are absent from the map.
///
/// The function handles nested groups correctly: if group G contains
/// group H which contains object X, the map records X → G (outermost).
/// This matches the spec's "outer-first" ancestry rule.
pub(crate) fn build_top_ancestor_map(all_objects: &[ProjectObject]) -> HashMap<ObjectId, ObjectId> {
    // First pass: immediate parent for every child.
    let mut immediate: HashMap<ObjectId, ObjectId> = HashMap::new();
    for obj in all_objects {
        if let ObjectData::Group { children } = &obj.data {
            for &child in children {
                immediate.insert(child, obj.id);
            }
        }
    }

    // Second pass: walk up the chain for each entry.
    let mut top: HashMap<ObjectId, ObjectId> = HashMap::with_capacity(immediate.len());
    for (&child, &_parent) in &immediate {
        let mut cursor = child;
        // Bounded traversal — max depth = number of objects, so at worst
        // N steps before we exit (a cycle would be ill-formed input).
        let mut depth = 0;
        while let Some(&p) = immediate.get(&cursor) {
            cursor = p;
            depth += 1;
            if depth > all_objects.len() {
                break;
            }
        }
        top.insert(child, cursor);
    }
    top
}

#[cfg(test)]
mod tests {
    use super::*;
    use beambench_common::geometry::{Bounds, Point2D};
    use beambench_core::object::{LayerRef, ObjectData, ProjectObject, ShapeKind};

    fn layer() -> LayerRef {
        LayerRef::new()
    }

    fn rect(name: &str, layer_id: LayerRef, z: i32) -> ProjectObject {
        let mut obj = ProjectObject::new(
            name,
            layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(1.0, 1.0)),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 1.0,
                height: 1.0,
                corner_radius: 0.0,
            },
        );
        obj.z_index = z;
        obj
    }

    fn group(name: &str, layer_id: LayerRef, z: i32, children: Vec<ObjectId>) -> ProjectObject {
        let mut obj = ProjectObject::new(
            name,
            layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(1.0, 1.0)),
            ObjectData::Group { children },
        );
        obj.z_index = z;
        obj
    }

    fn names(v: &[&ProjectObject]) -> Vec<String> {
        v.iter().map(|o| o.name.clone()).collect()
    }

    #[test]
    fn disabled_is_noop() {
        let l = layer();
        let a = rect("a", l, 0);
        let b = rect("b", l, 1);
        let all = vec![a.clone(), b.clone()];
        let refs: Vec<&ProjectObject> = all.iter().collect();
        let out = apply_order_by_group(refs, &all, false);
        assert_eq!(names(&out), vec!["a", "b"]);
    }

    #[test]
    fn no_groups_preserves_input_order() {
        let l = layer();
        let a = rect("a", l, 5);
        let b = rect("b", l, 0);
        let c = rect("c", l, 10);
        let all = vec![a.clone(), b.clone(), c.clone()];
        let refs: Vec<&ProjectObject> = all.iter().collect();
        let out = apply_order_by_group(refs, &all, true);
        // Each object is its own cluster; clusters sorted by z_index
        // ascending → b (0), a (5), c (10).
        assert_eq!(names(&out), vec!["b", "a", "c"]);
    }

    #[test]
    fn group_members_cluster_together() {
        let l = layer();
        // Two rects grouped together; one ungrouped between them by
        // z_index that would otherwise split the group.
        let r1 = rect("r1", l, 0);
        let r2 = rect("r2", l, 10);
        let middle = rect("middle", l, 5);
        let g = group("G", l, 100, vec![r1.id, r2.id]);

        let all = vec![r1.clone(), r2.clone(), middle.clone(), g.clone()];
        // Layer objects (as filtered by the builder) include r1, r2, middle —
        // but NOT the Group object itself (it has no direct geometry).
        let refs: Vec<&ProjectObject> = vec![&all[0], &all[1], &all[2]];

        let out = apply_order_by_group(refs, &all, true);

        // r1 and r2 cluster as group G (min z_index = 0). middle is its
        // own singleton cluster (z_index 5). Cluster G's min is 0, so G
        // emits first (r1, r2), then middle.
        assert_eq!(names(&out), vec!["r1", "r2", "middle"]);
    }

    #[test]
    fn ungrouped_between_groups_respects_z_order() {
        let l = layer();
        let r1 = rect("r1", l, 10); // cluster G: min 10
        let r2 = rect("r2", l, 11);
        let middle = rect("middle", l, 5); // singleton: 5
        let g = group("G", l, 100, vec![r1.id, r2.id]);

        let all = vec![r1.clone(), r2.clone(), middle.clone(), g.clone()];
        let refs: Vec<&ProjectObject> = vec![&all[0], &all[1], &all[2]];
        let out = apply_order_by_group(refs, &all, true);
        // middle's singleton has min z_index 5, G's min is 10, so middle first.
        assert_eq!(names(&out), vec!["middle", "r1", "r2"]);
    }

    #[test]
    fn nested_group_uses_outermost_ancestor() {
        let l = layer();
        let inner_a = rect("inner_a", l, 1);
        let inner_b = rect("inner_b", l, 2);
        let outer_c = rect("outer_c", l, 3);
        let inner_group = group("inner", l, 50, vec![inner_a.id, inner_b.id]);
        let outer_group = group("outer", l, 60, vec![inner_group.id, outer_c.id]);

        let all = vec![
            inner_a.clone(),
            inner_b.clone(),
            outer_c.clone(),
            inner_group.clone(),
            outer_group.clone(),
        ];
        let refs: Vec<&ProjectObject> = vec![&all[0], &all[1], &all[2]];
        let out = apply_order_by_group(refs, &all, true);
        // All three rects share the outer_group as top ancestor → single cluster.
        // Intra-cluster order follows input order.
        assert_eq!(names(&out), vec!["inner_a", "inner_b", "outer_c"]);
    }

    #[test]
    fn caller_scoped_to_one_layer_splits_mixed_layer_groups() {
        // Regression test for the mixed-layer precedence rule: when
        // Layer ordering is also on (upstream), the builder calls this
        // function ONCE PER LAYER with only that layer's objects. A
        // mixed-layer group's members are therefore presented to us in
        // separate calls — we must still cluster them correctly within
        // each call.
        let l0 = layer();
        let l1 = layer();
        let r0 = rect("r0", l0, 1);
        let r1 = rect("r1", l1, 2);
        let g = group("G", l0, 100, vec![r0.id, r1.id]);
        let solo_l0 = rect("solo_l0", l0, 5);

        let all = vec![r0.clone(), r1.clone(), g.clone(), solo_l0.clone()];
        // First call: layer 0 scope.
        let l0_refs: Vec<&ProjectObject> = vec![&all[0], &all[3]];
        let out_l0 = apply_order_by_group(l0_refs, &all, true);
        // Cluster keys: r0 → G (top ancestor), solo_l0 → self.
        // G's min z_index in this slice is 1 (only r0 visible);
        // solo_l0's is 5. G first.
        assert_eq!(names(&out_l0), vec!["r0", "solo_l0"]);

        // Second call: layer 1 scope.
        let l1_refs: Vec<&ProjectObject> = vec![&all[1]];
        let out_l1 = apply_order_by_group(l1_refs, &all, true);
        assert_eq!(names(&out_l1), vec!["r1"]);
    }

    #[test]
    fn single_object_is_noop() {
        let l = layer();
        let a = rect("a", l, 0);
        let all = vec![a.clone()];
        let refs: Vec<&ProjectObject> = all.iter().collect();
        let out = apply_order_by_group(refs, &all, true);
        assert_eq!(names(&out), vec!["a"]);
    }

    #[test]
    fn min_z_index_tie_breaks_on_first_seen() {
        // Two clusters with identical min z_index: first-seen wins.
        let l = layer();
        let a = rect("a", l, 0);
        let b = rect("b", l, 0);
        let c = rect("c", l, 5);
        let ga = group("GA", l, 100, vec![a.id]);
        let gb = group("GB", l, 101, vec![b.id]);
        let all = vec![a.clone(), b.clone(), c.clone(), ga.clone(), gb.clone()];
        // layer_objects order: b, a, c → cluster first-seen order: GB, GA, singleton-c.
        let refs: Vec<&ProjectObject> = vec![&all[1], &all[0], &all[2]];
        let out = apply_order_by_group(refs, &all, true);
        // GB min = 0, GA min = 0 (tie; GB first-seen wins), c = 5.
        assert_eq!(names(&out), vec!["b", "a", "c"]);
    }
}
