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

/// Axis-aligned bounds in model coordinates.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct BoundingBox {
    pub min: [f64; 3],
    pub max: [f64; 3],
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

