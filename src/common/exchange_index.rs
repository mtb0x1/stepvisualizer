//! Single-pass index of a STEP AST exchange structure.
//!
//! [`ExchangeIndex::build`] iterates every entity **once**, simultaneously:
//! - Normalising `INTERSECTION_CURVE` / `BOUNDARY_CURVE` → `SURFACE_CURVE` (previously `normalize_exchange`).
//! - Collecting all raw data tables needed by [`crate::common::StepColorMap`].
//! - Collecting all raw data tables needed by [`crate::common::StepNameMap`].
//! - Resolving the file's [`crate::common::LengthUnit`] (previously `parse_units`).
//!
//! The index is cheap to drop: call [`std::mem::drop`] once color map, name map, and units are
//! extracted, so the intermediate [`HashMap`]s are freed before tessellation begins.

use std::collections::HashMap;

use crate::common::color::Color;
use crate::common::types::LengthUnit;
use crate::common::utils::{
    extract_entity_refs, param_as_enum, param_as_list, param_as_ref, param_as_str,
};
use crate::ruststep::ast::{EntityInstance, Exchange, Record};

// ---------------------------------------------------------------------------
// Public index type
// ---------------------------------------------------------------------------

/// Pre-built index of all entity data needed by the color, name, and unit
/// extraction passes, produced in a **single** traversal over the exchange.
///
/// Use [`ExchangeIndex::build`] to construct, then call [`StepColorMap::from_index`],
/// [`StepNameMap::from_index`], and [`ExchangeIndex::resolved_unit`].
/// Drop the index immediately afterwards to free intermediate maps before tessellation.
#[derive(Default)]
pub struct ExchangeIndex {
    // ---- color data (StepColorMap) -----------------------------------------
    /// Direct entity-id → Color for COLOUR_RGB / PRE_DEFINED_COLOUR entities.
    pub direct_colors: HashMap<u64, Color>,
    /// entity-id → list of child style entity ids (style graph edges).
    pub style_edges: HashMap<u64, Vec<u64>>,
    /// CLOSED/OPEN_SHELL id → list of face entity ids.
    pub shell_to_faces: HashMap<u64, Vec<u64>>,
    /// face entity id → CLOSED/OPEN_SHELL id.
    pub face_to_shell: HashMap<u64, u64>,
    /// (style_refs, target_id) pairs from STYLED_ITEM / OVER_RIDING_STYLED_ITEM.
    pub styled_items: Vec<(Vec<u64>, u64)>,

    // ---- shared data (StepColorMap, StepNameMap) ---------------------------
    /// MANIFOLD_SOLID_BREP / FACETED_BREP / BREP_WITH_VOIDS / SHELL_BASED_SURFACE_MODEL id → outer SHELL id.
    pub solid_to_shell: HashMap<u64, u64>,

    // ---- name data (StepNameMap) -------------------------------------------
    /// CLOSED/OPEN_SHELL id → cleaned direct name (from the shell record itself).
    pub shell_direct_names: HashMap<u64, String>,
    /// SHELL id → list of solid/surface-model entity ids that reference it.
    pub shell_to_solids: HashMap<u64, Vec<u64>>,
    /// solid entity id → cleaned name.
    pub solid_names: HashMap<u64, String>,
    /// SHAPE_REPRESENTATION-family id → list of item entity ids.
    pub rep_items: HashMap<u64, Vec<u64>>,
    /// item entity id (shell or solid) → list of SHAPE_REPRESENTATION entity ids containing it.
    pub item_to_reps: HashMap<u64, Vec<u64>>,
    /// SHAPE_REPRESENTATION-family id → cleaned name.
    pub rep_names: HashMap<u64, String>,
    /// (rep1_id, rep2_id) pairs from REPRESENTATION_RELATIONSHIP entities.
    pub rep_links: Vec<(u64, u64)>,
    /// SHAPE_REPRESENTATION id → PRODUCT_DEFINITION_SHAPE id (from SHAPE_DEFINITION_REPRESENTATION).
    pub shape_rep_to_pds: HashMap<u64, u64>,
    /// PRODUCT_DEFINITION_SHAPE id → cleaned name.
    pub pds_names: HashMap<u64, String>,
    /// PRODUCT_DEFINITION_SHAPE id → PRODUCT_DEFINITION id.
    pub pds_to_pd: HashMap<u64, u64>,
    /// PRODUCT_DEFINITION id → cleaned name.
    pub pd_names: HashMap<u64, String>,
    /// PRODUCT_DEFINITION id → PRODUCT_DEFINITION_FORMATION id.
    pub pd_to_pdf: HashMap<u64, u64>,
    /// PRODUCT_DEFINITION_FORMATION id → PRODUCT id.
    pub pdf_to_prod: HashMap<u64, u64>,
    /// PRODUCT id → cleaned name.
    pub prod_names: HashMap<u64, String>,
    /// PRODUCT_DEFINITION id → name from NEXT_ASSEMBLY_USAGE_OCCURRENCE.
    pub nauo_names: HashMap<u64, String>,

