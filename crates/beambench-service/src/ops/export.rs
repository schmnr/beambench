use std::path::PathBuf;

use beambench_core::ObjectId;

use crate::context::ServiceContext;
use crate::error::{ServiceError, ServiceResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    Svg,
    Dxf,
    Pdf,
    Eps,
    Ai,
}

#[derive(Debug, Clone)]
pub struct ExportDocumentInput {
    pub path: Option<String>,
    pub selection_only: bool,
    pub selected_ids: Vec<ObjectId>,
    pub format: ExportFormat,
}

#[derive(Debug, Clone)]
pub struct ExportDocumentOutput {
    pub path: Option<String>,
    pub format: ExportFormat,
    pub bytes: usize,
    pub content: ExportDocumentContent,
}

#[derive(Debug, Clone)]
pub enum ExportDocumentContent {
    Svg(String),
    Dxf(String),
    Pdf(Vec<u8>),
    Eps(String),
    Ai(String),
}

impl ExportFormat {
    pub fn as_str(self) -> &'static str {
        match self {
            ExportFormat::Svg => "svg",
            ExportFormat::Dxf => "dxf",
            ExportFormat::Pdf => "pdf",
            ExportFormat::Eps => "eps",
            ExportFormat::Ai => "ai",
        }
    }
}

