//! STEP header/metadata extraction on top of ruststep's AST.
use super::logger;
use crate::common::exchange_index::ExchangeIndex;
use crate::common::utils::find_ignore_ascii_case;
use crate::error::StepError;
use crate::ruststep::ast::{DataSection, EntityInstance, Exchange, Record};
use crate::ruststep::header::{FileSchema, Header};
use crate::storage::hash_text_to_id;
use crate::trace_span;
use serde::{Deserialize, Serialize};

use super::types::{BoundingBox, FileId, LengthUnit, Metadata, StepHeader};

/// Supported STEP schemas recognized by the visualizer and geometry pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum StepSchema {
    /// ISO 10303-201: Explicit Draughting (corresponding to `ruststep::ap201::explicit_draughting`).
    Ap201,
    /// ISO 10303-203: Configuration Controlled 3D Design (corresponding to `ruststep::ap203::config_control_design`).
    Ap203,
    /// ISO 10303-214: Core Data for Automotive Mechanical Design Processes.
    Ap214,
}

impl StepSchema {
    /// Matches exact STEP schema identifiers and standard ASN.1 object identifier prefixes.
    pub fn parse(identifier: &str) -> Option<Self> {
        let clean = identifier.trim().trim_matches('\'').trim_matches('"');
        let upper = clean.to_ascii_uppercase();

        if upper == "CONFIG_CONTROL_DESIGN"
            || upper.starts_with("CONFIG_CONTROL_DESIGN ")
            || upper == "AP203"
            || upper == "AP203_E2"
            || upper.starts_with("CONFIGURATION_CONTROL_3D_DESIGN")
        {
            Some(Self::Ap203)
        } else if upper == "AUTOMOTIVE_DESIGN"
            || upper.starts_with("AUTOMOTIVE_DESIGN ")
            || upper == "AP214"
        {
            Some(Self::Ap214)
        } else if upper == "EXPLICIT_DRAUGHTING"
            || upper.starts_with("EXPLICIT_DRAUGHTING ")
            || upper == "AP201"
        {
            Some(Self::Ap201)
        } else {
            None
        }
    }

    /// Primary standard schema name string for display or comparison.
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Ap201 => "AP201",
            Self::Ap203 => "AP203",
            Self::Ap214 => "AP214",
        }
    }
}

/// Validates that at least one schema listed in the STEP header [`FileSchema`] is supported.
pub fn validate_schema(file_schema: &FileSchema) -> Result<StepSchema, StepError> {
    for id in &file_schema.schema {
        if let Some(schema) = StepSchema::parse(id) {
            return Ok(schema);
        }
    }
    let raw = if file_schema.schema.is_empty() {
        "UNKNOWN".to_string()
    } else {
        file_schema.schema.join(", ")
    };
    Err(StepError::UnsupportedSchema { schema: raw })
}