    // ---- unit data ---------------------------------------------------------
    /// Definitive length unit (from a Complex entity tagged LENGTH_UNIT). Preferred over fallback.
    pub length_unit: Option<LengthUnit>,
    /// Fallback length unit (first SI_UNIT or CONVERSION_BASED_UNIT seen, not necessarily length).
    pub unit_fallback: Option<LengthUnit>,
}

impl ExchangeIndex {
    /// Returns the best available [`LengthUnit`]: the definitive LENGTH_UNIT complex entity if
    /// found, otherwise the first fallback unit seen in the file.
    #[inline]
    pub fn resolved_unit(&self) -> Option<LengthUnit> {
        self.length_unit.or(self.unit_fallback)
    }

    /// Builds the index from the exchange structure in a **single pass**, simultaneously:
    /// - Normalising `INTERSECTION_CURVE` / `BOUNDARY_CURVE` → `SURFACE_CURVE` in-place.
    /// - Collecting color, name, and unit data.
    pub fn build(exchange: &mut Exchange) -> Self {
        let mut idx = ExchangeIndex::default();

        for section in &mut exchange.data {
            for entity in &mut section.entities {
                match entity {
                    EntityInstance::Simple { id, record } => {
                        let entity_id = *id;

                        // --- Normalisation (was normalize_exchange) ---
                        {
                            let name = record.name.as_str();
                            if (name.eq_ignore_ascii_case("INTERSECTION_CURVE")
                                || name.eq_ignore_ascii_case("BOUNDARY_CURVE"))
                                && name != "SURFACE_CURVE"
                            {
                                record.name.clear();
                                record.name.push_str("SURFACE_CURVE");
                            }
                        }

                        let name = record.name.as_str();

                        // -- Color entities --------------------------------------------------
                        if name.eq_ignore_ascii_case("COLOUR_RGB") {
                            if let Some(color) = Color::from_rgb_record(record) {
                                idx.direct_colors.insert(entity_id, color);
                            }
                        } else if name.eq_ignore_ascii_case("DRAUGHTING_PRE_DEFINED_COLOUR")
                            || name.eq_ignore_ascii_case("PRE_DEFINED_COLOUR")
                        {
                            if let Some(color) = Color::from_predefined_record(record) {
                                idx.direct_colors.insert(entity_id, color);
                            }
                        } else if is_color_style_entity(name) {
                            let refs = extract_entity_refs(&record.parameter);
                            if !refs.is_empty() {
                                idx.style_edges.insert(entity_id, refs);
                            }
                        } else if name.eq_ignore_ascii_case("STYLED_ITEM")
                            || name.eq_ignore_ascii_case("OVER_RIDING_STYLED_ITEM")
                        {
                            idx.collect_styled_item(record);

                        // -- Shell / solid: shared by color AND name --------------------------
                        } else if name.eq_ignore_ascii_case("CLOSED_SHELL")
                            || name.eq_ignore_ascii_case("OPEN_SHELL")
                        {
                            idx.collect_shell(entity_id, record);
                        } else if name.eq_ignore_ascii_case("MANIFOLD_SOLID_BREP")
                            || name.eq_ignore_ascii_case("BREP_WITH_VOIDS")
                            || name.eq_ignore_ascii_case("FACETED_BREP")
                        {
                            idx.collect_brep_solid(entity_id, record);
                        } else if name.eq_ignore_ascii_case("SHELL_BASED_SURFACE_MODEL") {
                            idx.collect_shell_based_surface_model(entity_id, record);

                        // -- Name-only entities -----------------------------------------------
                        } else if name.eq_ignore_ascii_case("ADVANCED_BREP_SHAPE_REPRESENTATION")
                            || name.eq_ignore_ascii_case("SHAPE_REPRESENTATION")
                            || name.eq_ignore_ascii_case("MANIFOLD_SURFACE_SHAPE_REPRESENTATION")
                            || name.eq_ignore_ascii_case(
                                "GEOMETRICALLY_BOUNDED_SURFACE_SHAPE_REPRESENTATION",
                            )
                            || name.eq_ignore_ascii_case("REPRESENTATION")
                        {
                            idx.collect_shape_representation(entity_id, record);
                        } else if name.eq_ignore_ascii_case("REPRESENTATION_RELATIONSHIP")
                            || name.eq_ignore_ascii_case("SHAPE_REPRESENTATION_RELATIONSHIP")
                        {
                            let refs = extract_entity_refs(&record.parameter);
                            if refs.len() >= 2 {
                                idx.rep_links.push((refs[0], refs[1]));
                            }
                        } else if name.eq_ignore_ascii_case("ID_ATTRIBUTE") {
                            idx.collect_id_attribute(record);
                        } else if name.eq_ignore_ascii_case("SHAPE_DEFINITION_REPRESENTATION") {
                            idx.collect_shape_def_rep(record);
                        } else if name.eq_ignore_ascii_case("PRODUCT_DEFINITION_SHAPE") {
                            idx.collect_product_definition_shape(entity_id, record);
                        } else if name.eq_ignore_ascii_case("PRODUCT_DEFINITION") {
                            idx.collect_product_definition(entity_id, record);
                        } else if name.starts_with("PRODUCT_DEFINITION_FORMATION") {
                            let refs = extract_entity_refs(&record.parameter);
                            if let Some(&prod_id) = refs.first() {
                                idx.pdf_to_prod.insert(entity_id, prod_id);
                            }
                        } else if name.eq_ignore_ascii_case("PRODUCT") {
                            idx.collect_product(entity_id, record);
                        } else if name.eq_ignore_ascii_case("NEXT_ASSEMBLY_USAGE_OCCURRENCE") {
                            idx.collect_nauo(record);

                        // -- Unit entities ----------------------------------------------------
                        } else if idx.length_unit.is_none()
                            && let Some(unit) = unit_from_record(record)
                            && idx.unit_fallback.is_none()
                        {
                            idx.unit_fallback = Some(unit);
                        }
                    }

                    EntityInstance::Complex { id: _, subsuper } => {
                        // Complex: REPRESENTATION_RELATIONSHIP / SHAPE_REPRESENTATION_RELATIONSHIP
                        let is_rep_rel = subsuper.0.iter().any(|r| {
                            r.name.eq_ignore_ascii_case("REPRESENTATION_RELATIONSHIP")
                                || r.name
                                    .eq_ignore_ascii_case("SHAPE_REPRESENTATION_RELATIONSHIP")
                        });
                        if is_rep_rel {
                            let mut all_refs = Vec::new();
                            for r in &subsuper.0 {
                                all_refs.extend(extract_entity_refs(&r.parameter));
                            }
                            if all_refs.len() >= 2 {
                                idx.rep_links.push((all_refs[0], all_refs[1]));
                            }
                        }

                        // Complex: LENGTH_UNIT (definitive) / fallback unit
                        if idx.length_unit.is_none() {
                            let is_length = subsuper
                                .0
                                .iter()
                                .any(|r| r.name.eq_ignore_ascii_case("LENGTH_UNIT"));
                            if is_length {
                                idx.length_unit = unit_from_subsuper(&subsuper.0);
                            } else if idx.unit_fallback.is_none() {
                                idx.unit_fallback = unit_from_subsuper(&subsuper.0);
                            }
                        }
                    }
                }
            }
        }

        idx
    }

