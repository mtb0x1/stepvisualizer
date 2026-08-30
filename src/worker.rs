//! Off-thread STEP tessellation Worker using gloo-worker.
use gloo_worker::Worker;
use serde::{Deserialize, Serialize};

use crate::common::constants::compute_adaptive_tolerance;
use crate::common::parser::{all_usable_sections, build_initial_metadata};
use crate::common::render::{extract_render_parts, visible_bounds};
use crate::common::types::{FileId, StepModel};

#[derive(Serialize, Deserialize, Debug)]
pub struct TessellationRequest {
    pub file_name: String,
    pub text: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum TessellationResponse {
    Success {
        model: StepModel,
        file_id: FileId,
    },
    Error(String),
}

pub struct TessellationWorker;

impl Worker for TessellationWorker {
    type Input = TessellationRequest;
    type Output = TessellationResponse;
    type Message = ();

    fn create(_scope: &gloo_worker::WorkerScope<Self>) -> Self {
        Self
    }

    fn update(&mut self, _scope: &gloo_worker::WorkerScope<Self>, _msg: Self::Message) {}

    fn received(
        &mut self,
        scope: &gloo_worker::WorkerScope<Self>,
        msg: Self::Input,
        id: gloo_worker::HandlerId,
    ) {
        let parsed = match ruststep::parser::parse(&msg.text) {
            Ok(p) => p,
            Err(e) => {
                scope.respond(id, TessellationResponse::Error(format!("Parse error: {e}")));
                return;
            }
        };

        let sections = match all_usable_sections(&parsed) {
            Ok(s) => s,
            Err(e) => {
                scope.respond(id, TessellationResponse::Error(format!("Section error: {e}")));
                return;
            }
        };

        let step_tables: Vec<truck_stepio::r#in::Table> = sections
            .into_iter()
            .map(truck_stepio::r#in::Table::from_data_section)
            .collect();

        let (meta, file_id) =
            match build_initial_metadata(&msg.file_name, &parsed, &step_tables, &msg.text) {
                Ok(v) => v,
                Err(e) => {
                    scope.respond(id, TessellationResponse::Error(format!("Metadata error: {e}")));
                    return;
                }
            };

        let tolerance = compute_adaptive_tolerance(meta.bounding_box.as_ref());
        let renderable_parts = extract_render_parts(&step_tables, tolerance);
        let part_count = renderable_parts.len();

        let mut model = StepModel {
            id: file_id.clone(),
            metadata: meta,
            render_parts: renderable_parts,
            part_visibility: vec![true; part_count],
            visibility_generation: 0,
            cached_bounds: None,
        };
        model.metadata.vertex_count = model.total_vertices();
        model.metadata.triangle_count = model.total_triangles();
        if let Some(bbox) = visible_bounds(&model.render_parts, &model.part_visibility) {
            model.metadata.bounding_box = Some(bbox);
        }

        scope.respond(id, TessellationResponse::Success { model, file_id });
    }
}
