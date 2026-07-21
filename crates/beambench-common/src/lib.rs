//! Shared types, errors, IDs, and geometry primitives used across all Beam Bench crates.

pub mod barcode;
pub mod camera;
pub mod color;
pub mod console;
pub mod controller_choice;
pub mod error;
pub mod event;
pub mod feedback;
pub mod geometry;
pub mod grbl_family;
pub mod id;
pub mod machine;
pub mod markers;
pub mod palette;
pub mod path;
pub mod raster_types;
pub mod types;

pub use barcode::{BarcodeOptions, BarcodeType, QrErrorCorrection};
pub use camera::{
    AlignmentPoint, AlignmentPointSet, CalibrationPoint, CalibrationPointSet,
    CalibrationSolveResult, CameraAgentState, CameraAlignment, CameraAlignmentSource,
    CameraArtifactInfo, CameraBackendKind, CameraCalibration, CameraDeviceInfo, CameraFrameHandle,
    CameraOverlayDisplayState, CameraOverlayRenderOptions, CameraOverlayRenderResult,
    CameraOverlayRenderView, CameraOverlayRuntimeState, CameraOverlayState, CameraOverlayStatus,
    SimilarityTransform,
};
pub use color::ColorTag;
pub use console::{ConsoleDirection, ConsoleEntry};
pub use controller_choice::{
    ControllerChoiceBlockReason, ControllerChoiceOutcome, ControllerChoiceResolution,
    ControllerChoiceSource, ControllerConnectionEndpoint, ControllerDriverId,
    ControllerIdentityBinding, ControllerMismatchDecision, ControllerOverrideInvalidationReason,
    ControllerOverrideScope, ControllerOverrideUpdate, ControllerSelection, DeviceFingerprint,
    DeviceFingerprintStrength, ExplicitControllerSelection, FingerprintBoundControllerOverride,
    PositiveControllerIdentity, ResolvedControllerChoice,
};
pub use error::{AppError, ErrorResponse};
pub use event::AppEvent;
pub use geometry::{Bounds, Point2D, Transform2D};
pub use grbl_family::{
    GrblFamilyDialect, GrblFamilyIdentity, GrblFamilyIdentityEvidence, GrblFamilyIdentityStatus,
};
pub use id::Id;
pub use machine::{
    ControllerEvidenceState, ControllerFamily, ControllerModel, ControllerProductTier,
    DeviceCapabilities, DeviceIdentity, DiscoveryCandidate, DiscoveryPhase, DiscoveryScanState,
    DiscoveryTcpTarget, DiscoveryUsbTarget, JobProgress, JobState, MachineConnectionTarget,
    MachinePosition, MachineRunState, MachineStatus, PortInfo, PreflightCheck, PreflightOutcome,
    PreflightReport, SessionState, TransportKind,
};
pub use markers::{
    AssetMarker, LayerMarker, MachineProfileMarker, ObjectMarker, PlanMarker, ProjectMarker,
};
pub use palette::{PALETTE_COLORS, PaletteColor, canonical_palette_color_tag, is_tool_color};
pub use path::{PathCommand, Polyline, SubPath, VecPath};
pub use raster_types::{RasterAdjustments, RasterMode};
pub use types::{AnchorPoint, StartFromMode, TransformLocks};