    // -----------------------------------------------------------------------
    // Private helpers — color
    // -----------------------------------------------------------------------

    fn collect_styled_item(&mut self, record: &Record) {
        let Some(params) = param_as_list(&record.parameter) else {
            return;
        };
        let target = params.get(2).and_then(param_as_ref);
        if let (Some(styles_param), Some(target_id)) = (params.get(1), target) {
            let style_refs = extract_entity_refs(styles_param);
            self.styled_items.push((style_refs, target_id));
        }
    }

    // -----------------------------------------------------------------------
    // Private helpers — shared (color + name)
    // -----------------------------------------------------------------------

    fn collect_shell(&mut self, entity_id: u64, record: &Record) {
        let Some(params) = param_as_list(&record.parameter) else {
            return;
        };

        // Color: build face → shell and shell → faces maps
        if let Some(faces_param) = params.get(1) {
            let face_refs = extract_entity_refs(faces_param);
            for &face_id in &face_refs {
                self.face_to_shell.insert(face_id, entity_id);
            }
            self.shell_to_faces.insert(entity_id, face_refs);
        }

        // Name: shell direct name
        if let Some(raw_name) = params.first().and_then(param_as_str)
            && crate::common::step_names::is_valid_part_name(raw_name)
        {
            self.shell_direct_names.insert(
                entity_id,
                crate::common::step_names::clean_part_name(raw_name),
            );
        }
    }

