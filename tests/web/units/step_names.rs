use stepvisualizer::common::exchange_index::ExchangeIndex;
use stepvisualizer::common::step_names::{StepNameMap, clean_part_name, is_valid_part_name};
use stepvisualizer::ruststep;
use wasm_bindgen_test::*;

#[wasm_bindgen_test]
fn test_is_valid_and_clean_part_name() {
    assert!(is_valid_part_name("Housing"));
    assert!(is_valid_part_name("'Pin 1'"));
    assert!(is_valid_part_name("\"PartBody\""));
    assert!(is_valid_part_name("l-bracket_1"));

    // Placeholders and invalid formats
    assert!(!is_valid_part_name(""));
    assert!(!is_valid_part_name("   "));
    assert!(!is_valid_part_name("''"));
    assert!(!is_valid_part_name("NONE"));
    assert!(!is_valid_part_name("'None'"));
    assert!(!is_valid_part_name("unspecified"));
    assert!(!is_valid_part_name("#602"));
    assert!(!is_valid_part_name("$"));

    assert_eq!(clean_part_name("'Housing'"), "Housing");
    assert_eq!(clean_part_name("  \"Pin 1\"  "), "Pin 1");
    assert_eq!(clean_part_name("'SHAPE FOR FW_EXP_CLIP.'"), "FW_EXP_CLIP");
}

#[wasm_bindgen_test]
fn test_step_name_map_synthetic_and_nauo() {
    const SYNTHETIC_STEP: &str = "ISO-10303-21;\n\
                                  HEADER;\n\
                                  FILE_DESCRIPTION(('Test'), '2;1');\n\
                                  FILE_NAME('test.stp', '2026-09-01', ('Author'), ('Org'), 'Prep', 'Sys', 'Auth');\n\
                                  FILE_SCHEMA(('AUTOMOTIVE_DESIGN'));\n\
                                  ENDSEC;\n\
                                  DATA;\n\
                                  #10 = CLOSED_SHELL('shell_fallback', (#1));\n\
                                  #20 = MANIFOLD_SOLID_BREP('Bracket_Body', #10);\n\
                                  #30 = ADVANCED_BREP_SHAPE_REPRESENTATION('rep_name', (#20), #5);\n\
                                  #40 = SHAPE_DEFINITION_REPRESENTATION(#50, #30);\n\
                                  #50 = PRODUCT_DEFINITION_SHAPE('', '', #60);\n\
                                  #60 = PRODUCT_DEFINITION('design', '', #70, #8);\n\
                                  #70 = PRODUCT_DEFINITION_FORMATION('1', '', #80);\n\
                                  #80 = PRODUCT('P1', 'Bracket_Product', '', (#9));\n\
                                  ENDSEC;\n\
                                  END-ISO-10303-21;";

    let parsed = ruststep::parser::parse(SYNTHETIC_STEP).expect("parsed exchange");
    let mut ex = parsed.clone();
    let index = ExchangeIndex::build(&mut ex);
    let map = StepNameMap::from_index(&index);
    assert_eq!(map.len(), 1);
    assert_eq!(map.get(10), Some("Bracket_Body"));

    // NAUO fallback when product and solid names are empty or placeholder
    const NAUO_STEP: &str = "ISO-10303-21;\n\
                             HEADER;\n\
                             FILE_DESCRIPTION(('Test'), '2;1');\n\
                             FILE_NAME('test.stp', '2026-09-01', ('Author'), ('Org'), 'Prep', 'Sys', 'Auth');\n\
                             FILE_SCHEMA(('AUTOMOTIVE_DESIGN'));\n\
                             ENDSEC;\n\
                             DATA;\n\
                             #10 = CLOSED_SHELL('', (#1));\n\
                             #20 = MANIFOLD_SOLID_BREP('#20', #10);\n\
                             #30 = ADVANCED_BREP_SHAPE_REPRESENTATION('', (#20), #5);\n\
                             #40 = SHAPE_DEFINITION_REPRESENTATION(#50, #30);\n\
                             #50 = PRODUCT_DEFINITION_SHAPE('', '', #60);\n\
                             #60 = PRODUCT_DEFINITION('design', '', #70, #8);\n\
                             #70 = PRODUCT_DEFINITION_FORMATION('1', '', #80);\n\
                             #80 = PRODUCT('', '', '', (#9));\n\
                             #90 = NEXT_ASSEMBLY_USAGE_OCCURRENCE('bolt_1', '', 'bolt_1', #100, #60, $);\n\
                             ENDSEC;\n\
                             END-ISO-10303-21;";

    let parsed_nauo = ruststep::parser::parse(NAUO_STEP).expect("parsed exchange");
    let mut ex_nauo = parsed_nauo.clone();
    let index_nauo = ExchangeIndex::build(&mut ex_nauo);
    let map_nauo = StepNameMap::from_index(&index_nauo);
    assert_eq!(map_nauo.len(), 1);
    assert_eq!(map_nauo.get(10), Some("bolt_1"));
}

