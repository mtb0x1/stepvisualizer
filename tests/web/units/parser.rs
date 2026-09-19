use stepvisualizer::{
    common::{
        parser::{StepParser, StepSchema, convert_header},
        types::LengthUnit,
    },
    error::StepError,
};
use truck_stepio::r#in::{
    ruststep,
    ruststep::ast::{EntityInstance, Name, Parameter, Record},
};
use wasm_bindgen_test::*;

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
    assert_eq!(header.author.as_slice(), &["Author Name".to_string()]);
    assert_eq!(header.organization.as_slice(), &["Organization Name".to_string()]);
    assert_eq!(header.preprocessor_version, "Preprocessor 1.0");
    assert_eq!(header.originating_system, "Originating Sys");
    assert_eq!(header.authorization, "Auth");
    assert_eq!(header.file_schema, "CONFIG_CONTROL_DESIGN");
}

#[wasm_bindgen_test]
fn header_missing_required_fields() {
    let empty_records: Vec<Record> = Vec::new();
    let res = convert_header(&empty_records);
    assert!(matches!(res, Err(StepError::InvalidHeader(_))));
}

#[wasm_bindgen_test]
fn header_sanitization_omitted_fields() {
    // We test both first level omitted fields `$` and sub level omitted fields `($, 'foo', $)`
    let step = "ISO-10303-21;\n\
                HEADER;\n\
                FILE_DESCRIPTION(($, 'desc2', $), $);\n\
                FILE_NAME('test.step', $, ($, 'auth2', $), ($, 'org2'), $, $, $);\n\
                FILE_SCHEMA(($));\n\
                ENDSEC;\n\
                DATA;\n\
                ENDSEC;\n\
                END-ISO-10303-21;";

    let exchange = ruststep::parser::parse(step).expect("valid step parse");
    let header = convert_header(&exchange.header).expect("valid header conversion");

    assert_eq!(header.file_description, "; desc2; ");
    assert_eq!(header.implementation_level, "");
    assert_eq!(header.file_name, "test.step");
    assert_eq!(header.time_stamp, "");
    assert_eq!(header.author.as_slice(), &["".to_string(), "auth2".to_string(), "".to_string()]);
    assert_eq!(header.organization.as_slice(), &["".to_string(), "org2".to_string()]);
    assert_eq!(header.preprocessor_version, "");
    assert_eq!(header.originating_system, "");
    assert_eq!(header.authorization, "");
    assert_eq!(header.file_schema, "");
}

#[wasm_bindgen_test]
fn schema_detection_supported() {
    assert_eq!(StepSchema::parse("CONFIG_CONTROL_DESIGN"), Some(StepSchema::Ap203));
    assert_eq!(StepSchema::parse("ap203"), Some(StepSchema::Ap203));
    assert_eq!(StepSchema::parse("AUTOMOTIVE_DESIGN"), Some(StepSchema::Ap214));
    assert_eq!(StepSchema::parse("AP214"), Some(StepSchema::Ap214));
    assert_eq!(StepSchema::parse("EXPLICIT_DRAUGHTING"), Some(StepSchema::Ap201));
    assert_eq!(StepSchema::parse("AP201"), Some(StepSchema::Ap201));
    assert_eq!(StepSchema::parse("UNKNOWN_SCHEMA"), None);
}

