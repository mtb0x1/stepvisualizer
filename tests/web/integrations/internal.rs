use stepvisualizer::common::constants::compute_adaptive_tolerance;
use stepvisualizer::common::exchange_index::ExchangeIndex;
use stepvisualizer::common::parser::{all_usable_sections, build_initial_metadata};
use stepvisualizer::common::render::{extract_render_parts, visible_bounds};
use stepvisualizer::common::types::{FileId, LengthUnit, StepModel};
use stepvisualizer::common::{StepColorMap, StepNameMap};
use stepvisualizer::error::StepError;
use stepvisualizer::ruststep;
use stepvisualizer::truck_stepio;
use stepvisualizer::workspace::{build_step_model, parse_step_file_content};
use wasm_bindgen_test::*;

#[wasm_bindgen_test]
fn step_pipeline_e2e_real_model() {
    const STEP_TEXT: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/samples/io1-ca-214.stp"
    ));

    let mut parsed = ruststep::parser::parse(STEP_TEXT).expect("successful STEP AST parse");
    let usable_sections = all_usable_sections(&parsed).expect("usable sections present");
    assert!(!usable_sections.is_empty());

    let step_tables: Vec<truck_stepio::r#in::Table> = usable_sections
        .into_iter()
        .map(truck_stepio::r#in::Table::from_data_section)
        .collect();
    assert_eq!(step_tables.len(), 1);

    let index = ExchangeIndex::build(&mut parsed);
    let units = index.resolved_unit();
    let color_map = StepColorMap::from_index(&index);
    let name_map = StepNameMap::from_index(&index);
    drop(index);

    let (meta, file_id) =
        build_initial_metadata("io1-ca-214.stp", &parsed, &step_tables, STEP_TEXT, units)
            .expect("metadata successfully built");

    assert_eq!(meta.header.file_name, "_bcd/io1ca.stp");
    assert_eq!(meta.header.file_schema, "AUTOMOTIVE_DESIGN");
    assert_eq!(meta.units, Some(LengthUnit::Millimetre));
    assert!(meta.entity_count > 0);
    assert!(meta.bounding_box.is_some());
    assert_eq!(file_id.as_str().len(), 16);

    let tolerance = compute_adaptive_tolerance(meta.bounding_box.as_ref());
    let output = extract_render_parts(&step_tables, Some(&color_map), Some(&name_map), tolerance);
    assert!(!output.parts.is_empty());
    let render_parts = output.parts;

    let part_count = render_parts.len();
    let mut model = StepModel {
        id: file_id,
        metadata: meta,
        render_parts,
        part_visibility: vec![true; part_count],
        visibility_generation: 0,
        cached_bounds: None,
        audit: stepvisualizer::common::AuditMetadata::default(),
    };

    model.metadata.vertex_count = model.total_vertices();
    model.metadata.triangle_count = model.total_triangles();
    if let Some(bbox) = visible_bounds(&model.render_parts, &model.part_visibility) {
        model.metadata.bounding_box = Some(bbox);
    }

    assert!(model.total_vertices() > 0);
    assert!(model.total_triangles() > 0);
    assert!(model.calculate_total_surface_area() > 0.0);
    assert!(model.metadata.bounding_box.as_ref().unwrap().is_valid());
}