#[wasm_bindgen_test]
fn test_step_name_map_as1_tc_214() {
    const STEP_TEXT: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/samples/as1-tc-214.stp"
    ));
    let parsed = ruststep::parser::parse(STEP_TEXT).expect("parsed exchange");
    let mut ex = parsed.clone();
    let index = ExchangeIndex::build(&mut ex);
    let map = StepNameMap::from_index(&index);
    assert_eq!(map.len(), 5);

    assert_eq!(map.get(601), Some("l-bracket"));
    assert_eq!(map.get(879), Some("nut"));
    assert_eq!(map.get(1109), Some("bolt"));
    assert_eq!(map.get(1871), Some("plate"));
    assert_eq!(map.get(2018), Some("rod"));
}

#[wasm_bindgen_test]
fn test_step_name_map_part1_ap203() {
    const STEP_TEXT: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/samples/Part1.stp"));
    let parsed = ruststep::parser::parse(STEP_TEXT).expect("parsed exchange");
    let mut ex = parsed.clone();
    let index = ExchangeIndex::build(&mut ex);
    let map = StepNameMap::from_index(&index);
    assert_eq!(map.len(), 1);
    assert_eq!(map.get(51), Some("PartBody"));
}

#[wasm_bindgen_test]
fn test_step_name_map_kxt_331_lhs() {
    const STEP_TEXT: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/samples/KXT_331_LHS.STEP"
    ));
    let parsed = ruststep::parser::parse(STEP_TEXT).expect("parsed exchange");
    let mut ex = parsed.clone();
    let index = ExchangeIndex::build(&mut ex);
    let map = StepNameMap::from_index(&index);
    assert_eq!(map.len(), 4);
    assert_eq!(map.get(2543), Some("Pin 1"));
    assert_eq!(map.get(3932), Some("Pin 2"));
    assert_eq!(map.get(4070), Some("Cap"));
    assert_eq!(map.get(4592), Some("Housing"));
}

#[wasm_bindgen_test]
fn test_step_name_map_expansion_card() {
    const STEP_TEXT: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/samples/ExpansionCard_SelfTapping.stp"
    ));
    let parsed = ruststep::parser::parse(STEP_TEXT).expect("parsed exchange");
    let mut ex = parsed.clone();
    let index = ExchangeIndex::build(&mut ex);
    let map = StepNameMap::from_index(&index);
    assert_eq!(map.len(), 3);
    assert_eq!(map.get(1671), Some("COMPOUND_1"));
    assert_eq!(map.get(8128), Some("FW_EXP_1USBC_FRAME_CLIP_BC_229_"));
    assert_eq!(map.get(9178), Some("STAR_SCREW_M2X3L_298_1"));
}

#[wasm_bindgen_test]
fn test_step_name_map_io1_ca_214_fallback_empty() {
    const STEP_TEXT: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/samples/io1-ca-214.stp"
    ));
    let parsed = ruststep::parser::parse(STEP_TEXT).expect("parsed exchange");
    let mut ex = parsed.clone();
    let index = ExchangeIndex::build(&mut ex);
    let map = StepNameMap::from_index(&index);
    assert!(map.is_empty());
}
