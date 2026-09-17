//! Single-pass index of a STEP AST exchange structure.
//!
//! [`ExchangeIndex::build`] iterates every entity **once**, simultaneously:
//! - Normalising `INTERSECTION_CURVE` / `BOUNDARY_CURVE` → `SURFACE_CURVE` (previously `normalize_exchange`).
//! - Collecting all raw data tables needed by [`crate::common::StepColorMap`].
//! - Collecting all raw data tables needed by [`crate::common::StepNameMap`].
//! - Resolving the file's [`crate::common::LengthUnit`] (previously `parse_units`).
//!
//! The index is cheap to drop: call [`std::mem::drop`] once color map, name map, and units are
//! extracted, so the intermediate [`FastU64Map`]s are freed before tessellation begins.

use phf::phf_map;
use smallvec::SmallVec;
use smol_str::SmolStr;

use crate::common::ast_helpers::{
    ParameterExt, extract_entity_refs, extract_entity_refs_with_capacity, extract_smallvec_refs,
};
use crate::common::color::Color;
use crate::common::fast_hash::FastU64Map;
use crate::common::types::LengthUnit;
use crate::ruststep::ast::Parameter;
use crate::ruststep::ast::{EntityInstance, Exchange, Record};

// TODO : double check kinds against specs.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum StepEntityKind {
    ColourRgb,
    PreDefinedColour,
    ColorStyle,
    StyledItem,
    ClosedShell,
    OpenShell,
    ManifoldSolidBrep,
    BrepWithVoids,
    FacetedBrep,
    ShellBasedSurfaceModel,
    ShapeRepresentation,
    RepRelationship,
    IdAttribute,
    ShapeDefinitionRepresentation,
    ProductDefinitionShape,
    ProductDefinition,
    ProductDefinitionFormation,
    Product,
    NextAssemblyUsageOccurrence,
}