/// Pre-checks an in-memory STEP buffer for the `ISO-10303-21` header marker and validates
/// that `FILE_SCHEMA` specifies a supported application protocol (AP201, AP203, or AP214)
/// before executing the full AST tokenizer and parser.
pub fn probe_validate_step_buffer(text: &str) -> Result<StepSchema, StepError> {
    trace_span!("probe_validate_step_buffer");

    // 1. Verify ISO-10303-21 exchange structure prefix (handling optional BOM and comments)
    let clean = text.trim_start_matches('\u{feff}').trim_start();
    let mut cursor = clean;
    while cursor.starts_with("/*") {
        if let Some(end) = cursor.find("*/") {
            cursor = cursor[end + 2..].trim_start();
        } else {
            break;
        }
    }

    if !cursor.starts_with("ISO-10303-21") {
        return Err(StepError::Parse(
            "Missing ISO-10303-21 exchange structure header".to_string(),
        ));
    }

    // 2. Locate FILE_SCHEMA in the header chunk (before DATA; if present, or up to 64KB)
    let search_limit = text.find("DATA;").unwrap_or(text.len().min(65536));
    let header_chunk = &text[..search_limit];

    let schema_kw_pos = find_ignore_ascii_case(header_chunk, "FILE_SCHEMA").ok_or_else(|| {
        StepError::InvalidHeader("Missing FILE_SCHEMA declaration in header".to_string())
    })?;

    let remainder = &header_chunk[schema_kw_pos + "FILE_SCHEMA".len()..];
    let semi_pos = remainder.find(';').ok_or_else(|| {
        StepError::InvalidHeader("Unterminated FILE_SCHEMA declaration".to_string())
    })?;
    let stmt = &remainder[..semi_pos];

    // Extract all strings enclosed in quotes within the FILE_SCHEMA statement:
    // e.g. FILE_SCHEMA(('CONFIG_CONTROL_DESIGN'));
    //      FILE_SCHEMA (('AUTOMOTIVE_DESIGN {1 0 10303 214 3 1 1}'));
    //      FILE_SCHEMA (( 'CONFIG_CONTROL_DESIGN' ));

    // below we check even for p21 (multi steps file format)
    // but for now we don't support the multi step logic ... yet
    let mut raw_schemas = Vec::new();
    let mut curr = stmt;
    while let Some(start) = curr.find('\'') {
        let after_start = &curr[start + 1..];
        if let Some(end) = after_start.find('\'') {
            let schema_token = &after_start[..end];
            raw_schemas.push(schema_token);
            curr = &after_start[end + 1..];
        } else {
            break;
        }
    }

    if raw_schemas.is_empty() {
        return Err(StepError::InvalidHeader(
            "FILE_SCHEMA contains no schema identifiers".to_string(),
        ));
    }

    for raw in &raw_schemas {
        if let Some(schema) = StepSchema::parse(raw) {
            return Ok(schema);
        }
    }

    Err(StepError::UnsupportedSchema {
        schema: raw_schemas.join(", "),
    })
}

/// Helper to convert a typed [`Header`] into a display-oriented [`StepHeader`].
pub fn convert_header_from_ast(header: &Header) -> StepHeader {
    StepHeader {
        file_description: header.file_description.description.join("; ").into(),
        implementation_level: header.file_description.implementation_level.clone().into(),
        file_name: header.file_name.name.clone().into(),
        time_stamp: header.file_name.time_stamp.clone().into(),
        author: header.file_name.author.iter().map(|s| s.into()).collect(),
        organization: header
            .file_name
            .organization
            .iter()
            .map(|s| s.into())
            .collect(),
        preprocessor_version: header.file_name.preprocessor_version.clone().into(),
        originating_system: header.file_name.originating_system.clone().into(),
        authorization: header.file_name.authorization.clone().into(),
        file_schema: header.file_schema.schema.join(", ").into(),
    }
}

/// Convert the STEP header section into the display-oriented [`StepHeader`].
/// Fails when the records do not form a valid header.
pub fn convert_header(header_in: &[Record]) -> Result<StepHeader, StepError> {
    trace_span!("convert_header");
    if header_in.len() < 3 {
        return Err(StepError::InvalidHeader(
            "Header section must contain at least 3 records".to_string(),
        ));
    }
    let header_obj =
        Header::from_records(header_in).map_err(|e| StepError::InvalidHeader(e.to_string()))?;
    Ok(convert_header_from_ast(&header_obj))
}

/// Parse unit system (e.g. `LengthUnit::Millimetre`) from the exchange structure.
///
/// In the hot path, prefer [`ExchangeIndex::build`] which resolves the unit as part of its
/// single combined pass. This function is retained for tests and call sites that do not
/// have a pre-built index available.
pub fn parse_units(exchange: &Exchange) -> Option<LengthUnit> {
    trace_span!("parse_units");
    let mut ex = exchange.clone();
    ExchangeIndex::build(&mut ex).resolved_unit()
}

