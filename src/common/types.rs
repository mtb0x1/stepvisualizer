use serde::{Deserialize, Serialize};

use super::render::RenderablePart;

/// Strongly-typed file content hash ID.
///
/// `repr(transparent)` ensures zero-cost runtime wrapping over `String`
/// and transparent serde serialization (serialized as a flat string in JSON/localStorage).
#[repr(transparent)]
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize, Deserialize)]
pub struct FileId(pub String);

impl FileId {
    pub const fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl std::fmt::Display for FileId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FileId {
    /// Content-based model identity (16 hex chars) computed via stable XXH3-64 hash.
    pub fn from_content(text: &str) -> Self {
        use std::fmt::Write;
        let hash = xxhash_rust::xxh3::xxh3_64(text.as_bytes());
        let mut s = String::with_capacity(16);
        let _ = write!(s, "{:016x}", hash);
        Self(s)
    }
}

impl From<String> for FileId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for FileId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl std::ops::Deref for FileId {
    type Target = str;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl AsRef<str> for FileId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl std::borrow::Borrow<str> for FileId {
    fn borrow(&self) -> &str {
        &self.0
    }
}

/// Physical pixel dimensions of a rendering viewport or canvas.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ViewportSize {
    pub width: u32,
    pub height: u32,
}

impl ViewportSize {
    pub const ZERO: Self = Self {
        width: 0,
        height: 0,
    };

    /// Construct new viewport dimensions.
    pub const fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }

    /// Extract client dimensions from an HTML canvas element, clamped to at least 1x1.
    pub fn from_canvas(canvas: &web_sys::HtmlCanvasElement) -> Self {
        Self {
            width: canvas.client_width().max(1) as u32,
            height: canvas.client_height().max(1) as u32,
        }
    }

    /// Whether both width and height are non-zero.
    pub const fn is_valid(&self) -> bool {
        self.width > 0 && self.height > 0
    }

    /// Aspect ratio (width / height), or 1.0 when height is zero.
    pub fn aspect_ratio(&self) -> f32 {
        if self.height == 0 {
            1.0
        } else {
            self.width as f32 / self.height as f32
        }
    }
}

/// Standard length units parsed from STEP SI_UNIT and conversion factors to meters.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum LengthUnit {
    Millimetre,
    Centimetre,
    Decimetre,
    Metre,
    Kilometre,
    Inch,
    Foot,
    Custom,
}

impl LengthUnit {
    /// Standard unit symbol ("mm", "cm", "m", etc.).
    pub const fn symbol(&self) -> &'static str {
        match self {
            Self::Millimetre => "mm",
            Self::Centimetre => "cm",
            Self::Decimetre => "dm",
            Self::Metre => "m",
            Self::Kilometre => "km",
            Self::Inch => "in",
            Self::Foot => "ft",
            Self::Custom => "units",
        }
    }

    /// Parse from STEP SI_UNIT identifier and optional prefix.
    pub fn from_si_spec(unit: &str, prefix: Option<&str>) -> Option<Self> {
        match unit.to_ascii_uppercase().as_str() {
            "METRE" | "METER" => match prefix.map(|p| p.to_ascii_uppercase()) {
                Some(p) if p == "MILLI" => Some(Self::Millimetre),
                Some(p) if p == "CENTI" => Some(Self::Centimetre),
                Some(p) if p == "DECI" => Some(Self::Decimetre),
                Some(p) if p == "KILO" => Some(Self::Kilometre),
                _ => Some(Self::Metre),
            },
            "INCH" => Some(Self::Inch),
            "FOOT" | "FEET" => Some(Self::Foot),
            _ => None,
        }
    }

    /// Parse from unit name string (e.g. from CONVERSION_BASED_UNIT).
    pub fn from_name(name: &str) -> Option<Self> {
        let clean = name.trim().trim_matches('\'').trim_matches('"');
        match clean.to_ascii_uppercase().as_str() {
            "MM" | "MILLIMETRE" | "MILLIMETER" => Some(Self::Millimetre),
            "CM" | "CENTIMETRE" | "CENTIMETER" => Some(Self::Centimetre),
            "DM" | "DECIMETRE" | "DECIMETER" => Some(Self::Decimetre),
            "M" | "METRE" | "METER" => Some(Self::Metre),
            "KM" | "KILOMETRE" | "KILOMETER" => Some(Self::Kilometre),
            "IN" | "INCH" | "INCHES" => Some(Self::Inch),
            "FT" | "FOOT" | "FEET" => Some(Self::Foot),
            _ => None,
        }
    }
}

