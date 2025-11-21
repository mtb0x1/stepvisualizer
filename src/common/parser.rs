use crate::trace_span;
use ruststep::ast::{EntityInstance, Exchange, Parameter, Record};
use ruststep::header::Header;

use super::types::{BoundingBox, StepHeader};

pub fn convert_header(header_in: &[Record]) -> StepHeader {
    trace_span!("convert_header");
    //FIXME : in real life this should always yield a valid Header, but we are not handling errors here
    let header_in: Header = Header::from_records(header_in).expect("failed to parse header");
    let file_description = header_in.file_description.description;
    return StepHeader {
        file_description: file_description.join("; "),
        implementation_level: header_in.file_description.implementation_level,
        file_name: header_in.file_name.name,
        time_stamp: header_in.file_name.time_stamp,
        author: header_in.file_name.author,
        organization: header_in.file_name.organization,
        preprocessor_version: header_in.file_name.preprocessor_version,
        originating_system: header_in.file_name.originating_system,
        authorization: header_in.file_name.authorization,
        file_schema: header_in.file_schema.schema.join("; "),
    };
}

//FIXME : this is utterly stupid
// we are bruteforcing the units from the exchange
// we should look into ruststep to see if there is a better way to do this
// or maybe use truck_stepio
// or ? anyway do better !
pub fn parse_units(exchange: &Exchange) -> Option<String> {
    trace_span!("parse_units");
    for section in &exchange.data {
        for entity in &section.entities {
            match entity {
                EntityInstance::Simple { record, .. } => {
                    if let Some(unit) = unit_from_record(record) {
                        return Some(unit);
                    }
                }
                EntityInstance::Complex { subsuper, .. } => {
                    for record in &subsuper.0 {
                        if let Some(unit) = unit_from_record(record) {
                            return Some(unit);
                        }
                    }
                }
            }
        }
    }
    None
}

pub fn compute_bounding_box(step_table: &truck_stepio::r#in::Table) -> Option<BoundingBox> {
    trace_span!("compute_bounding_box");
    let mut min = [f64::INFINITY; 3];
    let mut max = [f64::NEG_INFINITY; 3];

    let values = step_table.cartesian_point.values();
    for value in values {
        let coords = &value.coordinates;
        for i in 0..3 {
            min[i] = min[i].min(coords[i]);
            max[i] = max[i].max(coords[i]);
        }
    }
    if min[0].is_finite() {
        Some(BoundingBox { min, max })
    } else {
        None
    }
}

fn params_list(param: &Parameter) -> Option<&[Parameter]> {
    if let Parameter::List(list) = param {
        Some(list)
    } else {
        None
    }
}

fn param_to_enum<'a>(param: &'a Parameter) -> Option<&'a str> {
    if let Parameter::Enumeration(value) = param {
        Some(value.as_str())
    } else {
        None
    }
}

fn unit_from_record(record: &Record) -> Option<String> {
    if !record.name.eq_ignore_ascii_case("SI_UNIT") {
        return None;
    }

    let params = params_list(&record.parameter)?;
    let unit = params.get(1).and_then(param_to_enum)?;
    let prefix = params.get(0).and_then(param_to_enum);

    let unit = match unit {
        "METRE" => match prefix {
            Some("MILLI") => "mm".to_string(),
            Some("CENTI") => "cm".to_string(),
            Some("DECI") => "dm".to_string(),
            Some("KILO") => "km".to_string(),
            _ => "m".to_string(),
        },
        "INCH" => "in".to_string(),
        "FOOT" | "FEET" => "ft".to_string(),
        other => other.to_ascii_lowercase(),
    };

    Some(unit)
}