#[wasm_bindgen_test]
fn step_pipeline_e2e_nasty_cheese() {
    const STEP_TEXT: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/samples/nasty_cheese.stp"
    ));

    let mut parsed = ruststep::parser::parse(STEP_TEXT).expect("successful STEP AST parse");
    let index = ExchangeIndex::build(&mut parsed);
    let units = index.resolved_unit();
    drop(index);

    let usable_sections = all_usable_sections(&parsed).expect("usable sections present");
    assert_eq!(usable_sections.len(), 1);

    let step_tables: Vec<truck_stepio::r#in::Table> = usable_sections
        .into_iter()
        .map(truck_stepio::r#in::Table::from_data_section)
        .collect();
    assert_eq!(step_tables.len(), 1);

    let (meta, file_id) =
        build_initial_metadata("nasty_cheese.stp", &parsed, &step_tables, STEP_TEXT, units)
            .expect("metadata successfully built");

    assert_eq!(meta.header.file_name, "nasty_cheese");
    assert_eq!(meta.header.file_schema, "CONFIG_CONTROL_DESIGN");
    assert!(meta.entity_count > 0);
    assert!(meta.bounding_box.is_some());

    let tolerance = compute_adaptive_tolerance(meta.bounding_box.as_ref());
    let output = extract_render_parts(&step_tables, None, None, tolerance);
    assert!(
        !output.parts.is_empty(),
        "Expected render parts for nasty_cheese.stp"
    );

    let render_parts = output.parts;
    let mut model = StepModel {
        id: file_id,
        metadata: meta,
        part_visibility: vec![true; render_parts.len()],
        visibility_generation: 0,
        cached_bounds: None,
        render_parts,
        audit: stepvisualizer::common::AuditMetadata::default(),
    };

    model.metadata.vertex_count = model.total_vertices();
    model.metadata.triangle_count = model.total_triangles();
    if let Some(bbox) = visible_bounds(&model.render_parts, &model.part_visibility) {
        model.metadata.bounding_box = Some(bbox);
    }

    assert!(model.total_vertices() > 5000);
    assert!(model.total_triangles() > 5000);
    assert!(model.calculate_total_surface_area() > 0.0);
    assert!(model.metadata.bounding_box.as_ref().unwrap().is_valid());
}

#[wasm_bindgen_test]
fn workspace_processor_parse_and_model_build() {
    const STEP_FILE_NAME: &str = "io1-ca-214.stp";
    const STEP_TEXT: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/samples/io1-ca-214.stp"
    ));
    let (meta, id, tables, color_map, name_map) =
        parse_step_file_content(STEP_FILE_NAME, STEP_TEXT).expect("valid parse");
    assert_eq!(meta.header.file_name, "_bcd/io1ca.stp");
    assert_eq!(id.as_str().len(), 16);
    assert_eq!(tables.len(), 1);
    assert!(!color_map.is_empty());
    assert!(name_map.is_empty());

    let model_id = FileId::from_content("test_model");
    let model = build_step_model(model_id.clone(), meta, Vec::new());
    assert_eq!(model.id, model_id);
    assert_eq!(model.part_visibility.len(), 0);
    assert_eq!(model.metadata.vertex_count, 0);
    assert_eq!(model.metadata.triangle_count, 0);
}

#[wasm_bindgen_test]
fn workspace_parse_unsupported_and_invalid_fails_early() {
    const STEP_FILE_NAME: &str = "invalid.step";
    const STEP_TEXT: &str = "NOT A VALID STEP FILE";
    let res = parse_step_file_content(STEP_FILE_NAME, STEP_TEXT);
    assert!(res.is_err());

    const STEP_AIM: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/samples/fullroom_aim.stp"
    ));
    match parse_step_file_content("fullroom_aim.stp", STEP_AIM) {
        Err(StepError::UnsupportedSchema { schema }) => {
            assert_eq!(schema, "PLANT_SPATIAL_CONFIGURATION");
        }
        res => panic!("Expected UnsupportedSchema error, got {:?}", res),
    }

    const STEP_224: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/samples/ap224_997423743.stp"
    ));
    match parse_step_file_content("ap224_997423743.stp", STEP_224) {
        Err(StepError::UnsupportedSchema { schema }) => {
            assert_eq!(schema, "FEATURE_BASED_PROCESS_PLANNING");
        }
        res => panic!("Expected UnsupportedSchema error, got {:?}", res),
    }
}

#[wasm_bindgen_test]
fn step_pipeline_as1_ac_214_small() {
    const STEP_TEXT: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/samples/as1-ac-214_small.stp"
    ));

    let (meta, _id, step_tables, color_map, name_map) =
        parse_step_file_content("as1-ac-214_small.stp", STEP_TEXT)
            .expect("successful parse of as1-ac-214_small.stp");

    assert_eq!(meta.header.file_schema, "CONFIG_CONTROL_DESIGN");
    assert_eq!(step_tables.len(), 1);

    let tolerance = compute_adaptive_tolerance(meta.bounding_box.as_ref());
    let output = extract_render_parts(&step_tables, Some(&color_map), Some(&name_map), tolerance);

    assert_eq!(output.skipped_shells, 0, "No shells should be skipped: {:?}", output.warnings);
    assert_eq!(output.parts.len(), 3, "Expected all 3 shells to tessellate into renderable parts");
    for part in &output.parts {
        assert!(!part.vertices.is_empty());
        assert!(!part.indices.is_empty());
    }
}