impl std::fmt::Display for LengthUnit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.symbol())
    }
}

/// STEP header section (ISO 10303-21), shaped for display in the details panel.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct StepHeader {
    pub file_description: String,
    pub implementation_level: String,
    pub file_name: String,
    pub time_stamp: String,
    pub author: Vec<String>,
    pub organization: Vec<String>,
    pub preprocessor_version: String,
    pub originating_system: String,
    pub authorization: String,
    pub file_schema: String,
}

/// Display metadata for a loaded file: header fields plus derived geometry
/// stats. Persisted inside `StepModel`, so newly added fields need
/// `#[serde(default)]` to stay load-compatible with previously saved models.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct Metadata {
    pub header: StepHeader,
    pub entity_count: usize,
    #[serde(default)]
    pub bounding_box: Option<BoundingBox>,
    #[serde(default)]
    pub units: Option<LengthUnit>,
    #[serde(default)]
    pub vertex_count: usize,
    #[serde(default)]
    pub triangle_count: usize,
    #[serde(default)]
    pub volume: Option<f64>,
    #[serde(default)]
    pub surface_area: Option<f64>,
}

/// One entry of the recent-files history. `id` is the file's content hash,
/// which doubles as the localStorage key of its persisted model.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct FileIndexItem {
    pub id: FileId,
    pub name: String,
    pub entity_count: usize,
    pub time_stamp: String,
}

/// Axis-aligned bounds in 3D space.
#[derive(Clone, Copy, PartialEq, Debug, Serialize, Deserialize)]
pub struct BoundingBox {
    pub min: [f64; 3],
    pub max: [f64; 3],
}

impl BoundingBox {
    /// An empty/inverted bounding box ready to be expanded.
    pub const EMPTY: Self = Self {
        min: [f64::INFINITY; 3],
        max: [f64::NEG_INFINITY; 3],
    };

    /// Create a bounding box with the given min and max coordinates.
    pub const fn new(min: [f64; 3], max: [f64; 3]) -> Self {
        Self { min, max }
    }

    /// True if the bounding box has valid, finite dimensions.
    pub fn is_valid(&self) -> bool {
        self.min[0].is_finite()
            && self.max[0].is_finite()
            && self.min[0] <= self.max[0]
            && self.min[1] <= self.max[1]
            && self.min[2] <= self.max[2]
    }

    /// Center point of the bounding box as f64.
    pub const fn center(&self) -> [f64; 3] {
        [
            (self.min[0] + self.max[0]) * 0.5,
            (self.min[1] + self.max[1]) * 0.5,
            (self.min[2] + self.max[2]) * 0.5,
        ]
    }

    /// Center point converted to `Vec3` for GPU and camera framing.
    pub fn center_f32(&self) -> glam::Vec3 {
        let c = self.center();
        glam::Vec3::new(c[0] as f32, c[1] as f32, c[2] as f32)
    }

    /// Dimensions (width, height, depth) as f64.
    pub const fn size(&self) -> [f64; 3] {
        [self.size_x(), self.size_y(), self.size_z()]
    }

    /// Size along the X axis.
    pub const fn size_x(&self) -> f64 {
        let diff = self.max[0] - self.min[0];
        if diff > 0.0 { diff } else { 0.0 }
    }

    /// Size along the Y axis.
    pub const fn size_y(&self) -> f64 {
        let diff = self.max[1] - self.min[1];
        if diff > 0.0 { diff } else { 0.0 }
    }

    /// Size along the Z axis.
    pub const fn size_z(&self) -> f64 {
        let diff = self.max[2] - self.min[2];
        if diff > 0.0 { diff } else { 0.0 }
    }

    /// Maximum dimension across X, Y, Z as f64.
    pub fn max_extent(&self) -> f64 {
        let s = self.size();
        s[0].max(s[1]).max(s[2])
    }

    /// Maximum dimension converted to f32.
    pub fn max_extent_f32(&self) -> f32 {
        self.max_extent() as f32
    }

    /// Expands this bounding box to include the given 3D point.
    pub fn expand_point(&mut self, p: [f64; 3]) {
        for i in 0..3 {
            self.min[i] = self.min[i].min(p[i]);
            self.max[i] = self.max[i].max(p[i]);
        }
    }
}

