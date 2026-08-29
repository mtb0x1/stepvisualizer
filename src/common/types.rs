use serde::{Deserialize, Serialize};

use super::render::RenderablePart;

/// Strongly-typed file content hash ID.
///
/// `repr(transparent)` ensures zero-cost runtime wrapping over `String`
/// and transparent serde serialization (serialized as a flat string in JSON/localStorage).
#[repr(transparent)]
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize, Deserialize)]
pub struct FileId(pub String);

#[allow(dead_code)]
impl FileId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for FileId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
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
    pub units: Option<String>,
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

#[allow(dead_code)]
impl BoundingBox {
    /// An empty/inverted bounding box ready to be expanded.
    pub const EMPTY: Self = Self {
        min: [f64::INFINITY; 3],
        max: [f64::NEG_INFINITY; 3],
    };

    /// Create a bounding box with the given min and max coordinates.
    pub fn new(min: [f64; 3], max: [f64; 3]) -> Self {
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
    pub fn center(&self) -> [f64; 3] {
        [
            (self.min[0] + self.max[0]) * 0.5,
            (self.min[1] + self.max[1]) * 0.5,
            (self.min[2] + self.max[2]) * 0.5,
        ]
    }

    /// Center point converted to `[f32; 3]` for GPU and camera framing.
    pub fn center_f32(&self) -> [f32; 3] {
        let c = self.center();
        [c[0] as f32, c[1] as f32, c[2] as f32]
    }

    /// Dimensions (width, height, depth) as f64.
    pub fn size(&self) -> [f64; 3] {
        [
            (self.max[0] - self.min[0]).max(0.0),
            (self.max[1] - self.min[1]).max(0.0),
            (self.max[2] - self.min[2]).max(0.0),
        ]
    }

    /// Size along the X axis.
    pub fn size_x(&self) -> f64 {
        (self.max[0] - self.min[0]).max(0.0)
    }

    /// Size along the Y axis.
    pub fn size_y(&self) -> f64 {
        (self.max[1] - self.min[1]).max(0.0)
    }

    /// Size along the Z axis.
    pub fn size_z(&self) -> f64 {
        (self.max[2] - self.min[2]).max(0.0)
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

    /// Expands this bounding box to include another bounding box.
    pub fn expand_box(&mut self, other: &BoundingBox) {
        for i in 0..3 {
            self.min[i] = self.min[i].min(other.min[i]);
            self.max[i] = self.max[i].max(other.max[i]);
        }
    }
}

/// A fully processed STEP file: identity, metadata, tessellated parts, and
/// per-part visibility. The whole model is serialized into localStorage
/// under its `id`, so it survives reloads without re-parsing.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct StepModel {
    pub id: FileId,
    pub metadata: Metadata,
    pub render_parts: Vec<RenderablePart>,
    #[serde(default)]
    pub part_visibility: Vec<bool>,
}

