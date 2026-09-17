//! STEP header/metadata extraction on top of ruststep's AST.
use crate::common::ast_helpers::ParameterExt;
use crate::common::exchange_index::ExchangeIndex;
use crate::common::fast_hash::FastU64Map;
use crate::common::logger;
use crate::common::utils::find_ignore_ascii_case;
use crate::error::StepError;
use crate::ruststep::ast::{DataSection, EntityInstance, Exchange, Name, Parameter, Record};
use crate::ruststep::header::{FileSchema, Header};
use crate::storage::hash_text_to_id;
use crate::trace_span;
use glam::DVec3;
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

use crate::common::types::{BoundingBox, FileId, LengthUnit, Metadata, StepHeader};

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
/// This pass performs two essential AST sanitizations:
/// 1. Renames `INTERSECTION_CURVE` and `BOUNDARY_CURVE` → `SURFACE_CURVE` so that
///    `truck_stepio` can parse them into `table.surface_curve`.
/// 2. Sanitizes `AXIS2_PLACEMENT_3D` records whose `ref_direction` is omitted (`$`) and
///    whose `axis` is collinear with the global X-axis, preventing a catastrophic $\frac{0}{0} = \text{NaN}$
///    crash in `truck-stepio`'s Gram-Schmidt orthonormalization.
// TODO : better way to do this, it defies the index building or it feels like it.
pub fn normalize_exchange(exchange: &mut Exchange) {
    trace_span!("normalize_exchange");
    for section in &mut exchange.data {
        normalize_curve_subtypes(section);
        sanitize_axis2_placement_3d(section);
    }
}

