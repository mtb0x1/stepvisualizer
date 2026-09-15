use stepvisualizer::common::color::{Color, StepColorMap};
use stepvisualizer::ruststep;
use wasm_bindgen_test::*;

#[wasm_bindgen_test]
fn test_color_hex_conversions() {
    let orange = Color::from_rgb_u8(255, 128, 0);
    assert_eq!(orange.to_hex(), "#FF8000");

    let parsed = Color::from_hex("#FF8000").expect("hex parse");
    assert!((parsed.r() - 1.0).abs() < 1e-3);
    assert!((parsed.g() - 0.5019).abs() < 1e-3);
    assert!((parsed.b() - 0.0).abs() < 1e-3);

    let short_hex = Color::from_hex("#F80").expect("short hex parse");
    assert_eq!(short_hex.to_hex(), "#FF8800");
}

#[wasm_bindgen_test]
fn test_color_css_rgba() {
    let c = Color::new(1.0, 0.0, 0.5, 0.75);
    let css = c.to_css_rgba();
    assert_eq!(css, "rgba(255, 0, 128, 0.750)");
}

#[wasm_bindgen_test]
fn test_draughting_predefined_colors() {
    assert_eq!(
        Color::from_draughting_name("yellow"),
        Some(Color::rgb(1.0, 1.0, 0.0))
    );
    assert_eq!(
        Color::from_draughting_name("'BLUE'"),
        Some(Color::rgb(0.0, 0.0, 1.0))
    );
    assert_eq!(
        Color::from_draughting_name("red"),
        Some(Color::rgb(1.0, 0.0, 0.0))
    );
    assert_eq!(Color::from_draughting_name("unknown_color"), None);
}

#[wasm_bindgen_test]
fn test_pod_zeroable_bytemuck() {
    let c = Color::new(1.0, 2.0, 3.0, 4.0);
    let bytes = bytemuck::bytes_of(&c);
    assert_eq!(bytes.len(), 16);
    let roundtrip: &Color = bytemuck::from_bytes(bytes);
    assert_eq!(*roundtrip, c);
}

#[wasm_bindgen_test]
fn test_step_color_map_synthetic() {
    const STEP_TEXT: &str = "ISO-10303-21;\n\
                             HEADER;\n\
                             FILE_DESCRIPTION(('Test'), '2;1');\n\
                             FILE_NAME('test.stp', '2026-09-01', ('Author'), ('Org'), 'Prep', 'Sys', 'Auth');\n\
                             FILE_SCHEMA(('AUTOMOTIVE_DESIGN'));\n\
                             ENDSEC;\n\
                             DATA;\n\
                             #10 = COLOUR_RGB('', 1.0, 0.5, 0.0);\n\
                             #20 = FILL_AREA_STYLE_COLOUR('', #10);\n\
                             #30 = FILL_AREA_STYLE('', (#20));\n\
                             #40 = SURFACE_STYLE_FILL_AREA(#30);\n\
                             #50 = SURFACE_SIDE_STYLE('', (#40));\n\
                             #60 = SURFACE_STYLE_USAGE(.BOTH., #50);\n\
                             #70 = PRESENTATION_STYLE_ASSIGNMENT((#60));\n\
                             #80 = CLOSED_SHELL('shell', (#1));\n\
                             #90 = MANIFOLD_SOLID_BREP('solid', #80);\n\
                             #100 = STYLED_ITEM('', (#70), #90);\n\
                             ENDSEC;\n\
                             END-ISO-10303-21;";
    let parsed = ruststep::parser::parse(STEP_TEXT).expect("parsed exchange");
    let map = StepColorMap::from_exchange(&parsed);
    assert_eq!(map.len(), 1);
    let col = map.get(80).expect("shell 80 color resolved");
    assert_eq!(col, Color::rgb(1.0, 0.5, 0.0));
}

#[wasm_bindgen_test]
fn test_step_color_map_as1_tc_214() {
    const STEP_TEXT: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/samples/as1-tc-214.stp"
    ));
    let parsed = ruststep::parser::parse(STEP_TEXT).expect("parsed exchange");
    let map = StepColorMap::from_exchange(&parsed);
    assert_eq!(map.len(), 5);

    // Shell #601 -> green
    assert_eq!(map.get(601), Some(Color::rgb(0.0, 1.0, 0.0)));
    // Shell #879 -> red
    assert_eq!(map.get(879), Some(Color::rgb(1.0, 0.0, 0.0)));
    // Shell #1109 -> blue
    assert_eq!(map.get(1109), Some(Color::rgb(0.0, 0.0, 1.0)));
    // Shell #1871 -> yellow-ish RGB (0.78, 0.78, 0.0)
    let c1871 = map.get(1871).expect("shell 1871 color");
    assert!((c1871.r() - 0.780392).abs() < 1e-4);
    assert!((c1871.g() - 0.780392).abs() < 1e-4);
    assert!((c1871.b() - 0.0).abs() < 1e-4);
    // Shell #2018 -> orange RGB (1.0, 0.5686, 0.0)
    let c2018 = map.get(2018).expect("shell 2018 color");
    assert!((c2018.r() - 1.0).abs() < 1e-4);
    assert!((c2018.g() - 0.568627).abs() < 1e-4);
}

#[wasm_bindgen_test]
fn test_step_color_map_part1_ap203_empty() {
    const STEP_TEXT: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/samples/Part1.stp"));
    let parsed = ruststep::parser::parse(STEP_TEXT).expect("parsed exchange");
    let map = StepColorMap::from_exchange(&parsed);
    assert!(map.is_empty());
}