static STEP_ENTITY_KINDS: phf::Map<&'static str, StepEntityKind> = phf_map! {
    "COLOUR_RGB" => StepEntityKind::ColourRgb,
    "DRAUGHTING_PRE_DEFINED_COLOUR" => StepEntityKind::PreDefinedColour,
    "PRE_DEFINED_COLOUR" => StepEntityKind::PreDefinedColour,
    "FILL_AREA_STYLE_COLOUR" => StepEntityKind::ColorStyle,
    "FILL_AREA_STYLE" => StepEntityKind::ColorStyle,
    "SURFACE_STYLE_FILL_AREA" => StepEntityKind::ColorStyle,
    "SURFACE_SIDE_STYLE" => StepEntityKind::ColorStyle,
    "SURFACE_STYLE_USAGE" => StepEntityKind::ColorStyle,
    "PRESENTATION_STYLE_ASSIGNMENT" => StepEntityKind::ColorStyle,
    "CURVE_STYLE" => StepEntityKind::ColorStyle,
    "SYMBOL_STYLE" => StepEntityKind::ColorStyle,
    "SYMBOL_COLOUR" => StepEntityKind::ColorStyle,
    "STYLED_ITEM" => StepEntityKind::StyledItem,
    "OVER_RIDING_STYLED_ITEM" => StepEntityKind::StyledItem,
    "CLOSED_SHELL" => StepEntityKind::ClosedShell,
    "OPEN_SHELL" => StepEntityKind::OpenShell,
    "MANIFOLD_SOLID_BREP" => StepEntityKind::ManifoldSolidBrep,
    "BREP_WITH_VOIDS" => StepEntityKind::BrepWithVoids,
    "FACETED_BREP" => StepEntityKind::FacetedBrep,
    "SHELL_BASED_SURFACE_MODEL" => StepEntityKind::ShellBasedSurfaceModel,
    "ADVANCED_BREP_SHAPE_REPRESENTATION" => StepEntityKind::ShapeRepresentation,
    "SHAPE_REPRESENTATION" => StepEntityKind::ShapeRepresentation,
    "MANIFOLD_SURFACE_SHAPE_REPRESENTATION" => StepEntityKind::ShapeRepresentation,
    "GEOMETRICALLY_BOUNDED_SURFACE_SHAPE_REPRESENTATION" => StepEntityKind::ShapeRepresentation,
    "REPRESENTATION" => StepEntityKind::ShapeRepresentation,
    "REPRESENTATION_RELATIONSHIP" => StepEntityKind::RepRelationship,
    "SHAPE_REPRESENTATION_RELATIONSHIP" => StepEntityKind::RepRelationship,
    "ID_ATTRIBUTE" => StepEntityKind::IdAttribute,
    "SHAPE_DEFINITION_REPRESENTATION" => StepEntityKind::ShapeDefinitionRepresentation,
    "PRODUCT_DEFINITION_SHAPE" => StepEntityKind::ProductDefinitionShape,
    "PRODUCT_DEFINITION" => StepEntityKind::ProductDefinition,
    "PRODUCT_DEFINITION_FORMATION" => StepEntityKind::ProductDefinitionFormation,
    "PRODUCT_DEFINITION_FORMATION_WITH_SPECIFIED_SOURCE" => StepEntityKind::ProductDefinitionFormation,
    "PRODUCT" => StepEntityKind::Product,
    "NEXT_ASSEMBLY_USAGE_OCCURRENCE" => StepEntityKind::NextAssemblyUsageOccurrence,
};

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
    pub direct_colors: FastU64Map<Color>,
    /// entity-id → list of child style entity ids (style graph edges).
    pub style_edges: FastU64Map<SmallVec<[u64; 2]>>,
    /// CLOSED/OPEN_SHELL id → list of face entity ids.
    pub shell_to_faces: FastU64Map<Vec<u64>>,
    /// face entity id → CLOSED/OPEN_SHELL id.
    pub face_to_shell: FastU64Map<u64>,
    /// (style_refs, target_id) pairs from STYLED_ITEM / OVER_RIDING_STYLED_ITEM.
    pub styled_items: Vec<(SmallVec<[u64; 4]>, u64)>,

    // ---- shared data (StepColorMap, StepNameMap) ---------------------------
    /// MANIFOLD_SOLID_BREP / FACETED_BREP / BREP_WITH_VOIDS / SHELL_BASED_SURFACE_MODEL id → outer SHELL id.
    pub solid_to_shell: FastU64Map<u64>,

    // ---- name data (StepNameMap) -------------------------------------------
    /// CLOSED/OPEN_SHELL id → cleaned direct name (from the shell record itself).
    pub shell_direct_names: FastU64Map<SmolStr>,
    /// SHELL id → list of solid/surface-model entity ids that reference it.
    pub shell_to_solids: FastU64Map<SmallVec<[u64; 2]>>,
    /// solid entity id → cleaned name.
    pub solid_names: FastU64Map<SmolStr>,
    /// SHAPE_REPRESENTATION-family id → list of item entity ids.
    pub rep_items: FastU64Map<SmallVec<[u64; 5]>>,
    /// item entity id (shell or solid) → list of SHAPE_REPRESENTATION entity ids containing it.
    pub item_to_reps: FastU64Map<SmallVec<[u64; 4]>>,
    /// SHAPE_REPRESENTATION-family id → cleaned name.
    pub rep_names: FastU64Map<SmolStr>,
    /// (rep1_id, rep2_id) pairs from REPRESENTATION_RELATIONSHIP entities.
    pub rep_links: Vec<(u64, u64)>,
    /// SHAPE_REPRESENTATION id → PRODUCT_DEFINITION_SHAPE id (from SHAPE_DEFINITION_REPRESENTATION).
    pub shape_rep_to_pds: FastU64Map<u64>,
    /// PRODUCT_DEFINITION_SHAPE id → cleaned name.
    pub pds_names: FastU64Map<SmolStr>,
    /// PRODUCT_DEFINITION_SHAPE id → PRODUCT_DEFINITION id.
    pub pds_to_pd: FastU64Map<u64>,
    /// PRODUCT_DEFINITION id → cleaned name.
    pub pd_names: FastU64Map<SmolStr>,
    /// PRODUCT_DEFINITION id → PRODUCT_DEFINITION_FORMATION id.
    pub pd_to_pdf: FastU64Map<u64>,
    /// PRODUCT_DEFINITION_FORMATION id → PRODUCT id.
    pub pdf_to_prod: FastU64Map<u64>,
    /// PRODUCT id → cleaned name.
    pub prod_names: FastU64Map<SmolStr>,
    /// PRODUCT_DEFINITION id → name from NEXT_ASSEMBLY_USAGE_OCCURRENCE.
    pub nauo_names: FastU64Map<SmolStr>,

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

                        // O(1) minimal perfect hash lookup (fallback to uppercase if non-conformant case)
                        let kind = STEP_ENTITY_KINDS.get(name).copied().or_else(|| {
                            if name.bytes().any(|b| b.is_ascii_lowercase()) {
                                let upper = name.to_ascii_uppercase();
                                STEP_ENTITY_KINDS.get(upper.as_str()).copied()
                            } else {
                                None
                            }
                        });

                        match kind {
                            // ==============================================================================
                            // Color Extraction
                            // ==============================================================================
                            // We extract colors defined directly on RGB or predefined entities, and link
                            // presentation styles to their target geometry.
                            Some(StepEntityKind::ColourRgb) => {
                                if let Some(color) = Color::from_rgb_record(record) {
                                    idx.direct_colors.insert(entity_id, color);
                                }
                            }
                            Some(StepEntityKind::PreDefinedColour) => {
                                if let Some(color) = Color::from_predefined_record(record) {
                                    idx.direct_colors.insert(entity_id, color);
                                }
                            }
                            Some(StepEntityKind::ColorStyle) => {
                                let refs = extract_smallvec_refs(&record.parameter);
                                if !refs.is_empty() {
                                    idx.style_edges.insert(entity_id, refs);
                                }
                            }
                            Some(StepEntityKind::StyledItem) => {
                                if let Some(params) = record.parameter.try_extract::<&[Parameter]>()
                                    && let (Some(styles_param), Some(target_id)) = (
                                        params.get(1),
                                        params.get(2).and_then(|p| p.try_extract::<u64>()),
                                    )
                                {
                                    let style_refs = extract_smallvec_refs(styles_param);
                                    idx.styled_items.push((style_refs, target_id));
                                }
                            }
                            Some(StepEntityKind::ClosedShell | StepEntityKind::OpenShell) => {
                                if let Some(params) = record.parameter.try_extract::<&[Parameter]>()
                                {
                                    // Color: build face → shell and shell → faces maps
                                    if let Some(faces_param) = params.get(1) {
                                        let face_refs =
                                            extract_entity_refs_with_capacity(faces_param, 5000);
                                        for &face_id in &face_refs {
                                            idx.face_to_shell.insert(face_id, entity_id);
                                        }
                                        idx.shell_to_faces.insert(entity_id, face_refs);
                                    }

                                    // Name: shell direct name
                                    if let Some(raw_name) =
                                        params.first().and_then(|p| p.try_extract::<&str>())
                                        && crate::common::step_names::is_valid_part_name(raw_name)
                                    {
                                        idx.shell_direct_names.insert(
                                            entity_id,
                                            crate::common::step_names::clean_part_name(raw_name),
                                        );
                                    }
                                }
                            }
                            Some(
                                StepEntityKind::ManifoldSolidBrep
                                | StepEntityKind::BrepWithVoids
                                | StepEntityKind::FacetedBrep,
                            ) => {
                                if let Some(params) = record.parameter.try_extract::<&[Parameter]>()
                                {
                                    // Name: solid name (param 0)
                                    if let Some(raw_name) =
                                        params.first().and_then(|p| p.try_extract::<&str>())
                                        && crate::common::step_names::is_valid_part_name(raw_name)
                                    {
                                        idx.solid_names.insert(
                                            entity_id,
                                            crate::common::step_names::clean_part_name(raw_name),
                                        );
                                    }

                                    // Color + name: solid → shell link (param 1)
                                    if let Some(shell_id) =
                                        params.get(1).and_then(|p| p.try_extract::<u64>())
                                    {
                                        idx.solid_to_shell.insert(entity_id, shell_id);
                                        idx.shell_to_solids
                                            .entry(shell_id)
                                            .or_default()
                                            .push(entity_id);
                                    }
                                }
                            }
                            Some(StepEntityKind::ShellBasedSurfaceModel) => {
                                if let Some(params) = record.parameter.try_extract::<&[Parameter]>()
                                {
                                    // Name: surface model name (param 0)
                                    if let Some(raw_name) =
                                        params.first().and_then(|p| p.try_extract::<&str>())
                                        && crate::common::step_names::is_valid_part_name(raw_name)
                                    {
                                        idx.solid_names.insert(
                                            entity_id,
                                            crate::common::step_names::clean_part_name(raw_name),
                                        );
                                    }

                                    // Name: model → shells (param 1, a list of refs)
                                    if let Some(shells_param) = params.get(1) {
                                        for shell_id in extract_entity_refs(shells_param) {
                                            idx.solid_to_shell.insert(entity_id, shell_id);
                                            idx.shell_to_solids
                                                .entry(shell_id)
                                                .or_default()
                                                .push(entity_id);
                                        }
                                    }
                                }
                            }

                            // ==============================================================================
                            // Name Resolution
                            // ==============================================================================
                            // We traverse the assembly tree (Shape Representation -> Product Definition ->
                            // Product) to find and link the best human-readable part names.
                            Some(StepEntityKind::ShapeRepresentation) => {
                                if let Some(params) = record.parameter.try_extract::<&[Parameter]>()
                                {
                                    if let Some(raw_name) =
                                        params.first().and_then(|p| p.try_extract::<&str>())
                                        && crate::common::step_names::is_valid_part_name(raw_name)
                                    {
                                        idx.rep_names.insert(
                                            entity_id,
                                            crate::common::step_names::clean_part_name(raw_name),
                                        );
                                    }
                                    if let Some(items_param) = params.get(1) {
                                        let refs = extract_smallvec_refs(items_param);
                                        for &item_id in &refs {
                                            idx.item_to_reps
                                                .entry(item_id)
                                                .or_default()
                                                .push(entity_id);
                                        }
                                        idx.rep_items.insert(entity_id, refs);
                                    }
                                }
                            }
                            Some(StepEntityKind::RepRelationship) => {
                                let refs = extract_entity_refs(&record.parameter);
                                if refs.len() >= 2 {
                                    idx.rep_links.push((refs[0], refs[1]));
                                }
                            }
                            Some(StepEntityKind::IdAttribute) => {
                                if let Some(params) = record.parameter.try_extract::<&[Parameter]>()
                                    && let (Some(raw_val), Some(target_id)) = (
                                        params.first().and_then(|p| p.try_extract::<&str>()),
                                        params.get(1).and_then(|p| p.try_extract::<u64>()),
                                    )
                                    && crate::common::step_names::is_valid_part_name(raw_val)
                                {
                                    idx.rep_names.insert(
                                        target_id,
                                        crate::common::step_names::clean_part_name(raw_val),
                                    );
                                }
                            }
                            Some(StepEntityKind::ShapeDefinitionRepresentation) => {
                                if let Some(params) = record.parameter.try_extract::<&[Parameter]>()
                                    && let (Some(pds_id), Some(rep_id)) = (
                                        params.first().and_then(|p| p.try_extract::<u64>()),
                                        params.get(1).and_then(|p| p.try_extract::<u64>()),
                                    )
                                {
                                    idx.shape_rep_to_pds.insert(rep_id, pds_id);
                                }
                            }
                            Some(StepEntityKind::ProductDefinitionShape) => {
                                if let Some(params) = record.parameter.try_extract::<&[Parameter]>()
                                {
                                    let raw_name =
                                        params.first().and_then(|p| p.try_extract::<&str>());
                                    let raw_desc =
                                        params.get(1).and_then(|p| p.try_extract::<&str>());
                                    let chosen = raw_desc
                                        .filter(|s| {
                                            crate::common::step_names::is_valid_part_name(s)
                                        })
                                        .or_else(|| {
                                            raw_name.filter(|s| {
                                                crate::common::step_names::is_valid_part_name(s)
                                            })
                                        });
                                    if let Some(val) = chosen {
                                        idx.pds_names.insert(
                                            entity_id,
                                            crate::common::step_names::clean_part_name(val),
                                        );
                                    }
                                    if let Some(pd_id) =
                                        params.get(2).and_then(|p| p.try_extract::<u64>())
                                    {
                                        idx.pds_to_pd.insert(entity_id, pd_id);
                                    }
                                }
                            }
                            Some(StepEntityKind::ProductDefinition) => {
                                if let Some(params) = record.parameter.try_extract::<&[Parameter]>()
                                {
                                    let raw_id =
                                        params.first().and_then(|p| p.try_extract::<&str>());
                                    let raw_desc =
                                        params.get(1).and_then(|p| p.try_extract::<&str>());
                                    let chosen = raw_id
                                        .filter(|s| {
                                            crate::common::step_names::is_valid_part_name(s)
                                        })
                                        .or_else(|| {
                                            raw_desc.filter(|s| {
                                                crate::common::step_names::is_valid_part_name(s)
                                            })
                                        });
                                    if let Some(val) = chosen {
                                        idx.pd_names.insert(
                                            entity_id,
                                            crate::common::step_names::clean_part_name(val),
                                        );
                                    }
                                    if let Some(pdf_id) =
                                        params.get(2).and_then(|p| p.try_extract::<u64>())
                                    {
                                        idx.pd_to_pdf.insert(entity_id, pdf_id);
                                    }
                                }
                            }
                            Some(StepEntityKind::ProductDefinitionFormation) => {
                                let refs = extract_entity_refs(&record.parameter);
                                if let Some(&prod_id) = refs.first() {
                                    idx.pdf_to_prod.insert(entity_id, prod_id);
                                }
                            }
                            Some(StepEntityKind::Product) => {
                                if let Some(params) = record.parameter.try_extract::<&[Parameter]>()
                                {
                                    let raw_id =
                                        params.first().and_then(|p| p.try_extract::<&str>());
                                    let raw_name =
                                        params.get(1).and_then(|p| p.try_extract::<&str>());
                                    let raw_desc =
                                        params.get(2).and_then(|p| p.try_extract::<&str>());
                                    let chosen = raw_name
                                        .filter(|s| {
                                            crate::common::step_names::is_valid_part_name(s)
                                        })
                                        .or_else(|| {
                                            raw_id.filter(|s| {
                                                crate::common::step_names::is_valid_part_name(s)
                                            })
                                        })
                                        .or_else(|| {
                                            raw_desc.filter(|s| {
                                                crate::common::step_names::is_valid_part_name(s)
                                            })
                                        });
                                    if let Some(val) = chosen {
                                        idx.prod_names.insert(
                                            entity_id,
                                            crate::common::step_names::clean_part_name(val),
                                        );
                                    }
                                }
                            }
                            Some(StepEntityKind::NextAssemblyUsageOccurrence) => {
                                if let Some(params) = record.parameter.try_extract::<&[Parameter]>()
                                {
                                    let raw_id =
                                        params.first().and_then(|p| p.try_extract::<&str>());
                                    let raw_name =
                                        params.get(1).and_then(|p| p.try_extract::<&str>());
                                    let raw_desc =
                                        params.get(2).and_then(|p| p.try_extract::<&str>());
                                    let chosen = raw_desc
                                        .filter(|s| {
                                            crate::common::step_names::is_valid_part_name(s)
                                        })
                                        .or_else(|| {
                                            raw_id.filter(|s| {
                                                crate::common::step_names::is_valid_part_name(s)
                                            })
                                        })
                                        .or_else(|| {
                                            raw_name.filter(|s| {
                                                crate::common::step_names::is_valid_part_name(s)
                                            })
                                        });
                                    if let (Some(val), Some(related_pd)) =
                                        (chosen, params.get(4).and_then(|p| p.try_extract::<u64>()))
                                    {
                                        idx.nauo_names.insert(
                                            related_pd,
                                            crate::common::step_names::clean_part_name(val),
                                        );
                                    }
                                }
                            }

                            // ==============================================================================
                            // Unit Identification & Fallbacks
                            // ==============================================================================
                            // If we don't recognize the entity directly, we still extract PDF -> Product
                            // links and try to fallback to any SI_UNIT if no definitive unit is found.
                            None => {
                                if name.starts_with("PRODUCT_DEFINITION_FORMATION") {
                                    let refs = extract_entity_refs(&record.parameter);
                                    if let Some(&prod_id) = refs.first() {
                                        idx.pdf_to_prod.insert(entity_id, prod_id);
                                    }
                                } else if idx.length_unit.is_none()
                                    && let Some(unit) = unit_from_record(record)
                                    && idx.unit_fallback.is_none()
                                {
                                    idx.unit_fallback = Some(unit);
                                }
                            }
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
}

// ---------------------------------------------------------------------------
// Unit helpers (were private to parser.rs)
// ---------------------------------------------------------------------------

fn unit_from_subsuper(records: &[Record]) -> Option<LengthUnit> {
    records.iter().find_map(unit_from_record)
}

fn unit_from_record(record: &Record) -> Option<LengthUnit> {
    if record.name.eq_ignore_ascii_case("SI_UNIT") {
        let params = record.parameter.try_extract::<&[Parameter]>()?;
        let unit = params.get(1).and_then(|p| p.try_extract::<&str>())?;
        let prefix = params.first().and_then(|p| p.try_extract::<&str>());
        return LengthUnit::from_si_spec(unit, prefix);
    }
    if record.name.eq_ignore_ascii_case("CONVERSION_BASED_UNIT") {
        let params = record.parameter.try_extract::<&[Parameter]>()?;
        let name = params.first().and_then(|p| p.try_extract::<&str>())?;
        return LengthUnit::from_name(name);
    }
    None
}
