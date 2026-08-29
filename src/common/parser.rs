//! STEP header/metadata extraction on top of ruststep's AST.
use crate::{error::StepVizError, trace_span};
use ruststep::ast::{EntityInstance, Exchange, Parameter, Record};
use ruststep::header::Header;

use super::types::{BoundingBox, LengthUnit, StepHeader};

/// Convert the STEP header section into the display-oriented [`StepHeader`].
/// Fails when the records do not form a valid header.
pub fn convert_header(header_in: &[Record]) -> Result<StepHeader, StepVizError> {
    trace_span!("convert_header");
    let header_in: Header = Header::from_records(header_in)
        .map_err(|e| StepVizError::InvalidHeader(e.to_string()))?;
    let file_description = header_in.file_description.description;
    Ok(StepHeader {
        file_description: file_description.join("; "),
        implementation_level: header_in.file_description.implementation_level,
        file_name: header_in.file_name.name,
        time_stamp: header_in.file_name.time_stamp,
        author: header_in.file_name.author,
        organization: header_in.file_name.organization,
        preprocessor_version: header_in.file_name.preprocessor_version,
        originating_system: header_in.file_name.originating_system,
        authorization: header_in.file_name.authorization,
        file_schema: header_in.file_schema.schema.join(", "),
    })
}

/// Parse unit system (e.g. `LengthUnit::Millimetre`) from the exchange structure.
///
/// Units are declared as `(LENGTH_UNIT()NAMED_UNIT(*)SI_UNIT(.MILLI.,.METRE.))`
/// (a Complex entity) alongside sibling `PLANE_ANGLE_UNIT`/`SOLID_ANGLE_UNIT`
/// forms. We must prefer the length unit: a naive "first SI_UNIT wins" scan
/// returns the angle unit because it appears earlier in the file.
pub fn parse_units(exchange: &Exchange) -> Option<LengthUnit> {
    trace_span!("parse_units");
    let mut fallback: Option<LengthUnit> = None;
    for section in &exchange.data {
        for entity in &section.entities {
            match entity {
                EntityInstance::Simple { record, .. } => {
                    if fallback.is_none() {
                        fallback = unit_from_record(record);
                    }
                }
                EntityInstance::Complex { subsuper, .. } => {
                    let is_length = subsuper
                        .0
                        .iter()
                        .any(|r| r.name.eq_ignore_ascii_case("LENGTH_UNIT"));
                    if is_length {
                        if let Some(unit) = unit_from_subsuper(&subsuper.0) {
                            return Some(unit);
                        }
                    } else if fallback.is_none() {
                        fallback = unit_from_subsuper(&subsuper.0);
                    }
                }
            }
        }
    }
    fallback
}

/// Axis-aligned bounds over all `CARTESIAN_POINT`s in the table. Placement
/// transforms are ignored, so this is an approximation — good enough for a
/// pre-load size preview. `None` when the table has no points.
pub fn compute_bounding_box(step_table: &truck_stepio::r#in::Table) -> Option<BoundingBox> {
    trace_span!("compute_bounding_box");
    let mut bbox = BoundingBox::EMPTY;

    for value in step_table.cartesian_point.values() {
        let coords = &value.coordinates;
        if coords.len() >= 3 {
            bbox.expand_point([coords[0], coords[1], coords[2]]);
        }
    }
    bbox.is_valid().then_some(bbox)
}

fn unit_from_subsuper(records: &[Record]) -> Option<LengthUnit> {
    records.iter().find_map(unit_from_record)
}

fn param_as_list(param: &Parameter) -> Option<&[Parameter]> {
    match param {
        Parameter::List(list) => Some(list),
        _ => None,
    }
}

fn param_as_enum(param: &Parameter) -> Option<&str> {
    match param {
        Parameter::Enumeration(value) => Some(value.as_str()),
        _ => None,
    }
}

fn unit_from_record(record: &Record) -> Option<LengthUnit> {
    if !record.name.eq_ignore_ascii_case("SI_UNIT") {
        return None;
    }

    let params = param_as_list(&record.parameter)?;
    let unit = params.get(1).and_then(param_as_enum)?;
    let prefix = params.first().and_then(param_as_enum);

    LengthUnit::from_si_spec(unit, prefix)
}
