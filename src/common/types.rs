use serde::{Deserialize, Serialize};

use super::render::RenderablePart;



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
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct FileIndexItem {
    pub id: String,
    pub name: String,
    pub entity_count: usize,
    pub time_stamp: String,
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct BoundingBox {
    pub min: [f64; 3],
    pub max: [f64; 3],
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct StepModel {
    pub id: String,
    pub metadata: Metadata,
    pub render_parts: Vec<RenderablePart>,
}