/// Normalises `INTERSECTION_CURVE` and `BOUNDARY_CURVE` entity names in-place to `SURFACE_CURVE`.
// TODO : avoid eq_ignore_ascii_case("INTERSECTION_CURVE") use Kind
// TODO : better way to do this ? unsafe /faster ?
fn normalize_curve_subtypes(section: &mut DataSection) {
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

/// Extracts the 3D direction vector `[dx, dy, dz]` from a `DIRECTION` entity record as `DVec3`.
fn extract_direction_coords(record: &Record) -> Option<DVec3> {
    record
        .parameter
        .try_extract::<&[Parameter]>()?
        .get(1)?
        .try_extract::<DVec3>()
}

/// Tests whether a direction vector is approximately the positive Z unit vector `(0, 0, 1)`.
fn is_unit_z_direction(v: DVec3) -> bool {
    let norm = v.normalize_or_zero();
    (norm - DVec3::Z).length_squared() < 1e-8
}

/// Tests whether a direction vector is collinear or antiparallel with the global X-axis `(1, 0, 0)`.
///
/// Uses the normalized squared cross product with `(1, 0, 0)`:
/// $$\sin^2(\theta) = \frac{||\vec{v} \times \hat{x}||^2}{||\vec{v}||^2}$$
fn is_collinear_with_x(v: DVec3) -> bool {
    let len_sq = v.length_squared();
    if len_sq < 1e-12 {
        return false;
    }
    v.cross(DVec3::X).length_squared() / len_sq < 1e-4
}

/// Sanitizes `AXIS2_PLACEMENT_3D` entities in-place within a STEP data section.
///
/// # Problem & Specification Rationale (ISO 10303-42 vs. `truck-stepio`)
///
/// In ISO 10303-42 (Geometric and topological representation), `AXIS2_PLACEMENT_3D` defines
/// a 3D coordinate system:
/// ```text
/// ENTITY axis2_placement_3d SUBTYPE OF (placement);
///   axis : OPTIONAL direction;
///   ref_direction : OPTIONAL direction;
/// WHERE
///   WR1: (NOT EXISTS(axis)) OR (NOT EXISTS(ref_direction)) OR
///        (cross_product(axis, ref_direction).magnitude > 0.0);
/// END_ENTITY;
/// ```
///
/// Per ISO 10303-42 Section 4.4.28:
/// > *"If the attribute ref_direction is omitted, the direction of the x axis is
/// > arbitrary, but shall be orthogonal to axis."*
///
/// Standard-compliant CAD systems (notably CATIA V5) routinely omit `ref_direction`
/// on circular and cylindrical features (e.g. circles, cylinders, holes, fillets) whenever the
/// orientation around the rotation axis is geometrically arbitrary.
///
/// However, `truck-stepio` (v0.3.0) implements `From<&Axis2Placement3d> for Matrix4` using
/// a hardcoded fallback to global unit X:
/// ```text
/// let z = match &axis.axis {
///     Some(axis) => Vector3::from(axis),
///     None => Vector3::unit_z(),
/// };
/// let x = match &axis.ref_direction {
///     Some(axis) => Vector3::from(axis),
///     None => Vector3::unit_x(), // <--- Collinear singularity when z || unit_x!
/// };
/// let x = (x - x.dot(z) * z).normalize();
/// let y = z.cross(x);
/// ```
///
/// When `axis` ($\vec{z}$) is collinear with the global X-axis (e.g. `(1.0, 0.0, 0.0)` or
/// `(-1.0, 0.0, 0.0)`) and `ref_direction` is omitted:
/// 1. $x - (x \cdot z) z = (1, 0, 0) - (\pm 1)(\pm 1, 0, 0) = (0, 0, 0)$.
/// 2. `(0, 0, 0).normalize()` attempts to divide by zero: `0.0 / 0.0 = NaN`.
/// 3. `y = z.cross(x)` becomes `NaN`.
/// 4. The resulting `Matrix4` coordinate frame is corrupted with `NaN` elements.
/// 5. Downstream in `truck-meshalgo` during `cshell.triangulation(tolerance)`, each edge
///    curve (e.g. `CIRCLE`, `ELLIPSE`) or surface (e.g. `TOROIDAL_SURFACE`) is tessellated
///    via `Processor::parameter_division`.
/// 6. `Processor` computes its spatial scaling factor $n$ via an Iwasawa decomposition on the
///    `Matrix4`. Because the matrix elements are `NaN`, $n$ evaluates to `NaN`.
/// 7. The effective tolerance passed to `UnitCircle::parameter_division` is
///    `tolerance / n = tolerance / NaN = NaN`.
/// 8. In `truck-geometry-0.5.0/src/specifieds/circle.rs:51`, `nonpositive_tolerance!(tol)`
///    executes `assert!(tol >= 1.0e-6)`.
/// 9. In IEEE-754 floating-point arithmetic, any comparison with `NaN` evaluates to `false`
///    (`NaN >= 1.0e-6` is `false`).
/// 10. The assertion panics with: `"tolerance must be no less than 1e-6"`, terminating the
///     entire WebAssembly thread / async task and crashing the visualizer.
///
/// # Mathematical Resolution
///
/// To eliminate the $\frac{0}{0} = \text{NaN}$ singularity, this function ensures that any
/// `AXIS2_PLACEMENT_3D` whose `ref_direction` is omitted and whose `axis` is collinear with
/// $(1, 0, 0)$ is explicitly assigned an orthogonal reference direction $\hat{u}_z = (0.0, 0.0, 1.0)$:
/// - If a `DIRECTION` pointing along $(0.0, 0.0, 1.0)$ already exists in the section, its entity
///   ID is reused.
/// - Otherwise, a synthetic `DIRECTION('synthetic_ref_z', (0.0, 0.0, 1.0))` entity is appended to
///   the data section.
/// - The `AXIS2_PLACEMENT_3D` record's parameter list is updated to reference this `DIRECTION`.
///
/// This satisfies ISO 10303-42 requirement WR1, ensures `(x - x.dot(z) * z).normalize()` safely
/// produces $(0.0, 0.0, 1.0)$, guarantees an orthonormal coordinate frame without `NaN`s, and
/// allows `truck-meshalgo` to tessellate all shells and features successfully.
// TODO : avoid eq_ignore_ascii_case("DIRECTION") / "synthetic_ref_z" use Kind
// TODO : better way to do this ? unsafe /faster ?
pub fn sanitize_axis2_placement_3d(section: &mut DataSection) {
    let mut max_id: u64 = 0;
    let mut direction_map: FastU64Map<DVec3> = FastU64Map::default();
    let mut existing_unit_z: Option<u64> = None;

    // Pass 1: Index existing DIRECTION entities and find the maximum entity ID.
    for entity in &section.entities {
        match entity {
            EntityInstance::Simple { id, record } => {
                max_id = max_id.max(*id);
                if record.name.eq_ignore_ascii_case("DIRECTION")
                    && let Some(coords) = extract_direction_coords(record)
                {
                    if existing_unit_z.is_none() && is_unit_z_direction(coords) {
                        existing_unit_z = Some(*id);
                    }
                    direction_map.insert(*id, coords);
                }
            }
            EntityInstance::Complex { id, .. } => {
                max_id = max_id.max(*id);
            }
        }
    }

    let mut synthetic_directions: SmallVec<[EntityInstance; 1]> = SmallVec::new();

    // Pass 2: Sanitize AXIS2_PLACEMENT_3D entities whose axis is collinear with X.
    for entity in &mut section.entities {
        let EntityInstance::Simple { record, .. } = entity else {
            continue;
        };
        if !record.name.eq_ignore_ascii_case("AXIS2_PLACEMENT_3D") {
            continue;
        }
        let Parameter::List(ref mut params) = record.parameter else {
            continue;
        };

        // AXIS2_PLACEMENT_3D signature: (name, location, [axis], [ref_direction])
        // ref_direction is omitted if params.len() == 3 or params[3] is NotProvided.
        let ref_dir_omitted =
            params.len() == 3 || (params.len() >= 4 && matches!(params[3], Parameter::NotProvided));
        if !ref_dir_omitted {
            continue;
        }

        let Some(axis_id) = params.get(2).and_then(|p| p.try_extract::<u64>()) else {
            continue;
        };

        let Some(&axis_dir) = direction_map.get(&axis_id) else {
            continue;
        };

        if !is_collinear_with_x(axis_dir) {
            continue;
        }

        // axis_dir is collinear with X: supply an orthogonal unit Z reference direction.
        let target_ref_id = match existing_unit_z {
            Some(id) => id,
            None => {
                let synthetic_id = max_id + 1;
                max_id += 1;
                existing_unit_z = Some(synthetic_id);
                synthetic_directions.push(EntityInstance::Simple {
                    id: synthetic_id,
                    record: Record {
                        name: "DIRECTION".to_string(),
                        parameter: Parameter::List(vec![
                            Parameter::String("synthetic_ref_z".to_string()),
                            Parameter::List(vec![
                                Parameter::Real(0.0),
                                Parameter::Real(0.0),
                                Parameter::Real(1.0),
                            ]),
                        ]),
                    },
                });
                synthetic_id
            }
        };

        let ref_param = Parameter::Ref(Name::Entity(target_ref_id));
        if params.len() >= 4 {
            params[3] = ref_param;
        } else {
            params.push(ref_param);
        }
    }

    if !synthetic_directions.is_empty() {
        section.entities.extend(synthetic_directions);
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