    fn collect_brep_solid(&mut self, entity_id: u64, record: &Record) {
        let Some(params) = param_as_list(&record.parameter) else {
            return;
        };

        // Name: solid name (param 0)
        if let Some(raw_name) = params.first().and_then(param_as_str)
            && crate::common::step_names::is_valid_part_name(raw_name)
        {
            self.solid_names.insert(
                entity_id,
                crate::common::step_names::clean_part_name(raw_name),
            );
        }

        // Color + name: solid → shell link (param 1)
        if let Some(shell_id) = params.get(1).and_then(param_as_ref) {
            self.solid_to_shell.insert(entity_id, shell_id);
            self.shell_to_solids
                .entry(shell_id)
                .or_default()
                .push(entity_id);
        }
    }

    fn collect_shell_based_surface_model(&mut self, entity_id: u64, record: &Record) {
        let Some(params) = param_as_list(&record.parameter) else {
            return;
        };

        // Name: surface model name (param 0)
        if let Some(raw_name) = params.first().and_then(param_as_str)
            && crate::common::step_names::is_valid_part_name(raw_name)
        {
            self.solid_names.insert(
                entity_id,
                crate::common::step_names::clean_part_name(raw_name),
            );
        }

        // Name: model → shells (param 1, a list of refs)
        if let Some(shells_param) = params.get(1) {
            for shell_id in extract_entity_refs(shells_param) {
                self.solid_to_shell.insert(entity_id, shell_id);
                self.shell_to_solids
                    .entry(shell_id)
                    .or_default()
                    .push(entity_id);
            }
        }
    }

    // -----------------------------------------------------------------------
    // Private helpers — name only
    // -----------------------------------------------------------------------

    fn collect_shape_representation(&mut self, entity_id: u64, record: &Record) {
        let Some(params) = param_as_list(&record.parameter) else {
            return;
        };
        if let Some(raw_name) = params.first().and_then(param_as_str)
            && crate::common::step_names::is_valid_part_name(raw_name)
        {
            self.rep_names.insert(
                entity_id,
                crate::common::step_names::clean_part_name(raw_name),
            );
        }
        if let Some(items_param) = params.get(1) {
            let refs = extract_entity_refs(items_param);
            for &item_id in &refs {
                self.item_to_reps
                    .entry(item_id)
                    .or_default()
                    .push(entity_id);
            }
            self.rep_items.insert(entity_id, refs);
        }
    }

    fn collect_id_attribute(&mut self, record: &Record) {
        let Some(params) = param_as_list(&record.parameter) else {
            return;
        };
        let Some(raw_val) = params.first().and_then(param_as_str) else {
            return;
        };
        let Some(target_id) = params.get(1).and_then(param_as_ref) else {
            return;
        };
        if crate::common::step_names::is_valid_part_name(raw_val) {
            self.rep_names.insert(
                target_id,
                crate::common::step_names::clean_part_name(raw_val),
            );
        }
    }

    fn collect_shape_def_rep(&mut self, record: &Record) {
        let Some(params) = param_as_list(&record.parameter) else {
            return;
        };
        let Some(pds_id) = params.first().and_then(param_as_ref) else {
            return;
        };
        let Some(rep_id) = params.get(1).and_then(param_as_ref) else {
            return;
        };
        self.shape_rep_to_pds.insert(rep_id, pds_id);
    }

    fn collect_product_definition_shape(&mut self, entity_id: u64, record: &Record) {
        let Some(params) = param_as_list(&record.parameter) else {
            return;
        };
        let raw_name = params.first().and_then(param_as_str);
        let raw_desc = params.get(1).and_then(param_as_str);
        let chosen = raw_desc
            .filter(|s| crate::common::step_names::is_valid_part_name(s))
            .or_else(|| raw_name.filter(|s| crate::common::step_names::is_valid_part_name(s)));
        if let Some(val) = chosen {
            self.pds_names
                .insert(entity_id, crate::common::step_names::clean_part_name(val));
        }
        if let Some(pd_id) = params.get(2).and_then(param_as_ref) {
            self.pds_to_pd.insert(entity_id, pd_id);
        }
    }