/// Axis-aligned bounds over all `CARTESIAN_POINT`s in the tables. Placement
/// transforms are ignored, so this is an approximation — good enough for a
/// pre-load size preview. `None` when tables have no points.
pub fn compute_bounding_box(step_tables: &[truck_stepio::r#in::Table]) -> Option<BoundingBox> {
    trace_span!("compute_bounding_box");
    let mut bbox = BoundingBox::EMPTY;

    for step_table in step_tables {
        for value in step_table.cartesian_point.values() {
            let coords = &value.coordinates;
            if coords.len() >= 3 {
                bbox.expand_point(glam::DVec3::new(coords[0], coords[1], coords[2]));
            }
        }
    }
    bbox.is_valid().then_some(bbox)
}

/// Normalises STEP entity records in-place before loading into `truck_stepio::Table`.
///
/// **Deprecated hot-path**: in the hot path this is done inside [`ExchangeIndex::build`].
/// This wrapper is retained for tests that only have a parsed `&mut Exchange`.
///
/// Renames `INTERSECTION_CURVE` and `BOUNDARY_CURVE` → `SURFACE_CURVE` so that
/// `truck_stepio` can parse them into `table.surface_curve`.
pub fn normalize_exchange(exchange: &mut Exchange) {
    trace_span!("normalize_exchange");
    for section in &mut exchange.data {
        for entity in &mut section.entities {
            let EntityInstance::Simple { record, .. } = entity else {
                continue;
            };

            let name = record.name.as_str();
            if (name.eq_ignore_ascii_case("INTERSECTION_CURVE")
                || name.eq_ignore_ascii_case("BOUNDARY_CURVE"))
                && name != "SURFACE_CURVE"
            {
                record.name.clear();
                record.name.push_str("SURFACE_CURVE");
            }
        }
    }
}

/// Returns all data sections carrying usable STEP content, or a
/// domain error explaining why the file has none.
pub fn all_usable_sections(parsed: &Exchange) -> Result<Vec<&DataSection>, StepError> {
    let usable: Vec<&DataSection> = parsed
        .data
        .iter()
        .filter(|s| !s.entities.is_empty() || !s.meta.is_empty())
        .collect();
    if usable.is_empty() {
        Err(StepError::EmptyDataSection)
    } else {
        if parsed.data.len() > 1 {
            logger::warn(&format!(
                "STEP file contains {} DATA sections; processing all {} usable sections",
                parsed.data.len(),
                usable.len()
            ));
        }
        Ok(usable)
    }
}

/// Extracts the typed [`StepHeader`] and total entity count from a parsed STEP AST exchange structure.
pub fn extract_header_and_count(
    fallback_name: &str,
    parsed: &Exchange,
) -> Result<(StepHeader, usize), StepError> {
    if parsed.header.len() < 3 {
        return Err(StepError::InvalidHeader(
            "Header section must contain at least 3 records".to_string(),
        ));
    }
    let header_obj = Header::from_records(&parsed.header)
        .map_err(|e| StepError::InvalidHeader(e.to_string()))?;
    validate_schema(&header_obj.file_schema)?;

    let entity_count: usize = parsed
        .data
        .iter()
        .map(|section| section.entities.len())
        .sum();
    let mut step_header = convert_header_from_ast(&header_obj);
    if step_header.file_name.is_empty() {
        step_header.file_name = fallback_name.into();
    }
    Ok((step_header, entity_count))
}

/// Assembles the pre-tessellation metadata (header, entity count, bounding
/// box, units) for a parsed STEP file, together with its content-hash id.
/// The tessellated counts (vertices/triangles) are filled in later, once
/// the geometry pass has produced them.
///
/// `units` should be pre-resolved from [`ExchangeIndex::resolved_unit`] in the hot path
/// so that no additional AST scan is required.
pub fn build_initial_metadata(
    fallback_name: &str,
    parsed: &Exchange,
    step_tables: &[truck_stepio::r#in::Table],
    text: &str,
    units: Option<LengthUnit>,
) -> Result<(Metadata, FileId), StepError> {
    trace_span!("build_initial_metadata");
    let (step_header, entity_count) = extract_header_and_count(fallback_name, parsed)?;

    let meta = Metadata {
        header: step_header,
        entity_count,
        bounding_box: compute_bounding_box(step_tables),
        units,
        vertex_count: 0,
        triangle_count: 0,
        volume: None,
        surface_area: None,
    };
    Ok((meta, hash_text_to_id(text)))
}