/// A fully processed STEP file: identity, metadata, tessellated parts, and
/// per-part visibility. The whole model is serialized into localStorage
/// under its `id`, so it survives reloads without re-parsing.
///
/// ### Part Visibility Contract
/// - Part visibility defaults to all-parts-visible (`vec![true; n]`) upon initial load.
/// - During an active session, visibility changes are tracked dynamically in UI state
///   and synchronized into the active `StepModel`.
/// - `#[serde(default)]` ensures deserializing cached models without a `part_visibility`
///   field safely yields an empty vector which is hydrated to all-true on load.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct StepModel {
    pub id: FileId,
    pub metadata: Metadata,
    pub render_parts: Vec<RenderablePart>,
    /// Per-part visibility mask. When empty after deserialization, callers hydrate to all-true.
    #[serde(default)]
    pub part_visibility: Vec<bool>,
    /// Monotonically increasing generation bumped on visibility changes to invalidate bounds cache.
    #[serde(default)]
    pub visibility_generation: u64,
    /// Cached bounding box for the current visibility generation (skipped during serialization).
    #[serde(skip)]
    pub cached_bounds: Option<(u64, BoundingBox)>,
}

impl StepModel {
    /// Compute total vertex count across all render parts.
    pub fn total_vertices(&self) -> usize {
        self.render_parts.iter().map(|p| p.vertex_count()).sum()
    }

    /// Compute total triangle count across all render parts.
    pub fn total_triangles(&self) -> usize {
        self.render_parts.iter().map(|p| p.triangle_count()).sum()
    }

    /// Calculate total volume across all render parts.
    pub fn calculate_total_volume(&self) -> f64 {
        self.render_parts.iter().map(|p| p.calculate_volume()).sum()
    }

