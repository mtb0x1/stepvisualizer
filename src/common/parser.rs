//! STEP header/metadata extraction on top of ruststep's AST.
use super::logger;
use crate::common::storage::hash_text_to_id;
use crate::common::utils::{param_as_enum, param_as_list, param_as_str};
use crate::error::StepVizError;
use crate::ruststep::ast::{DataSection, EntityInstance, Exchange, Record};
use crate::ruststep::header::{FileSchema, Header};
use crate::trace_span;
use serde::{Deserialize, Serialize};

use super::types::{BoundingBox, FileId, LengthUnit, Metadata, StepHeader};

/// Supported STEP schemas recognized by the visualizer and geometry pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum StepSchema {
    /// ISO 10303-201: Explicit Draughting (corresponding to `ruststep::ap201::explicit_draughting`).
    Ap201,
    /// ISO 10303-203: Configuration Controlled 3D Design (corresponding to `ruststep::ap203::config_control_design`).
    Ap203,
    /// ISO 10303-214: Core Data for Automotive Mechanical Design Processes.
    Ap214,
}

impl StepSchema {
    /// Matches exact STEP schema identifiers and standard ASN.1 object identifier prefixes.
    pub fn parse(identifier: &str) -> Option<Self> {
        let clean = identifier.trim().trim_matches('\'').trim_matches('"');
        let upper = clean.to_ascii_uppercase();

        if upper == "CONFIG_CONTROL_DESIGN"
            || upper.starts_with("CONFIG_CONTROL_DESIGN ")
            || upper == "AP203"
            || upper == "AP203_E2"
            || upper.starts_with("CONFIGURATION_CONTROL_3D_DESIGN")
        {
            Some(Self::Ap203)
        } else if upper == "AUTOMOTIVE_DESIGN"
            || upper.starts_with("AUTOMOTIVE_DESIGN ")
            || upper == "AP214"
        {
            Some(Self::Ap214)
        } else if upper == "EXPLICIT_DRAUGHTING"
            || upper.starts_with("EXPLICIT_DRAUGHTING ")
            || upper == "AP201"
        {
            Some(Self::Ap201)
        } else {
            None
        }
    }

    /// Primary standard schema name string for display or comparison.
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Ap201 => "AP201",
            Self::Ap203 => "AP203",
            Self::Ap214 => "AP214",
        }
    }
}

/// Validates that at least one schema listed in the STEP header [`FileSchema`] is supported.
pub fn validate_schema(file_schema: &FileSchema) -> Result<StepSchema, StepVizError> {
    for id in &file_schema.schema {
        if let Some(schema) = StepSchema::parse(id) {
            return Ok(schema);
        }
    }
    let raw = if file_schema.schema.is_empty() {
        "UNKNOWN".to_string()
    } else {
        file_schema.schema.join(", ")
    };
    Err(StepVizError::UnsupportedSchema { schema: raw })
}

/// Helper to convert a typed [`Header`] into a display-oriented [`StepHeader`].
pub fn convert_header_from_ast(header: &Header) -> StepHeader {
    StepHeader {
        file_description: header.file_description.description.join("; "),
        implementation_level: header.file_description.implementation_level.clone(),
        file_name: header.file_name.name.clone(),
        time_stamp: header.file_name.time_stamp.clone(),
        author: header.file_name.author.clone(),
        organization: header.file_name.organization.clone(),
        preprocessor_version: header.file_name.preprocessor_version.clone(),
        originating_system: header.file_name.originating_system.clone(),
        authorization: header.file_name.authorization.clone(),
        file_schema: header.file_schema.schema.join(", "),
    }
}

/// Convert the STEP header section into the display-oriented [`StepHeader`].
/// Fails when the records do not form a valid header.
pub fn convert_header(header_in: &[Record]) -> Result<StepHeader, StepVizError> {
    trace_span!("convert_header");
    if header_in.len() < 3 {
        return Err(StepVizError::InvalidHeader(
            "Header section must contain at least 3 records".to_string(),
        ));
    }
    let header_obj =
        Header::from_records(header_in).map_err(|e| StepVizError::InvalidHeader(e.to_string()))?;
    Ok(convert_header_from_ast(&header_obj))
}