    fn collect_product_definition(&mut self, entity_id: u64, record: &Record) {
        let Some(params) = param_as_list(&record.parameter) else {
            return;
        };
        let raw_id = params.first().and_then(param_as_str);
        let raw_desc = params.get(1).and_then(param_as_str);
        let chosen = raw_id
            .filter(|s| crate::common::step_names::is_valid_part_name(s))
            .or_else(|| raw_desc.filter(|s| crate::common::step_names::is_valid_part_name(s)));
        if let Some(val) = chosen {
            self.pd_names
                .insert(entity_id, crate::common::step_names::clean_part_name(val));
        }
        if let Some(pdf_id) = params.get(2).and_then(param_as_ref) {
            self.pd_to_pdf.insert(entity_id, pdf_id);
        }
    }

    fn collect_product(&mut self, entity_id: u64, record: &Record) {
        let Some(params) = param_as_list(&record.parameter) else {
            return;
        };
        let raw_id = params.first().and_then(param_as_str);
        let raw_name = params.get(1).and_then(param_as_str);
        let raw_desc = params.get(2).and_then(param_as_str);
        let chosen = raw_name
            .filter(|s| crate::common::step_names::is_valid_part_name(s))
            .or_else(|| raw_id.filter(|s| crate::common::step_names::is_valid_part_name(s)))
            .or_else(|| raw_desc.filter(|s| crate::common::step_names::is_valid_part_name(s)));
        if let Some(val) = chosen {
            self.prod_names
                .insert(entity_id, crate::common::step_names::clean_part_name(val));
        }
    }

    fn collect_nauo(&mut self, record: &Record) {
        let Some(params) = param_as_list(&record.parameter) else {
            return;
        };
        let raw_id = params.first().and_then(param_as_str);
        let raw_name = params.get(1).and_then(param_as_str);
        let raw_desc = params.get(2).and_then(param_as_str);
        let chosen = raw_desc
            .filter(|s| crate::common::step_names::is_valid_part_name(s))
            .or_else(|| raw_id.filter(|s| crate::common::step_names::is_valid_part_name(s)))
            .or_else(|| raw_name.filter(|s| crate::common::step_names::is_valid_part_name(s)));
        if let (Some(val), Some(related_pd)) = (chosen, params.get(4).and_then(param_as_ref)) {
            self.nauo_names
                .insert(related_pd, crate::common::step_names::clean_part_name(val));
        }
    }
}

// ---------------------------------------------------------------------------
// Unit helpers (were private to parser.rs)
// ---------------------------------------------------------------------------

fn unit_from_subsuper(records: &[Record]) -> Option<LengthUnit> {
    records.iter().find_map(unit_from_record)
}

fn unit_from_record(record: &Record) -> Option<LengthUnit> {
    if record.name.eq_ignore_ascii_case("SI_UNIT") {
        let params = param_as_list(&record.parameter)?;
        let unit = params.get(1).and_then(param_as_enum)?;
        let prefix = params.first().and_then(param_as_enum);
        return LengthUnit::from_si_spec(unit, prefix);
    }
    if record.name.eq_ignore_ascii_case("CONVERSION_BASED_UNIT") {
        let params = param_as_list(&record.parameter)?;
        let name = params.first().and_then(param_as_str)?;
        return LengthUnit::from_name(name);
    }
    None
}

// ---------------------------------------------------------------------------
// Color helpers
// ---------------------------------------------------------------------------

#[inline]
fn is_color_style_entity(name: &str) -> bool {
    name.eq_ignore_ascii_case("FILL_AREA_STYLE_COLOUR")
        || name.eq_ignore_ascii_case("FILL_AREA_STYLE")
        || name.eq_ignore_ascii_case("SURFACE_STYLE_FILL_AREA")
        || name.eq_ignore_ascii_case("SURFACE_SIDE_STYLE")
        || name.eq_ignore_ascii_case("SURFACE_STYLE_USAGE")
        || name.eq_ignore_ascii_case("PRESENTATION_STYLE_ASSIGNMENT")
        || name.eq_ignore_ascii_case("CURVE_STYLE")
        || name.eq_ignore_ascii_case("SYMBOL_STYLE")
        || name.eq_ignore_ascii_case("SYMBOL_COLOUR")
}