#[wasm_bindgen_test]
fn probe_validate_step_buffer_various() {
    let text_203 = step_with_schema("CONFIG_CONTROL_DESIGN");
    assert_eq!(StepParser::probe_validate_buffer(&text_203), Ok(StepSchema::Ap203));

    let text_214 = step_with_schema("AUTOMOTIVE_DESIGN { 1 0 10303 214 1 1 1 1 }");
    assert_eq!(StepParser::probe_validate_buffer(&text_214), Ok(StepSchema::Ap214));

    let text_201 = step_with_schema("EXPLICIT_DRAUGHTING");
    assert_eq!(StepParser::probe_validate_buffer(&text_201), Ok(StepSchema::Ap201));

    // Unsupported schema early rejection
    let text_aim = step_with_schema("PLANT_SPATIAL_CONFIGURATION");
    match StepParser::probe_validate_buffer(&text_aim) {
        Err(StepError::UnsupportedSchema { schema }) => {
            assert_eq!(schema, "PLANT_SPATIAL_CONFIGURATION");
        }
        res => panic!("Expected UnsupportedSchema, got {:?}", res),
    }

    // Invalid file
    let invalid = "NOT A VALID STEP FILE";
    assert!(matches!(StepParser::probe_validate_buffer(invalid), Err(StepError::Parse(_))));
}

#[wasm_bindgen_test]
fn usable_sections_filtering() {
    let step_no_data = "ISO-10303-21;\n\
                        HEADER;\n\
                        FILE_DESCRIPTION(('Test'), '2;1');\n\
                        FILE_NAME('test.step', '2026-09-01', ('Author'), ('Org'), 'Prep', 'Sys', 'Auth');\n\
                        FILE_SCHEMA(('CONFIG_CONTROL_DESIGN'));\n\
                        ENDSEC;\n\
                        DATA;\n\
                        ENDSEC;\n\
                        END-ISO-10303-21;";

    let parsed_empty = ruststep::parser::parse(step_no_data).expect("parse");
    let parser = StepParser::from_exchange(parsed_empty);
    assert!(matches!(parser.all_usable_sections(), Err(StepError::EmptyDataSection)));

    let step_multi = "ISO-10303-21;\n\
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

    let parsed_multi = ruststep::parser::parse(step_multi).expect("parse");
    let parser = StepParser::from_exchange(parsed_multi);
    let usable = parser.all_usable_sections().expect("usable sections");
    assert_eq!(usable.len(), 1);
    assert_eq!(usable[0].entities.len(), 1);
}

#[wasm_bindgen_test]
fn units_parsing() {
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
    let mut parser = StepParser::from_exchange(parsed);
    let index = stepvisualizer::common::ExchangeIndex::build(&mut parser);
    assert_eq!(index.resolved_unit(), Some(LengthUnit::Millimetre));

    let step_inch = "ISO-10303-21;\n\
                     HEADER;\n\
                     FILE_DESCRIPTION(('Test'), '2;1');\n\
                     FILE_NAME('test.step', '2026-09-01', ('Author'), ('Org'), 'Prep', 'Sys', 'Auth');\n\
                     FILE_SCHEMA(('CONFIG_CONTROL_DESIGN'));\n\
                     ENDSEC;\n\
                     DATA;\n\
                     #10 = CONVERSION_BASED_UNIT('INCH', #11);\n\
                     ENDSEC;\n\
                     END-ISO-10303-21;";

    let parsed_inch = ruststep::parser::parse(step_inch).expect("parse");
    let mut parser_inch = StepParser::from_exchange(parsed_inch);
    let index_inch = stepvisualizer::common::ExchangeIndex::build(&mut parser_inch);
    assert_eq!(index_inch.resolved_unit(), Some(LengthUnit::Inch));
}

