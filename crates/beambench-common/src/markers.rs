/// Zero-sized marker types for typed IDs.
/// Enables `Id<LayerMarker>` and `Id<ObjectMarker>` to be distinct types at compile time.
/// All markers derive `Hash` because `Id<T>` derives `Hash` and Rust 2024 enforces bounds
/// on phantom type parameters.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ProjectMarker;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LayerMarker;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CutEntryMarker;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ObjectMarker;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AssetMarker;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PlanMarker;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MachineProfileMarker;
