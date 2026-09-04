//! Color domain model, STEP ISO 10303-46 presentation color extraction, and palette helpers.

use std::collections::HashMap;
use std::fmt::Write;
use std::ops::Deref;

use bytemuck::{Pod, Zeroable};
use glam::Vec4;
use serde::{Deserialize, Serialize};

use crate::common::utils::{
    extract_entity_refs, param_as_list, param_as_real, param_as_ref, param_as_str,
};
use crate::ruststep::ast::{EntityInstance, Exchange, Parameter};

/// RGBA color representation backed by `glam::Vec4`.
///
/// `#[repr(transparent)]` and `#[serde(transparent)]` ensure:
/// - Zero-cost binary compatibility with WebGPU uniform buffers (`Pod`, `Zeroable`).
/// - Clean serialization format identical to `glam::Vec4` for backward-compatible cache storage.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Pod, Zeroable)]
#[serde(transparent)]
pub struct Color(pub Vec4);

impl Deref for Color {
    type Target = Vec4;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<Vec4> for Color {
    #[inline]
    fn from(v: Vec4) -> Self {
        Self(v)
    }
}

impl From<Color> for Vec4 {
    #[inline]
    fn from(c: Color) -> Self {
        c.0
    }
}

impl Color {
    pub const WHITE: Self = Self(Vec4::ONE);
    pub const BLACK: Self = Self(Vec4::new(0.0, 0.0, 0.0, 1.0));
    pub const TRANSPARENT: Self = Self(Vec4::ZERO);
    pub const DEFAULT_PART: Self = Self(Vec4::new(0.8, 0.8, 0.8, 1.0));

    /// Constructs a color from RGBA channels in `0.0..=1.0`.
    #[inline]
    pub const fn new(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self(Vec4::new(r, g, b, a))
    }

    /// Constructs an opaque color (`a = 1.0`) from RGB channels in `0.0..=1.0`.
    #[inline]
    pub const fn rgb(r: f32, g: f32, b: f32) -> Self {
        Self::new(r, g, b, 1.0)
    }

    /// Constructs a color wrapping an existing `glam::Vec4`.
    #[inline]
    pub const fn from_vec4(vec: Vec4) -> Self {
        Self(vec)
    }

    /// Returns the underlying `glam::Vec4`.
    #[inline]
    pub const fn as_vec4(&self) -> Vec4 {
        self.0
    }

    /// Red component.
    #[inline]
    pub const fn r(&self) -> f32 {
        self.0.to_array()[0]
    }

    /// Green component.
    #[inline]
    pub const fn g(&self) -> f32 {
        self.0.to_array()[1]
    }

    /// Blue component.
    #[inline]
    pub const fn b(&self) -> f32 {
        self.0.to_array()[2]
    }

    /// Alpha component.
    #[inline]
    pub const fn a(&self) -> f32 {
        self.0.to_array()[3]
    }

    /// Converts to a `[f32; 4]` array `[r, g, b, a]`.
    #[inline]
    pub const fn to_array(&self) -> [f32; 4] {
        self.0.to_array()
    }

    /// Constructs from a `[f32; 4]` array `[r, g, b, a]`.
    #[inline]
    pub const fn from_array(arr: [f32; 4]) -> Self {
        Self::new(arr[0], arr[1], arr[2], arr[3])
    }

    /// Constructs an opaque color from 8-bit integers `0..=255`.
    #[inline]
    pub const fn from_rgb_u8(r: u8, g: u8, b: u8) -> Self {
        Self::rgb(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0)
    }

    /// Constructs a color from 8-bit RGBA integers `0..=255`.
    #[inline]
    pub const fn from_rgba_u8(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self::new(
            r as f32 / 255.0,
            g as f32 / 255.0,
            b as f32 / 255.0,
            a as f32 / 255.0,
        )
    }

    /// Formats the color as an uppercase 6-digit hex string `"#RRGGBB"`.
    pub fn to_hex(&self) -> String {
        let r = (self.0.x.clamp(0.0, 1.0) * 255.0).round() as u8;
        let g = (self.0.y.clamp(0.0, 1.0) * 255.0).round() as u8;
        let b = (self.0.z.clamp(0.0, 1.0) * 255.0).round() as u8;
        format!("#{r:02X}{g:02X}{b:02X}")
    }