#[wasm_bindgen_test]
fn test_normalize_exchange_surface_curve_subtypes() {
    let step_text = "ISO-10303-21;\n\
                     HEADER;\n\
                     FILE_DESCRIPTION(('Test'), '2;1');\n\
                     FILE_NAME('test.stp', '2026-09-01', ('Author'), ('Org'), 'Prep', 'Sys', 'Auth');\n\
                     FILE_SCHEMA(('CONFIG_CONTROL_DESIGN'));\n\
                     ENDSEC;\n\
                     DATA;\n\
                     #1 = INTERSECTION_CURVE('int_curve', #10, (#20), .CURVE_3D.);\n\
                     #2 = BOUNDARY_CURVE('bnd_curve', #11, (#21), .CURVE_3D.);\n\
                     #3 = LINE('line', #12, #13);\n\
                     ENDSEC;\n\
                     END-ISO-10303-21;";

    let parsed = ruststep::parser::parse(step_text).expect("parse");
    let mut parser = StepParser::from_exchange(parsed);
    parser.normalize();
    let parsed = parser.into_exchange();
    let entities = &parsed.data[0].entities;
    if let EntityInstance::Simple { record, .. } = &entities[0] {
        assert_eq!(record.name, "SURFACE_CURVE");
    } else {
        panic!("Expected simple entity #1");
    }

    if let EntityInstance::Simple { record, .. } = &entities[1] {
        assert_eq!(record.name, "SURFACE_CURVE");
    } else {
        panic!("Expected simple entity #2");
    }

    if let EntityInstance::Simple { record, .. } = &entities[2] {
        assert_eq!(record.name, "LINE");
    } else {
        panic!("Expected simple entity #3");
    }
}

#[wasm_bindgen_test]
fn test_sanitize_axis2_placement_3d_collinear_x() {
    let step_text = "ISO-10303-21;\n\
                     HEADER;\n\
                     FILE_DESCRIPTION(('Test'), '2;1');\n\
                     FILE_NAME('test.stp', '2026-09-01', ('Author'), ('Org'), 'Prep', 'Sys', 'Auth');\n\
                     FILE_SCHEMA(('CONFIG_CONTROL_DESIGN'));\n\
                     ENDSEC;\n\
                     DATA;\n\
                     #10 = CARTESIAN_POINT('Loc', (0.0, 0.0, 0.0));\n\
                     #20 = DIRECTION('DirAlongX', (-1.0, 0.0, 0.0));\n\
                     #30 = AXIS2_PLACEMENT_3D('PlacementAlongX', #10, #20, $);\n\
                     #40 = DIRECTION('DirAlongY', (0.0, 1.0, 0.0));\n\
                     #50 = AXIS2_PLACEMENT_3D('PlacementAlongY', #10, #40, $);\n\
                     ENDSEC;\n\
                     END-ISO-10303-21;";

    let parsed = ruststep::parser::parse(step_text).expect("parse");
    let mut parser = StepParser::from_exchange(parsed);
    parser.normalize();
    let parsed = parser.into_exchange();
    let entities = &parsed.data[0].entities;

    // Entity #30 (PlacementAlongX) had axis collinear with (-1, 0, 0) and omitted ref_direction.
    // It should now have an explicit ref_direction pointing to a unit Z direction.
    let placement_x = entities
        .iter()
        .find(|e| match e {
            EntityInstance::Simple { id, .. } => *id == 30,
            _ => false,
        })
        .expect("entity #30 found");

    if let EntityInstance::Simple { record, .. } = placement_x {
        if let Parameter::List(ref params) = record.parameter {
            assert!(params.len() >= 4);
            match &params[3] {
                Parameter::Ref(Name::Entity(ref_id)) => {
                    let ref_dir_entity = entities
                        .iter()
                        .find(|e| match e {
                            EntityInstance::Simple { id, .. } => id == ref_id,
                            _ => false,
                        })
                        .expect("ref_dir entity found in section");
                    if let EntityInstance::Simple { record: ref_record, .. } = ref_dir_entity {
                        assert_eq!(ref_record.name, "DIRECTION");
                    }
                }
                other => {
                    panic!("Expected Parameter::Ref for sanitized ref_direction, got {other:?}")
                }
            }
        }
    }

    // Entity #50 (PlacementAlongY) had axis along Y, not collinear with X.
    // Its ref_direction was $ and should remain omitted ($).
    let placement_y = entities
        .iter()
        .find(|e| match e {
            EntityInstance::Simple { id, .. } => *id == 50,
            _ => false,
        })
        .expect("entity #50 found");

    if let EntityInstance::Simple { record, .. } = placement_y {
        if let Parameter::List(ref params) = record.parameter {
            assert!(params.len() == 3 || matches!(params[3], Parameter::NotProvided));
        }
    }
}
