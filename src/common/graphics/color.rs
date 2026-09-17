//! Color domain model, STEP ISO 10303-46 presentation color extraction, and palette helpers.

use crate::common::fast_hash::FastU64Map;
use std::fmt::Write;
use std::ops::Deref;

use bytemuck::{Pod, Zeroable};
use glam::Vec4;
use serde::{Deserialize, Serialize};

use crate::common::ast_helpers::ParameterExt;
use crate::common::exchange_index::ExchangeIndex;
use crate::ruststep::ast::{Parameter, Record};

/// RGBA color representation backed by `glam::Vec4`.
///
/// `#[repr(transparent)]` and `#[serde(transparent)]` ensure:
/// - Zero-cost binary compatibility with WebGPU uniform buffers (`Pod`, `Zeroable`).
/// - Clean serialization format identical to `glam::Vec4` for backward-compatible cache storage.
#[repr(transparent)]
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Serialize,
    Deserialize,
    Pod,
    Zeroable,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
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

    /// Parses standard CSS color names, RGB/RGBA strings, and Hex strings.
    pub fn parse(text: &str) -> Option<Self> {
        let clean = text.trim().trim_matches('\'').trim_matches('"');
        csscolorparser::parse(clean)
            .ok()
            .map(|c| Self::new(c.r, c.g, c.b, c.a))
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

    /// Parses a color from a `COLOUR_RGB` STEP record.
    pub fn from_rgb_record(record: &Record) -> Option<Self> {
        let params = record.parameter.try_extract::<&[Parameter]>()?;
        if params.len() < 4 {
            return None;
        }
        let r = params[1].try_extract::<f64>().unwrap_or(0.0) as f32;
        let g = params[2].try_extract::<f64>().unwrap_or(0.0) as f32;
        let b = params[3].try_extract::<f64>().unwrap_or(0.0) as f32;
        Some(Self::rgb(
            r.clamp(0.0, 1.0),
            g.clamp(0.0, 1.0),
            b.clamp(0.0, 1.0),
        ))
    }

    /// Parses a color from a `PRE_DEFINED_COLOUR` or `DRAUGHTING_PRE_DEFINED_COLOUR` STEP record.
    pub fn from_predefined_record(record: &Record) -> Option<Self> {
        let col_name = match &record.parameter {
            Parameter::String(s) | Parameter::Enumeration(s) => Some(s.as_str()),
            Parameter::List(l) => l.first().and_then(|p| p.try_extract::<&str>()),
            _ => None,
        };
        col_name.and_then(Self::parse)
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
    pub shell_colors: FastU64Map<Color>,
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

    /// Extracts colors and connects presentation styles to shells from a pre-built [`ExchangeIndex`].
    pub fn from_index(index: &ExchangeIndex) -> Self {
        // Resolve presentation styles recursively to Color via memoized DFS
        let mut resolved_styles: FastU64Map<Color> = index.direct_colors.clone();
        let mut visiting: Vec<u64> = Vec::with_capacity(8);
        for &style_id in index.style_edges.keys() {
            resolve_style_color(
                style_id,
                &index.style_edges,
                &mut resolved_styles,
                &mut visiting,
            );
        }

        // Map styled items to shells
        let mut shell_colors = FastU64Map::default();
        for (styles, target) in &index.styled_items {
            let mut resolved_color = None;
            for style_id in styles {
                if let Some(&c) = resolved_styles.get(style_id) {
                    resolved_color = Some(c);
                    break;
                }
            }
            let Some(color) = resolved_color else {
                continue;
            };

            // Resolve target geometry to a shell ID
            if let Some(&shell_id) = index.solid_to_shell.get(target) {
                shell_colors.insert(shell_id, color);
            } else if index.shell_to_faces.contains_key(target) {
                shell_colors.insert(*target, color);
            } else if let Some(&shell_id) = index.face_to_shell.get(target) {
                shell_colors.insert(shell_id, color);
            } else {
                shell_colors.insert(*target, color);
            }
        }

        Self { shell_colors }
    }
}

/// Recursively resolves a presentation style entity to its terminal [`Color`] via memoized DFS,
/// breaking any cyclic references safely using a call-stack vector.
fn resolve_style_color(
    style_id: u64,
    style_edges: &FastU64Map<smallvec::SmallVec<[u64; 2]>>,
    resolved: &mut FastU64Map<Color>,
    visiting: &mut Vec<u64>,
) -> Option<Color> {
    if let Some(&color) = resolved.get(&style_id) {
        return Some(color);
    }
    if visiting.contains(&style_id) {
        // Cycle detected; abort this branch
        return None;
    }
    visiting.push(style_id);
    if let Some(children) = style_edges.get(&style_id) {
        for &child in children {
            if let Some(c) = resolve_style_color(child, style_edges, resolved, visiting) {
                resolved.insert(style_id, c);
                visiting.pop();
                return Some(c);
            }
        }
    }
    visiting.pop();
    None
}