    /// Parses a hex color string like `"#RRGGBB"`, `"RRGGBB"`, `"#RGB"`, or `"#RRGGBBAA"`.
    pub fn from_hex(hex: &str) -> Option<Self> {
        let clean = hex.trim().trim_start_matches('#');
        match clean.len() {
            3 => {
                const HEX_SHORT_MULTIPLIER: u8 = 17;
                let r = u8::from_str_radix(&clean[0..1], 16).ok()? * HEX_SHORT_MULTIPLIER;
                let g = u8::from_str_radix(&clean[1..2], 16).ok()? * HEX_SHORT_MULTIPLIER;
                let b = u8::from_str_radix(&clean[2..3], 16).ok()? * HEX_SHORT_MULTIPLIER;
                Some(Self::from_rgb_u8(r, g, b))
            }
            6 => {
                let r = u8::from_str_radix(&clean[0..2], 16).ok()?;
                let g = u8::from_str_radix(&clean[2..4], 16).ok()?;
                let b = u8::from_str_radix(&clean[4..6], 16).ok()?;
                Some(Self::from_rgb_u8(r, g, b))
            }
            8 => {
                let r = u8::from_str_radix(&clean[0..2], 16).ok()?;
                let g = u8::from_str_radix(&clean[2..4], 16).ok()?;
                let b = u8::from_str_radix(&clean[4..6], 16).ok()?;
                let a = u8::from_str_radix(&clean[6..8], 16).ok()?;
                Some(Self::from_rgba_u8(r, g, b, a))
            }
            _ => None,
        }
    }

    /// Formats the color as a CSS `rgba(r, g, b, a)` string.
    pub fn to_css_rgba(&self) -> String {
        const CSS_RGBA_CAPACITY: usize = 24;
        let r = (self.0.x.clamp(0.0, 1.0) * 255.0).round() as u8;
        let g = (self.0.y.clamp(0.0, 1.0) * 255.0).round() as u8;
        let b = (self.0.z.clamp(0.0, 1.0) * 255.0).round() as u8;
        let a = self.0.w.clamp(0.0, 1.0);
        let mut out = String::with_capacity(CSS_RGBA_CAPACITY);
        let _ = write!(out, "rgba({r}, {g}, {b}, {a:.3})");
        out
    }

    /// Maps standard ISO 10303-46 predefined draughting color names to `Color`.
    pub fn from_draughting_name(name: &str) -> Option<Self> {
        let clean = name.trim().trim_matches('\'').trim_matches('"');
        if clean.eq_ignore_ascii_case("red") {
            Some(Self::rgb(1.0, 0.0, 0.0))
        } else if clean.eq_ignore_ascii_case("green") {
            Some(Self::rgb(0.0, 1.0, 0.0))
        } else if clean.eq_ignore_ascii_case("blue") {
            Some(Self::rgb(0.0, 0.0, 1.0))
        } else if clean.eq_ignore_ascii_case("yellow") {
            Some(Self::rgb(1.0, 1.0, 0.0))
        } else if clean.eq_ignore_ascii_case("magenta") {
            Some(Self::rgb(1.0, 0.0, 1.0))
        } else if clean.eq_ignore_ascii_case("cyan") {
            Some(Self::rgb(0.0, 1.0, 1.0))
        } else if clean.eq_ignore_ascii_case("black") {
            Some(Self::rgb(0.0, 0.0, 0.0))
        } else if clean.eq_ignore_ascii_case("white") {
            Some(Self::rgb(1.0, 1.0, 1.0))
        } else if clean.eq_ignore_ascii_case("grey") || clean.eq_ignore_ascii_case("gray") {
            Some(Self::rgb(0.5, 0.5, 0.5))
        } else if clean.eq_ignore_ascii_case("orange") {
            Some(Self::rgb(1.0, 0.5, 0.0))
        } else if clean.eq_ignore_ascii_case("brown") {
            Some(Self::rgb(0.6, 0.3, 0.0))
        } else {
            None
        }
    }

    /// Flexible parser: checks standard draughting color names, then hex strings.
    pub fn parse_flexible(text: &str) -> Option<Self> {
        Self::from_draughting_name(text).or_else(|| Self::from_hex(text))
    }
}

impl Default for Color {
    #[inline]
    fn default() -> Self {
        Self::DEFAULT_PART
    }
}

/// Palette count for cycling colors.
pub const PART_COLORS_COUNT: usize = 10;

/// Default cycling palette for parts that do not declare an in-file color.
pub const PART_COLORS: [Color; PART_COLORS_COUNT] = [
    Color::new(0.310, 0.765, 0.969, 1.0), // Sky blue
    Color::new(0.988, 0.553, 0.235, 1.0), // Coral
    Color::new(0.400, 0.843, 0.584, 1.0), // Emerald
    Color::new(0.706, 0.533, 0.980, 1.0), // Violet
    Color::new(0.992, 0.816, 0.294, 1.0), // Amber
    Color::new(0.969, 0.443, 0.584, 1.0), // Rose
    Color::new(0.306, 0.804, 0.769, 1.0), // Teal
    Color::new(0.980, 0.698, 0.200, 1.0), // Orange
    Color::new(0.549, 0.655, 0.980, 1.0), // Indigo
    Color::new(0.627, 0.808, 0.271, 1.0), // Lime
];

/// Returns the cycling palette color for part index `index`.
#[inline]
pub const fn part_color(index: usize) -> Color {
    PART_COLORS[index % PART_COLORS_COUNT]
}

// ---------------------------------------------------------------------------
// STEP File Color Extraction
// ---------------------------------------------------------------------------

/// Extracted mapping of STEP shell entity IDs to their resolved presentation [`Color`].
#[derive(Debug, Clone, Default, PartialEq)]
pub struct StepColorMap {
    /// Maps STEP entity ID (typically `CLOSED_SHELL` or `OPEN_SHELL`) to resolved `Color`.
    pub shell_colors: HashMap<u64, Color>,
}

impl StepColorMap {
    /// Returns the resolved color for shell with STEP entity ID `shell_id`.
    #[inline]
    pub fn get(&self, shell_id: u64) -> Option<Color> {
        self.shell_colors.get(&shell_id).copied()
    }