    /// Calculate total surface area across all render parts.
    pub fn calculate_total_surface_area(&self) -> f64 {
        self.render_parts
            .iter()
            .map(|p| p.calculate_surface_area())
            .sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wasm_bindgen_test::*;

    wasm_bindgen_test_configure!(run_in_browser);

    /// Verifies that FileId content hashing with XXH3 produces an identical 16-character
    /// hexadecimal string deterministically across multiple runs.
    #[wasm_bindgen_test]
    fn file_id_deterministic_hash() {
        let content = "ISO-10303-21;\nHEADER;\nFILE_DESCRIPTION(('Test STEP File'),'2;1');\nENDSEC;\nEND-ISO-10303-21;";
        let id1 = FileId::from_content(content);
        let id2 = FileId::from_content(content);

        assert_eq!(id1, id2);
        assert_eq!(id1.as_str().len(), 16);
        assert!(id1.as_str().chars().all(|c| c.is_ascii_hexdigit()));
    }

    /// Verifies that distinct input contents produce different FileId hashes.
    #[wasm_bindgen_test]
    fn file_id_distinct_content() {
        let id_a = FileId::from_content("model_a");
        let id_b = FileId::from_content("model_b");

        assert_ne!(id_a, id_b);
    }

    /// Verifies that hashing an empty content string executes safely without panic
    /// and yields a valid 16-character hexadecimal identifier.
    #[wasm_bindgen_test]
    fn file_id_empty_string() {
        let id_empty = FileId::from_content("");
        assert_eq!(id_empty.as_str().len(), 16);
        assert!(id_empty.as_str().chars().all(|c| c.is_ascii_hexdigit()));
    }

    /// Verifies string conversion and trait implementations for FileId (Deref, AsRef<str>, Display, Borrow<str>).
    #[wasm_bindgen_test]
    fn file_id_string_conversion_traits() {
        use std::borrow::Borrow;

        let raw = "0123456789abcdef";
        let file_id = FileId::from(raw);

        // Deref
        let deref_str: &str = &file_id;
        assert_eq!(deref_str, raw);

        // AsRef<str>
        assert_eq!(file_id.as_ref(), raw);

        // Display
        assert_eq!(format!("{file_id}"), raw);

        // Borrow<str>
        let borrowed: &str = file_id.borrow();
        assert_eq!(borrowed, raw);

        // From<String> and From<&str>
        let from_string = FileId::from(raw.to_string());
        assert_eq!(from_string, file_id);
    }

    /// Verifies that an unexpanded BoundingBox::EMPTY sentinel is invalid (min = inf, max = -inf).
    #[wasm_bindgen_test]
    fn bbox_empty_invalid() {
        let bbox = BoundingBox::EMPTY;
        assert!(!bbox.is_valid());
    }

    /// Verifies that expanding BoundingBox::EMPTY with a single point creates a zero-size, valid bounding box
    /// where min equals max at the point coordinates.
    #[wasm_bindgen_test]
    fn bbox_single_point_expansion() {
        let mut bbox = BoundingBox::EMPTY;
        bbox.expand_point([1.0, 2.0, 3.0]);

        assert!(bbox.is_valid());
        assert_eq!(bbox.min, [1.0, 2.0, 3.0]);
        assert_eq!(bbox.max, [1.0, 2.0, 3.0]);
    }

    /// Verifies that sequentially expanding a bounding box with multiple points correctly computes
    /// the component-wise minimum and maximum coordinate extrema.
    #[wasm_bindgen_test]
    fn bbox_multiple_points_expansion() {
        let mut bbox = BoundingBox::EMPTY;
        bbox.expand_point([0.0, 10.0, -5.0]);
        bbox.expand_point([5.0, 2.0, 8.0]);

        assert_eq!(bbox.min, [0.0, 2.0, -5.0]);
        assert_eq!(bbox.max, [5.0, 10.0, 8.0]);
    }

    /// Verifies that center and center_f32 compute the exact midpoints of the bounding box coordinates.
    #[wasm_bindgen_test]
    fn bbox_center_f64_and_f32() {
        let bbox = BoundingBox::new([-10.0, -20.0, -30.0], [10.0, 20.0, 30.0]);

        assert_eq!(bbox.center(), [0.0, 0.0, 0.0]);
        assert_eq!(bbox.center_f32(), glam::Vec3::ZERO);
    }

    /// Verifies that size, size_x, size_y, size_z, and max_extent compute accurate bounding dimensions.
    #[wasm_bindgen_test]
    fn bbox_dimensions_and_extents() {
        let bbox = BoundingBox::new([1.0, 2.0, 3.0], [5.0, 10.0, 7.0]);

        assert_eq!(bbox.size(), [4.0, 8.0, 4.0]);
        assert_eq!(bbox.size_x(), 4.0);
        assert_eq!(bbox.size_y(), 8.0);
        assert_eq!(bbox.size_z(), 4.0);
        assert_eq!(bbox.max_extent(), 8.0);
        assert_eq!(bbox.max_extent_f32(), 8.0);
    }

    /// Verifies that bounding boxes containing NaN or infinite values fail validity checks.
    #[wasm_bindgen_test]
    fn bbox_invalid_nan_infinity() {
        let nan_bbox = BoundingBox::new([f64::NAN, 0.0, 0.0], [1.0, 1.0, 1.0]);
        assert!(!nan_bbox.is_valid());

        let inf_bbox = BoundingBox::new([f64::NEG_INFINITY, 0.0, 0.0], [1.0, 1.0, 1.0]);
        assert!(!inf_bbox.is_valid());
    }

    /// Verifies that bounding boxes where min > max along any axis are recognized as invalid.
    #[wasm_bindgen_test]
    fn bbox_inverted_min_max() {
        let inverted = BoundingBox::new([10.0, 0.0, 0.0], [5.0, 0.0, 0.0]);
        assert!(!inverted.is_valid());
    }

    /// Verifies that total_vertices and total_triangles compute the accurate aggregate sum across all parts.
    #[wasm_bindgen_test]
    fn step_model_vertex_triangle_sums() {
        use crate::common::render::GpuVertex;

        let part1 = RenderablePart {
            vertices: (0..30)
                .map(|_| GpuVertex {
                    position: [0.0, 0.0, 0.0],
                    normal: [0.0, 1.0, 0.0],
                })
                .collect(),
            indices: (0..30).collect(), // 30 indices = 10 triangles
            model_matrix: glam::Mat4::IDENTITY,
            color: [1.0, 1.0, 1.0, 1.0],
        };

        let part2 = RenderablePart {
            vertices: (0..12)
                .map(|_| GpuVertex {
                    position: [0.0, 0.0, 0.0],
                    normal: [0.0, 1.0, 0.0],
                })
                .collect(),
            indices: (0..12).collect(), // 12 indices = 4 triangles
            model_matrix: glam::Mat4::IDENTITY,
            color: [1.0, 1.0, 1.0, 1.0],
        };

        let model = StepModel {
            id: FileId::from("model_test"),
            metadata: Metadata {
                header: StepHeader {
                    file_description: "test".to_string(),
                    implementation_level: "2;1".to_string(),
                    file_name: "test.step".to_string(),
                    time_stamp: "2026-09-01T00:00:00".to_string(),
                    author: vec![],
                    organization: vec![],
                    preprocessor_version: "1.0".to_string(),
                    originating_system: "sys".to_string(),
                    authorization: "none".to_string(),
                    file_schema: "AP203".to_string(),
                },
                entity_count: 0,
                bounding_box: None,
                units: None,
                vertex_count: 0,
                triangle_count: 0,
                volume: None,
                surface_area: None,
            },
            render_parts: vec![part1, part2],
            part_visibility: vec![],
            visibility_generation: 0,
            cached_bounds: None,
        };

        assert_eq!(model.total_vertices(), 42);
        assert_eq!(model.total_triangles(), 14);
    }

    /// Verifies that calculate_total_volume and calculate_total_surface_area aggregate correctly across render parts.
    #[wasm_bindgen_test]
    fn step_model_volume_area_sums() {
        use crate::common::render::GpuVertex;

        let part1 = RenderablePart {
            vertices: vec![
                GpuVertex {
                    position: [0.0, 0.0, 0.0],
                    normal: [0.0, 0.0, 1.0],
                },
                GpuVertex {
                    position: [1.0, 0.0, 0.0],
                    normal: [0.0, 0.0, 1.0],
                },
                GpuVertex {
                    position: [0.0, 1.0, 0.0],
                    normal: [0.0, 0.0, 1.0],
                },
            ],
            indices: vec![0, 1, 2],
            model_matrix: glam::Mat4::IDENTITY,
            color: [1.0, 1.0, 1.0, 1.0],
        };

        let part2 = RenderablePart {
            vertices: vec![
                GpuVertex {
                    position: [0.0, 0.0, 0.0],
                    normal: [0.0, 0.0, 1.0],
                },
                GpuVertex {
                    position: [2.0, 0.0, 0.0],
                    normal: [0.0, 0.0, 1.0],
                },
                GpuVertex {
                    position: [0.0, 2.0, 0.0],
                    normal: [0.0, 0.0, 1.0],
                },
            ],
            indices: vec![0, 1, 2],
            model_matrix: glam::Mat4::IDENTITY,
            color: [1.0, 1.0, 1.0, 1.0],
        };

        let part1_vol = part1.calculate_volume();
        let part2_vol = part2.calculate_volume();
        let part1_area = part1.calculate_surface_area();
        let part2_area = part2.calculate_surface_area();

        let model = StepModel {
            id: FileId::from("model_test"),
            metadata: Metadata {
                header: StepHeader {
                    file_description: "test".to_string(),
                    implementation_level: "2;1".to_string(),
                    file_name: "test.step".to_string(),
                    time_stamp: "2026-09-01T00:00:00".to_string(),
                    author: vec![],
                    organization: vec![],
                    preprocessor_version: "1.0".to_string(),
                    originating_system: "sys".to_string(),
                    authorization: "none".to_string(),
                    file_schema: "AP203".to_string(),
                },
                entity_count: 0,
                bounding_box: None,
                units: None,
                vertex_count: 0,
                triangle_count: 0,
                volume: None,
                surface_area: None,
            },
            render_parts: vec![part1, part2],
            part_visibility: vec![],
            visibility_generation: 0,
            cached_bounds: None,
        };

        approx::assert_relative_eq!(
            model.calculate_total_volume(),
            part1_vol + part2_vol,
            epsilon = 1e-6
        );
        approx::assert_relative_eq!(
            model.calculate_total_surface_area(),
            part1_area + part2_area,
            epsilon = 1e-6
        );
    }

    /// Verifies that empty StepModels evaluate cleanly to zero vertices, triangles, volume, and surface area.
    #[wasm_bindgen_test]
    fn step_model_empty_parts() {
        let model = StepModel {
            id: FileId::from("empty_model"),
            metadata: Metadata {
                header: StepHeader {
                    file_description: "test".to_string(),
                    implementation_level: "2;1".to_string(),
                    file_name: "empty.step".to_string(),
                    time_stamp: "2026-09-01T00:00:00".to_string(),
                    author: vec![],
                    organization: vec![],
                    preprocessor_version: "1.0".to_string(),
                    originating_system: "sys".to_string(),
                    authorization: "none".to_string(),
                    file_schema: "AP203".to_string(),
                },
                entity_count: 0,
                bounding_box: None,
                units: None,
                vertex_count: 0,
                triangle_count: 0,
                volume: None,
                surface_area: None,
            },
            render_parts: vec![],
            part_visibility: vec![],
            visibility_generation: 0,
            cached_bounds: None,
        };

        assert_eq!(model.total_vertices(), 0);
        assert_eq!(model.total_triangles(), 0);
        assert_eq!(model.calculate_total_volume(), 0.0);
        assert_eq!(model.calculate_total_surface_area(), 0.0);
    }
}