/// Parse unit system (e.g. `LengthUnit::Millimetre`) from the exchange structure.
///
/// Units are declared as `(LENGTH_UNIT()NAMED_UNIT(*)SI_UNIT(.MILLI.,.METRE.))`
/// (a Complex entity) alongside sibling `PLANE_ANGLE_UNIT`/`SOLID_ANGLE_UNIT`
/// forms. We must prefer the length unit: a naive "first SI_UNIT wins" scan
/// returns the angle unit because it appears earlier in the file.
pub fn parse_units(exchange: &Exchange) -> Option<LengthUnit> {
    trace_span!("parse_units");
    let mut fallback: Option<LengthUnit> = None;
    for section in &exchange.data {
        for entity in &section.entities {
            match entity {
                EntityInstance::Simple { record, .. } => {
                    if fallback.is_none() {
                        fallback = unit_from_record(record);
                    }
                }
                EntityInstance::Complex { subsuper, .. } => {
                    let is_length = subsuper
                        .0
                        .iter()
                        .any(|r| r.name.eq_ignore_ascii_case("LENGTH_UNIT"));
                    if is_length {
                        if let Some(unit) = unit_from_subsuper(&subsuper.0) {
                            return Some(unit);
                        }
                    } else if fallback.is_none() {
                        fallback = unit_from_subsuper(&subsuper.0);
                    }
                }
            }
        }
    }
    fallback
}