    /// Whether any colors were extracted from the STEP file.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.shell_colors.is_empty()
    }

    /// Total number of colored shells identified.
    #[inline]
    pub fn len(&self) -> usize {
        self.shell_colors.len()
    }

    /// Extracts colors and connects presentation styles to shells from a parsed STEP AST.
    pub fn from_exchange(exchange: &Exchange) -> Self {
        let mut direct_colors: HashMap<u64, Color> = HashMap::new();
        let mut style_edges: HashMap<u64, Vec<u64>> = HashMap::new();
        let mut solid_to_shell: HashMap<u64, u64> = HashMap::new();
        let mut shell_to_faces: HashMap<u64, Vec<u64>> = HashMap::new();
        let mut face_to_shell: HashMap<u64, u64> = HashMap::new();
        let mut styled_items: Vec<(Vec<u64>, u64)> = Vec::new();

        for section in &exchange.data {
            for entity in &section.entities {
                let EntityInstance::Simple { id, record } = entity else {
                    continue;
                };
                let entity_id = *id;
                let name = record.name.as_str();

                if name.eq_ignore_ascii_case("COLOUR_RGB") {
                    if let Some(params) = param_as_list(&record.parameter).filter(|p| p.len() >= 4)
                    {
                        let r = param_as_real(&params[1]).unwrap_or(0.0) as f32;
                        let g = param_as_real(&params[2]).unwrap_or(0.0) as f32;
                        let b = param_as_real(&params[3]).unwrap_or(0.0) as f32;
                        direct_colors.insert(
                            entity_id,
                            Color::rgb(r.clamp(0.0, 1.0), g.clamp(0.0, 1.0), b.clamp(0.0, 1.0)),
                        );
                    }
                } else if name.eq_ignore_ascii_case("DRAUGHTING_PRE_DEFINED_COLOUR")
                    || name.eq_ignore_ascii_case("PRE_DEFINED_COLOUR")
                {
                    let col_name = match &record.parameter {
                        Parameter::String(s) | Parameter::Enumeration(s) => Some(s.as_str()),
                        Parameter::List(l) => l.first().and_then(param_as_str),
                        _ => None,
                    };
                    if let Some(color) = col_name.and_then(Color::parse_flexible) {
                        direct_colors.insert(entity_id, color);
                    }
                } else if name.eq_ignore_ascii_case("FILL_AREA_STYLE_COLOUR")
                    || name.eq_ignore_ascii_case("FILL_AREA_STYLE")
                    || name.eq_ignore_ascii_case("SURFACE_STYLE_FILL_AREA")
                    || name.eq_ignore_ascii_case("SURFACE_SIDE_STYLE")
                    || name.eq_ignore_ascii_case("SURFACE_STYLE_USAGE")
                    || name.eq_ignore_ascii_case("PRESENTATION_STYLE_ASSIGNMENT")
                    || name.eq_ignore_ascii_case("CURVE_STYLE")
                    || name.eq_ignore_ascii_case("SYMBOL_STYLE")
                    || name.eq_ignore_ascii_case("SYMBOL_COLOUR")
                {
                    let refs = extract_entity_refs(&record.parameter);
                    if !refs.is_empty() {
                        style_edges.insert(entity_id, refs);
                    }
                } else if name.eq_ignore_ascii_case("MANIFOLD_SOLID_BREP")
                    || name.eq_ignore_ascii_case("BREP_WITH_VOIDS")
                    || name.eq_ignore_ascii_case("FACETED_BREP")
                {
                    let outer_shell = param_as_list(&record.parameter)
                        .and_then(|p| p.get(1))
                        .and_then(param_as_ref);
                    if let Some(shell_id) = outer_shell {
                        solid_to_shell.insert(entity_id, shell_id);
                    }
                } else if name.eq_ignore_ascii_case("CLOSED_SHELL")
                    || name.eq_ignore_ascii_case("OPEN_SHELL")
                {
                    let faces_param = param_as_list(&record.parameter).and_then(|p| p.get(1));
                    if let Some(faces) = faces_param {
                        let face_refs = extract_entity_refs(faces);
                        for &face_id in &face_refs {
                            face_to_shell.insert(face_id, entity_id);
                        }
                        shell_to_faces.insert(entity_id, face_refs);
                    }
                } else if name.eq_ignore_ascii_case("STYLED_ITEM")
                    || name.eq_ignore_ascii_case("OVER_RIDING_STYLED_ITEM")
                {
                    let Some(params) = param_as_list(&record.parameter) else {
                        continue;
                    };
                    let target = params.get(2).and_then(param_as_ref);
                    if let (Some(styles_param), Some(target_id)) = (params.get(1), target) {
                        let style_refs = extract_entity_refs(styles_param);
                        styled_items.push((style_refs, target_id));
                    }
                }
            }
        }

        // Resolve presentation styles recursively to Color
        let mut resolved_styles: HashMap<u64, Color> = direct_colors;
        let mut changed = true;
        let mut passes = 0;
        // Limit passes to prevent infinite loop on cyclic data
        const MAX_STYLE_RESOLUTION_PASSES: usize = 16;
        while changed && passes < MAX_STYLE_RESOLUTION_PASSES {
            changed = false;
            passes += 1;
            for (&style_id, refs) in &style_edges {
                if resolved_styles.contains_key(&style_id) {
                    continue;
                }
                for &child_id in refs {
                    if let Some(&color) = resolved_styles.get(&child_id) {
                        resolved_styles.insert(style_id, color);
                        changed = true;
                        break;
                    }
                }
            }
        }

        // Map styled items to shells
        let mut shell_colors = HashMap::new();
        for (styles, target) in styled_items {
            let mut resolved_color = None;
            for style_id in styles {
                if let Some(&c) = resolved_styles.get(&style_id) {
                    resolved_color = Some(c);
                    break;
                }
            }

            let Some(color) = resolved_color else {
                continue;
            };

            // Resolve target geometry to a shell ID
            if let Some(&shell_id) = solid_to_shell.get(&target) {
                shell_colors.insert(shell_id, color);
            } else if shell_to_faces.contains_key(&target) {
                shell_colors.insert(target, color);
            } else if let Some(&shell_id) = face_to_shell.get(&target) {
                shell_colors.insert(shell_id, color);
            } else {
                // Also store direct target ref as fallback
                shell_colors.insert(target, color);
            }
        }

        Self { shell_colors }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wasm_bindgen_test::*;

    wasm_bindgen_test_configure!(run_in_browser);

    #[wasm_bindgen_test]
    fn test_color_constructors_and_accessors() {
        let c = Color::new(0.2, 0.4, 0.6, 0.8);
        assert_eq!(c.r(), 0.2);
        assert_eq!(c.g(), 0.4);
        assert_eq!(c.b(), 0.6);
        assert_eq!(c.a(), 0.8);

        let rgb = Color::rgb(0.1, 0.5, 0.9);
        assert_eq!(rgb.a(), 1.0);

        let arr = c.to_array();
        assert_eq!(arr, [0.2, 0.4, 0.6, 0.8]);
        assert_eq!(Color::from_array(arr), c);
    }

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
    fn test_serde_roundtrip() {
        let c = Color::new(0.1, 0.2, 0.3, 1.0);
        let json = serde_json::to_string(&c).expect("serialize");
        let deserialized: Color = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(c, deserialized);
    }

    #[wasm_bindgen_test]
    fn test_palette_cycling() {
        assert_eq!(part_color(0), PART_COLORS[0]);
        assert_eq!(part_color(10), PART_COLORS[0]);
        assert_eq!(part_color(1), PART_COLORS[1]);
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
        let parsed = crate::ruststep::parser::parse(STEP_TEXT).expect("parsed exchange");
        let map = StepColorMap::from_exchange(&parsed);
        assert_eq!(map.len(), 1);
        let col = map.get(80).expect("shell 80 color resolved");
        assert_eq!(col, Color::rgb(1.0, 0.5, 0.0));
    }

    #[wasm_bindgen_test]
    fn test_step_color_map_as1_tc_214() {
        const STEP_TEXT: &str = include_str!("../../examples/as1-tc-214.stp");
        let parsed = crate::ruststep::parser::parse(STEP_TEXT).expect("parsed exchange");
        let map = StepColorMap::from_exchange(&parsed);
        assert_eq!(map.len(), 5);

        // Verify known assembly colors from as1-tc-214:
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
        const STEP_TEXT: &str = include_str!("../../examples/Part1.stp");
        let parsed = crate::ruststep::parser::parse(STEP_TEXT).expect("parsed exchange");
        let map = StepColorMap::from_exchange(&parsed);
        assert!(map.is_empty());
    }
}
