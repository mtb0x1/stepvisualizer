use serde::{Deserialize, Serialize};
use smallvec::SmallVec;
use smol_str::{SmolStr, format_smolstr};

use crate::common::render::RenderablePart;
use crate::common::utils::clean_unit_name;
use glam::DVec3;

/// Strongly-typed file content hash ID.
///
/// `repr(transparent)` ensures zero-cost runtime wrapping over `String`
/// and transparent serde serialization (serialized as a flat string in JSON/localStorage).
#[repr(transparent)]
#[derive(
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Debug,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct FileId(pub SmolStr);

impl FileId {
    #[inline]
    pub fn as_str(&self) -> &str {
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
        let hash = xxhash_rust::xxh3::xxh3_64(text.as_bytes());
        // 16 hex chars fit perfectly within SmolStr's 23-byte inline capacity
        Self(format_smolstr!("{:016x}", hash))
    }
}

impl From<String> for FileId {
    fn from(s: String) -> Self {
        Self(SmolStr::new(s))
    }
}

impl From<SmolStr> for FileId {
    fn from(s: SmolStr) -> Self {
        Self(s)
    }
}

impl From<&str> for FileId {
    fn from(s: &str) -> Self {
        Self(SmolStr::new(s))
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
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Default,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
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
    #[inline]
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
    #[inline]
    pub const fn is_valid(&self) -> bool {
        self.width > 0 && self.height > 0
    }

    /// Aspect ratio (width / height), or 1.0 when height is zero.
    #[inline]
    pub fn aspect_ratio(&self) -> f64 {
        if self.height == 0 {
            1.0
        } else {
            self.width as f64 / self.height as f64
        }
    }
}

/// Standard length units parsed from STEP SI_UNIT and conversion factors to meters.
#[derive(
    Clone,
    Copy,
    PartialEq,
    Eq,
    Debug,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
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
    #[inline]
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
        let clean = clean_unit_name(name);
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
#[derive(
    Clone,
    PartialEq,
    Debug,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct StepHeader {
    pub file_description: SmolStr,
    pub implementation_level: SmolStr,
    pub file_name: SmolStr,
    pub time_stamp: SmolStr,
    pub author: SmallVec<[SmolStr; 2]>,
    pub organization: SmallVec<[SmolStr; 2]>,
    pub preprocessor_version: SmolStr,
    pub originating_system: SmolStr,
    pub authorization: SmolStr,
    pub file_schema: SmolStr,
}

/// Display metadata for a loaded file: header fields plus derived geometry
/// stats. Persisted inside `StepModel`, so newly added fields need
/// `#[serde(default)]` to stay load-compatible with previously saved models.
#[derive(
    Clone,
    PartialEq,
    Debug,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
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
#[derive(
    Clone,
    PartialEq,
    Debug,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct FileIndexItem {
    pub id: FileId,
    pub name: SmolStr,
    pub entity_count: usize,
    pub time_stamp: SmolStr,
    #[serde(default)]
    pub audit: AuditMetadata,
}

/// Axis-aligned bounds in 3D space.
#[derive(
    Clone,
    Copy,
    PartialEq,
    Debug,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct BoundingBox {
    pub min: DVec3,
    pub max: DVec3,
}

impl BoundingBox {
    /// An empty/inverted bounding box ready to be expanded.
    pub const EMPTY: Self = Self {
        min: DVec3::INFINITY,
        max: DVec3::NEG_INFINITY,
    };

    /// Create a bounding box with the given min and max coordinates.
    #[inline]
    pub const fn new(min: DVec3, max: DVec3) -> Self {
        Self { min, max }
    }

    /// True if the bounding box has valid, finite dimensions.
    #[inline]
    pub fn is_valid(&self) -> bool {
        self.min.is_finite() && self.max.is_finite() && self.min.cmple(self.max).all()
    }

    /// Center point as double-precision `DVec3`.
    #[inline]
    pub fn center(&self) -> glam::DVec3 {
        (self.min + self.max) * 0.5
    }

    /// Dimensions (width, height, depth) as `DVec3`.
    #[inline]
    pub fn size(&self) -> glam::DVec3 {
        (self.max - self.min).max(glam::DVec3::ZERO)
    }

    /// Size along the X axis.
    #[inline]
    pub fn size_x(&self) -> f64 {
        self.size().x
    }

    /// Size along the Y axis.
    #[inline]
    pub fn size_y(&self) -> f64 {
        self.size().y
    }

    /// Size along the Z axis.
    #[inline]
    pub fn size_z(&self) -> f64 {
        self.size().z
    }

    /// Maximum dimension across X, Y, Z as f64.
    #[inline]
    pub fn max_extent(&self) -> f64 {
        self.size().max_element()
    }

    /// Expands this bounding box to include the given `DVec3` point.
    #[inline]
    pub fn expand_point(&mut self, p: glam::DVec3) {
        self.min = self.min.min(p);
        self.max = self.max.max(p);
    }

    /// Expands this bounding box to include another bounding box.
    #[inline]
    pub fn expand_bbox(&mut self, other: Self) {
        self.min = self.min.min(other.min);
        self.max = self.max.max(other.max);
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
#[derive(
    Clone,
    PartialEq,
    Debug,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
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
    #[rkyv(with = rkyv::with::Skip)]
    pub cached_bounds: Option<(u64, BoundingBox)>,
    #[serde(default)]
    pub audit: AuditMetadata,
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

/// Standard audit fields applied to database records.
#[derive(
    Clone,
    PartialEq,
    Debug,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct AuditMetadata {
    #[serde(default = "default_timestamp")]
    pub created_on: f64,
    #[serde(default = "default_timestamp")]
    pub updated_on: f64,
    #[serde(default = "default_user")]
    pub created_by: SmolStr,
    #[serde(default = "default_user")]
    pub updated_by: SmolStr,
}

impl Default for AuditMetadata {
    fn default() -> Self {
        let now = default_timestamp();
        let user = default_user();
        Self {
            created_on: now,
            updated_on: now,
            created_by: user.clone(),
            updated_by: user,
        }
    }
}

fn default_timestamp() -> f64 {
    crate::common::web::now_ms()
}

fn default_user() -> SmolStr {
    SmolStr::new("admin")
}

impl AuditMetadata {
    pub fn mark_updated(&mut self) {
        self.updated_on = default_timestamp();
        self.updated_by = default_user();
    }
}