/// Axis-aligned bounds over all `CARTESIAN_POINT`s in the tables. Placement
/// transforms are ignored, so this is an approximation — good enough for a
/// pre-load size preview. `None` when tables have no points.
pub fn compute_bounding_box(step_tables: &[truck_stepio::r#in::Table]) -> Option<BoundingBox> {
    trace_span!("compute_bounding_box");
    let mut bbox = BoundingBox::EMPTY;

    for step_table in step_tables {
        for value in step_table.cartesian_point.values() {
            let coords = &value.coordinates;
            if coords.len() >= 3 {
                bbox.expand_point(glam::DVec3::new(coords[0], coords[1], coords[2]));
            }
        }
    }
    bbox.is_valid().then_some(bbox)
}

fn unit_from_subsuper(records: &[Record]) -> Option<LengthUnit> {
    records.iter().find_map(unit_from_record)
}

fn unit_from_record(record: &Record) -> Option<LengthUnit> {
    if record.name.eq_ignore_ascii_case("SI_UNIT") {
        let params = param_as_list(&record.parameter)?;
        let unit = params.get(1).and_then(param_as_enum)?;
        let prefix = params.first().and_then(param_as_enum);
        return LengthUnit::from_si_spec(unit, prefix);
    }
    if record.name.eq_ignore_ascii_case("CONVERSION_BASED_UNIT") {
        let params = param_as_list(&record.parameter)?;
        let name = params.first().and_then(param_as_str)?;
        return LengthUnit::from_name(name);
    }
    None
}

/// Returns all data sections carrying usable STEP content, or a
/// domain error explaining why the file has none.
pub fn all_usable_sections(parsed: &Exchange) -> Result<Vec<&DataSection>, StepVizError> {
    let usable: Vec<&DataSection> = parsed
        .data
        .iter()
        .filter(|s| !s.entities.is_empty() || !s.meta.is_empty())
        .collect();
    if usable.is_empty() {
        Err(StepVizError::EmptyDataSection)
    } else {
        if parsed.data.len() > 1 {
            logger::warn(&format!(
                "STEP file contains {} DATA sections; processing all {} usable sections",
                parsed.data.len(),
                usable.len()
            ));
        }
        Ok(usable)
    }
}

/// Assembles the pre-tessellation metadata (header, entity count, bounding
/// box, units) for a parsed STEP file, together with its content-hash id.
/// The tessellated counts (vertices/triangles) are filled in later, once
/// the geometry pass has produced them.
pub fn build_initial_metadata(
    fallback_name: &str,
    parsed: &Exchange,
    step_tables: &[truck_stepio::r#in::Table],
    text: &str,
) -> Result<(Metadata, FileId), StepVizError> {
    trace_span!("build_initial_metadata");
    if parsed.header.len() < 3 {
        return Err(StepVizError::InvalidHeader(
            "Header section must contain at least 3 records".to_string(),
        ));
    }
    let header_obj = Header::from_records(&parsed.header)
        .map_err(|e| StepVizError::InvalidHeader(e.to_string()))?;
    validate_schema(&header_obj.file_schema)?;

    let entity_count: usize = parsed
        .data
        .iter()
        .map(|section| section.entities.len())
        .sum();
    let mut step_header = convert_header_from_ast(&header_obj);
    if step_header.file_name.is_empty() {
        step_header.file_name = fallback_name.to_string();
    }

    let meta = Metadata {
        header: step_header,
        entity_count,
        bounding_box: compute_bounding_box(step_tables),
        units: parse_units(parsed),
        vertex_count: 0,
        triangle_count: 0,
        volume: None,
        surface_area: None,
    };
    Ok((meta, hash_text_to_id(text)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ruststep;
    use wasm_bindgen_test::*;

    wasm_bindgen_test_configure!(run_in_browser);

    /// Verifies that valid STEP header AST records are properly converted into a StepHeader struct
    /// with all metadata strings, author/org vectors, and schema fields populated.
    #[wasm_bindgen_test]
    fn header_valid_records() {
        let step = "ISO-10303-21;\n\
                    HEADER;\n\
                    FILE_DESCRIPTION(('Test Description'), '2;1');\n\
                    FILE_NAME('test_model.step', '2026-09-01T12:00:00', ('Author Name'), ('Organization Name'), 'Preprocessor 1.0', 'Originating Sys', 'Auth');\n\
                    FILE_SCHEMA(('CONFIG_CONTROL_DESIGN'));\n\
                    ENDSEC;\n\
                    DATA;\n\
                    ENDSEC;\n\
                    END-ISO-10303-21;";

        let exchange = ruststep::parser::parse(step).expect("valid step parse");
        let header = convert_header(&exchange.header).expect("valid header conversion");

        assert_eq!(header.file_description, "Test Description");
        assert_eq!(header.implementation_level, "2;1");
        assert_eq!(header.file_name, "test_model.step");
        assert_eq!(header.time_stamp, "2026-09-01T12:00:00");
        assert_eq!(header.author, vec!["Author Name".to_string()]);
        assert_eq!(header.organization, vec!["Organization Name".to_string()]);
        assert_eq!(header.preprocessor_version, "Preprocessor 1.0");
        assert_eq!(header.originating_system, "Originating Sys");
        assert_eq!(header.authorization, "Auth");
        assert_eq!(header.file_schema, "CONFIG_CONTROL_DESIGN");
    }

    /// Verifies that an incomplete or empty slice of header records returns an InvalidHeader domain error.
    #[wasm_bindgen_test]
    fn header_missing_required_fields() {
        let empty_records: Vec<Record> = Vec::new();
        let res = convert_header(&empty_records);

        assert!(matches!(res, Err(StepVizError::InvalidHeader(_))));
    }

    fn step_with_schema(schema: &str) -> String {
        format!(
            "ISO-10303-21;\n\
             HEADER;\n\
             FILE_DESCRIPTION(('Test'), '2;1');\n\
             FILE_NAME('test.step', '2026-09-01', ('Author'), ('Org'), 'Prep', 'Sys', 'Auth');\n\
             FILE_SCHEMA(('{schema}'));\n\
             ENDSEC;\n\
             DATA;\n\
             ENDSEC;\n\
             END-ISO-10303-21;"
        )
    }

    /// Verifies that schemas specifying AP203 or CONFIG_CONTROL_DESIGN are accepted as supported.
    #[wasm_bindgen_test]
    fn schema_supported_ap203() {
        let text1 = step_with_schema("CONFIG_CONTROL_DESIGN");
        let parsed1 = ruststep::parser::parse(&text1).expect("parse");
        assert!(build_initial_metadata("test", &parsed1, &[], &text1).is_ok());

        let text2 = step_with_schema("AP203");
        let parsed2 = ruststep::parser::parse(&text2).expect("parse");
        assert!(build_initial_metadata("test", &parsed2, &[], &text2).is_ok());
    }

    /// Verifies that schemas specifying AP214 / AUTOMOTIVE_DESIGN are accepted as supported.
    #[wasm_bindgen_test]
    fn schema_supported_ap214() {
        let text = step_with_schema("AUTOMOTIVE_DESIGN");
        let parsed = ruststep::parser::parse(&text).expect("parse");
        assert!(build_initial_metadata("test", &parsed, &[], &text).is_ok());
    }

    /// Verifies that schemas specifying AP201 are accepted as supported.
    #[wasm_bindgen_test]
    fn schema_supported_ap201() {
        let text = step_with_schema("AP201");
        let parsed = ruststep::parser::parse(&text).expect("parse");
        assert!(build_initial_metadata("test", &parsed, &[], &text).is_ok());
    }

    /// Verifies that unsupported AP209 / STRUCTURAL_ANALYSIS_DESIGN schemas return an UnsupportedSchema error.
    #[wasm_bindgen_test]
    fn schema_unsupported_ap209() {
        let text = step_with_schema("STRUCTURAL_ANALYSIS_DESIGN");
        let parsed = ruststep::parser::parse(&text).expect("parse");
        let res = build_initial_metadata("test", &parsed, &[], &text);

        match res {
            Err(StepVizError::UnsupportedSchema { schema }) => {
                assert_eq!(schema, "STRUCTURAL_ANALYSIS_DESIGN");
            }
            _ => panic!("Expected UnsupportedSchema error, got {:?}", res),
        }
    }

    /// Verifies that unsupported AP224 FEATURE_BASED_PROCESS_PLANNING schema returns an UnsupportedSchema error.
    #[wasm_bindgen_test]
    fn schema_unsupported_ap224() {
        let text = step_with_schema("FEATURE_BASED_PROCESS_PLANNING");
        let parsed = ruststep::parser::parse(&text).expect("parse");
        let res = build_initial_metadata("test", &parsed, &[], &text);

        match res {
            Err(StepVizError::UnsupportedSchema { schema }) => {
                assert_eq!(schema, "FEATURE_BASED_PROCESS_PLANNING");
            }
            _ => panic!("Expected UnsupportedSchema error, got {:?}", res),
        }
    }

    /// Verifies that schema validation is case-insensitive (e.g. lowercase config_control_design is accepted).
    #[wasm_bindgen_test]
    fn schema_case_insensitivity() {
        let text = step_with_schema("config_control_design");
        let parsed = ruststep::parser::parse(&text).expect("parse");
        assert!(build_initial_metadata("test", &parsed, &[], &text).is_ok());
    }

    /// Verifies that EXPLICIT_DRAUGHTING (the standard ISO 10303-201 schema name) is accepted.
    #[wasm_bindgen_test]
    fn schema_supported_ap201_explicit_draughting() {
        let text = step_with_schema("EXPLICIT_DRAUGHTING");
        let parsed = ruststep::parser::parse(&text).expect("parse");
        assert!(build_initial_metadata("test", &parsed, &[], &text).is_ok());
    }

    /// Verifies that schemas with ASN.1 object identifiers are properly matched.
    #[wasm_bindgen_test]
    fn schema_supported_with_asn1_parameters() {
        let text_214 = step_with_schema("AUTOMOTIVE_DESIGN { 1 0 10303 214 1 1 1 1 }");
        let parsed_214 = ruststep::parser::parse(&text_214).expect("parse");
        assert!(build_initial_metadata("test", &parsed_214, &[], &text_214).is_ok());

        let text_203 = step_with_schema("CONFIG_CONTROL_DESIGN { 1 0 10303 203 1 }");
        let parsed_203 = ruststep::parser::parse(&text_203).expect("parse");
        assert!(build_initial_metadata("test", &parsed_203, &[], &text_203).is_ok());
    }

    /// Direct unit tests for the StepSchema enum parser and validate_schema function.
    #[wasm_bindgen_test]
    fn test_step_schema_enum() {
        assert_eq!(StepSchema::parse("CONFIG_CONTROL_DESIGN"), Some(StepSchema::Ap203));
        assert_eq!(StepSchema::parse("ap203"), Some(StepSchema::Ap203));
        assert_eq!(StepSchema::parse("AUTOMOTIVE_DESIGN"), Some(StepSchema::Ap214));
        assert_eq!(StepSchema::parse("AP214"), Some(StepSchema::Ap214));
        assert_eq!(StepSchema::parse("EXPLICIT_DRAUGHTING"), Some(StepSchema::Ap201));
        assert_eq!(StepSchema::parse("AP201"), Some(StepSchema::Ap201));
        assert_eq!(StepSchema::parse("UNKNOWN_SCHEMA"), None);

        let valid_file_schema = FileSchema {
            schema: vec!["CONFIG_CONTROL_DESIGN".to_string()],
        };
        assert_eq!(validate_schema(&valid_file_schema), Ok(StepSchema::Ap203));

        let invalid_file_schema = FileSchema {
            schema: vec!["UNKNOWN_SCHEMA".to_string()],
        };
        assert!(validate_schema(&invalid_file_schema).is_err());
    }

    /// Verifies that an exchange structure containing only empty data sections returns an EmptyDataSection error.
    #[wasm_bindgen_test]
    fn usable_sections_empty_data() {
        let step_no_data = "ISO-10303-21;\n\
                            HEADER;\n\
                            FILE_DESCRIPTION(('Test'), '2;1');\n\
                            FILE_NAME('test.step', '2026-09-01', ('Author'), ('Org'), 'Prep', 'Sys', 'Auth');\n\
                            FILE_SCHEMA(('CONFIG_CONTROL_DESIGN'));\n\
                            ENDSEC;\n\
                            DATA;\n\
                            ENDSEC;\n\
                            END-ISO-10303-21;";

        let parsed = ruststep::parser::parse(step_no_data).expect("parse");
        let res = all_usable_sections(&parsed);
        assert!(matches!(res, Err(StepVizError::EmptyDataSection)));
    }

    /// Verifies that all_usable_sections filters out empty sections and retains sections containing entities.
    #[wasm_bindgen_test]
    fn usable_sections_filters_empty_sections() {
        let step_multi_data = "ISO-10303-21;\n\
                               HEADER;\n\
                               FILE_DESCRIPTION(('Test'), '2;1');\n\
                               FILE_NAME('test.step', '2026-09-01', ('Author'), ('Org'), 'Prep', 'Sys', 'Auth');\n\
                               FILE_SCHEMA(('CONFIG_CONTROL_DESIGN'));\n\
                               ENDSEC;\n\
                               DATA;\n\
                               ENDSEC;\n\
                               DATA;\n\
                               #1 = CARTESIAN_POINT('', (0.0, 0.0, 0.0));\n\
                               ENDSEC;\n\
                               END-ISO-10303-21;";

        let parsed = ruststep::parser::parse(step_multi_data).expect("parse");
        let usable = all_usable_sections(&parsed).expect("usable sections");
        assert_eq!(usable.len(), 1);
        assert_eq!(usable[0].entities.len(), 1);
    }

    /// Verifies that parse_units prioritizes LENGTH_UNIT over preceding PLANE_ANGLE_UNIT or SOLID_ANGLE_UNIT declarations.
    #[wasm_bindgen_test]
    fn units_prefers_length_over_plane_angle() {
        let step_text = "ISO-10303-21;\n\
                         HEADER;\n\
                         FILE_DESCRIPTION(('Test'), '2;1');\n\
                         FILE_NAME('test.step', '2026-09-01', ('Author'), ('Org'), 'Prep', 'Sys', 'Auth');\n\
                         FILE_SCHEMA(('CONFIG_CONTROL_DESIGN'));\n\
                         ENDSEC;\n\
                         DATA;\n\
                         #1 = ( NAMED_UNIT(*) PLANE_ANGLE_UNIT() SI_UNIT($, .RADIAN.) );\n\
                         #2 = ( LENGTH_UNIT() NAMED_UNIT(*) SI_UNIT(.MILLI., .METRE.) );\n\
                         ENDSEC;\n\
                         END-ISO-10303-21;";

        let parsed = ruststep::parser::parse(step_text).expect("parse");
        let unit = parse_units(&parsed);
        assert_eq!(unit, Some(LengthUnit::Millimetre));
    }

    /// Verifies that parse_units extracts conversion-based unit declarations (e.g. INCH).
    #[wasm_bindgen_test]
    fn units_conversion_based_unit() {
        let step_text = "ISO-10303-21;\n\
                         HEADER;\n\
                         FILE_DESCRIPTION(('Test'), '2;1');\n\
                         FILE_NAME('test.step', '2026-09-01', ('Author'), ('Org'), 'Prep', 'Sys', 'Auth');\n\
                         FILE_SCHEMA(('CONFIG_CONTROL_DESIGN'));\n\
                         ENDSEC;\n\
                         DATA;\n\
                         #10 = CONVERSION_BASED_UNIT('INCH', #11);\n\
                         ENDSEC;\n\
                         END-ISO-10303-21;";

        let parsed = ruststep::parser::parse(step_text).expect("parse");
        let unit = parse_units(&parsed);
        assert_eq!(unit, Some(LengthUnit::Inch));
    }

    /// Verifies that parse_units returns None when the file lacks any unit entity declarations.
    #[wasm_bindgen_test]
    fn units_absent_fallback() {
        let step_text = "ISO-10303-21;\n\
                         HEADER;\n\
                         FILE_DESCRIPTION(('Test'), '2;1');\n\
                         FILE_NAME('test.step', '2026-09-01', ('Author'), ('Org'), 'Prep', 'Sys', 'Auth');\n\
                         FILE_SCHEMA(('CONFIG_CONTROL_DESIGN'));\n\
                         ENDSEC;\n\
                         DATA;\n\
                         #1 = CARTESIAN_POINT('', (0.0, 0.0, 0.0));\n\
                         ENDSEC;\n\
                         END-ISO-10303-21;";

        let parsed = ruststep::parser::parse(step_text).expect("parse");
        let unit = parse_units(&parsed);
        assert_eq!(unit, None);
    }

    /// End-to-end integration test reading, parsing, validating metadata, and tessellating a real AP214 STEP model.
    #[wasm_bindgen_test]
    fn step_pipeline_e2e_real_model() {
        use crate::common::constants::compute_adaptive_tolerance;
        use crate::common::render::{extract_render_parts, visible_bounds};
        use crate::common::types::StepModel;

        let stp_data = include_str!("../../examples/io1-ca-214.stp");

        // 1. Full STEP text parse
        let parsed = ruststep::parser::parse(stp_data).expect("successful STEP AST parse");

        // 2. Data section filtering
        let usable_sections = all_usable_sections(&parsed).expect("usable sections present");
        assert!(!usable_sections.is_empty());

        // 3. Step table conversion
        let step_tables: Vec<truck_stepio::r#in::Table> = usable_sections
            .into_iter()
            .map(truck_stepio::r#in::Table::from_data_section)
            .collect();
        assert_eq!(step_tables.len(), 1);

        // 4. Initial metadata build & schema validation
        let (meta, file_id) =
            build_initial_metadata("io1-ca-214.stp", &parsed, &step_tables, stp_data)
                .expect("metadata successfully built");

        assert_eq!(meta.header.file_name, "_bcd/io1ca.stp");
        assert_eq!(meta.header.file_schema, "AUTOMOTIVE_DESIGN");
        assert_eq!(meta.units, Some(LengthUnit::Millimetre));
        assert!(meta.entity_count > 0);
        assert!(meta.bounding_box.is_some());
        assert_eq!(file_id.as_str().len(), 16);

        // 5. Tessellation & Part Extraction
        let tolerance = compute_adaptive_tolerance(meta.bounding_box.as_ref());
        let (render_parts, _skipped) = extract_render_parts(&step_tables, tolerance);
        assert!(!render_parts.is_empty());

        // 6. Complete Model Assembly
        let part_count = render_parts.len();
        let mut model = StepModel {
            id: file_id.clone(),
            metadata: meta,
            render_parts,
            part_visibility: vec![true; part_count],
            visibility_generation: 0,
            cached_bounds: None,
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
}