pub fn export_document(
    ctx: &ServiceContext,
    input: ExportDocumentInput,
) -> ServiceResult<ExportDocumentOutput> {
    let project_guard = ctx
        .project
        .lock()
        .map_err(|e| ServiceError::internal(format!("Failed to lock project: {e}")))?;
    let project = project_guard
        .as_ref()
        .ok_or_else(|| ServiceError::not_found("No project open"))?;

    let selected_ids = &input.selected_ids;
    let content = match input.format {
        ExportFormat::Svg => ExportDocumentContent::Svg(beambench_core::export_svg(
            project,
            input.selection_only,
            selected_ids,
        )),
        ExportFormat::Dxf => ExportDocumentContent::Dxf(beambench_core::export_dxf(
            project,
            input.selection_only,
            selected_ids,
        )),
        ExportFormat::Pdf => ExportDocumentContent::Pdf(beambench_core::export_pdf(
            project,
            input.selection_only,
            selected_ids,
        )),
        ExportFormat::Eps => ExportDocumentContent::Eps(beambench_core::export_eps(
            project,
            input.selection_only,
            selected_ids,
        )),
        ExportFormat::Ai => ExportDocumentContent::Ai(beambench_core::export_ai(
            project,
            input.selection_only,
            selected_ids,
        )),
    };
    drop(project_guard);

    let bytes = match &content {
        ExportDocumentContent::Svg(c)
        | ExportDocumentContent::Dxf(c)
        | ExportDocumentContent::Eps(c)
        | ExportDocumentContent::Ai(c) => c.len(),
        ExportDocumentContent::Pdf(c) => c.len(),
    };

    let path = if let Some(path) = input.path {
        let path_buf = PathBuf::from(&path);
        match &content {
            ExportDocumentContent::Svg(c)
            | ExportDocumentContent::Dxf(c)
            | ExportDocumentContent::Eps(c)
            | ExportDocumentContent::Ai(c) => {
                std::fs::write(&path_buf, c).map_err(|e| {
                    ServiceError::persistence(format!("Failed to write export: {e}"))
                })?;
            }
            ExportDocumentContent::Pdf(c) => {
                std::fs::write(&path_buf, c).map_err(|e| {
                    ServiceError::persistence(format!("Failed to write export: {e}"))
                })?;
            }
        }
        Some(path)
    } else {
        None
    };

    Ok(ExportDocumentOutput {
        path,
        format: input.format,
        bytes,
        content,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use beambench_common::{Bounds, Point2D};
    use beambench_core::{ObjectData, Project, ProjectObject, ShapeKind};
    use tempfile::tempdir;

    fn test_context() -> ServiceContext {
        let ctx = ServiceContext::new();
        let mut project = Project::new("Export");
        let layer_id = project.ensure_default_layer();
        project.add_object(ProjectObject::new(
            "Rect",
            layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(20.0, 10.0)),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 20.0,
                height: 10.0,
                corner_radius: 0.0,
            },
        ));
        *ctx.project.lock().unwrap() = Some(project);
        ctx
    }

    #[test]
    fn export_returns_inline_content_without_path() {
        let ctx = test_context();
        let output = export_document(
            &ctx,
            ExportDocumentInput {
                path: None,
                selection_only: false,
                selected_ids: vec![],
                format: ExportFormat::Svg,
            },
        )
        .unwrap();

        assert_eq!(output.format, ExportFormat::Svg);
        match output.content {
            ExportDocumentContent::Svg(content) => assert!(content.contains("<svg")),
            _ => panic!("expected SVG content"),
        }
    }

    #[test]
    fn export_writes_requested_file() {
        let ctx = test_context();
        let dir = tempdir().unwrap();
        let out_path = dir.path().join("export.dxf");
        let output = export_document(
            &ctx,
            ExportDocumentInput {
                path: Some(out_path.to_string_lossy().to_string()),
                selection_only: false,
                selected_ids: vec![],
                format: ExportFormat::Dxf,
            },
        )
        .unwrap();

        assert_eq!(
            output.path.as_deref(),
            Some(out_path.to_string_lossy().as_ref())
        );
        assert!(out_path.exists());
    }

    #[test]
    fn export_eps_returns_inline_content() {
        let ctx = test_context();
        let output = export_document(
            &ctx,
            ExportDocumentInput {
                path: None,
                selection_only: false,
                selected_ids: vec![],
                format: ExportFormat::Eps,
            },
        )
        .unwrap();

        assert_eq!(output.format, ExportFormat::Eps);
        match output.content {
            ExportDocumentContent::Eps(content) => {
                assert!(content.contains("%!PS-Adobe-3.0 EPSF-3.0"));
                assert!(content.contains("moveto"));
            }
            _ => panic!("expected EPS content"),
        }
    }

    #[test]
    fn export_eps_empty_project() {
        let ctx = ServiceContext::new();
        *ctx.project.lock().unwrap() = Some(Project::new("Empty"));
        let output = export_document(
            &ctx,
            ExportDocumentInput {
                path: None,
                selection_only: false,
                selected_ids: vec![],
                format: ExportFormat::Eps,
            },
        )
        .unwrap();

        match output.content {
            ExportDocumentContent::Eps(content) => {
                assert!(content.contains("%!PS-Adobe-3.0 EPSF-3.0"));
                assert!(content.contains("%%EOF"));
                assert!(!content.contains("moveto"));
            }
            _ => panic!("expected EPS content"),
        }
    }

    #[test]
    fn export_eps_writes_file() {
        let ctx = test_context();
        let dir = tempdir().unwrap();
        let out_path = dir.path().join("export.eps");
        let output = export_document(
            &ctx,
            ExportDocumentInput {
                path: Some(out_path.to_string_lossy().to_string()),
                selection_only: false,
                selected_ids: vec![],
                format: ExportFormat::Eps,
            },
        )
        .unwrap();

        assert!(out_path.exists());
        assert!(output.bytes > 0);
    }

    #[test]
    fn export_ai_returns_inline_content() {
        let ctx = test_context();
        let output = export_document(
            &ctx,
            ExportDocumentInput {
                path: None,
                selection_only: false,
                selected_ids: vec![],
                format: ExportFormat::Ai,
            },
        )
        .unwrap();

        assert_eq!(output.format, ExportFormat::Ai);
        match output.content {
            ExportDocumentContent::Ai(content) => {
                assert!(content.contains("%%Creator: Adobe Illustrator"));
                assert!(content.contains("moveto"));
            }
            _ => panic!("expected AI content"),
        }
    }
}
