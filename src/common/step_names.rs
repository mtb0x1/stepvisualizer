//! STEP ISO 10303 part and mesh name extraction across solids, products,
//! representations, and assembly occurrences.

use std::collections::HashMap;

use crate::common::utils::{extract_entity_refs, param_as_list, param_as_ref, param_as_str};
use crate::ruststep::ast::{EntityInstance, Exchange};

/// Extracted mapping of STEP shell entity IDs to resolved, human-readable part names.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct StepNameMap {
    /// Maps STEP entity ID (typically `CLOSED_SHELL` or `OPEN_SHELL`) to a cleaned part name.
    pub shell_names: HashMap<u64, String>,
}

impl StepNameMap {
    /// Returns the resolved name for shell with STEP entity ID `shell_id`.
    #[inline]
    pub fn get(&self, shell_id: u64) -> Option<&str> {
        self.shell_names.get(&shell_id).map(|s| s.as_str())
    }

    /// Whether any part names were extracted from the STEP file.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.shell_names.is_empty()
    }

    /// Total number of named shells identified.
    #[inline]
    pub fn len(&self) -> usize {
        self.shell_names.len()
    }

    /// Extracts part names and associates them with shells from a parsed STEP AST.
    pub fn from_exchange(exchange: &Exchange) -> Self {
        let mut shell_direct_names: HashMap<u64, String> = HashMap::new();
        let mut solid_to_shell: HashMap<u64, u64> = HashMap::new();
        let mut shell_to_solids: HashMap<u64, Vec<u64>> = HashMap::new();
        let mut solid_names: HashMap<u64, String> = HashMap::new();
        let mut rep_items: HashMap<u64, Vec<u64>> = HashMap::new();
        let mut rep_names: HashMap<u64, String> = HashMap::new();
        let mut rep_links: Vec<(u64, u64)> = Vec::new();
        let mut shape_rep_to_pds: HashMap<u64, u64> = HashMap::new();
        let mut pds_names: HashMap<u64, String> = HashMap::new();
        let mut pds_to_pd: HashMap<u64, u64> = HashMap::new();
        let mut pd_names: HashMap<u64, String> = HashMap::new();
        let mut pd_to_pdf: HashMap<u64, u64> = HashMap::new();
        let mut pdf_to_prod: HashMap<u64, u64> = HashMap::new();
        let mut prod_names: HashMap<u64, String> = HashMap::new();
        let mut nauo_names: HashMap<u64, String> = HashMap::new();

        for section in &exchange.data {
            for entity in &section.entities {
                match entity {
                    EntityInstance::Simple { id, record } => {
                        let entity_id = *id;
                        let name = record.name.as_str();

                        if name.eq_ignore_ascii_case("CLOSED_SHELL")
                            || name.eq_ignore_ascii_case("OPEN_SHELL")
                        {
                            if let Some(params) = param_as_list(&record.parameter)
                                && let Some(raw_name) = params.first().and_then(param_as_str)
                                && is_valid_part_name(raw_name)
                            {
                                shell_direct_names.insert(entity_id, clean_part_name(raw_name));
                            }
                        } else if name.eq_ignore_ascii_case("MANIFOLD_SOLID_BREP")
                            || name.eq_ignore_ascii_case("BREP_WITH_VOIDS")
                            || name.eq_ignore_ascii_case("FACETED_BREP")
                        {
                            if let Some(params) = param_as_list(&record.parameter) {
                                if let Some(raw_name) = params.first().and_then(param_as_str)
                                    && is_valid_part_name(raw_name)
                                {
                                    solid_names.insert(entity_id, clean_part_name(raw_name));
                                }
                                if let Some(shell_id) = params.get(1).and_then(param_as_ref) {
                                    solid_to_shell.insert(entity_id, shell_id);
                                    shell_to_solids.entry(shell_id).or_default().push(entity_id);
                                }
                            }
                        } else if name.eq_ignore_ascii_case("SHELL_BASED_SURFACE_MODEL") {
                            if let Some(params) = param_as_list(&record.parameter) {
                                if let Some(raw_name) = params.first().and_then(param_as_str)
                                    && is_valid_part_name(raw_name)
                                {
                                    solid_names.insert(entity_id, clean_part_name(raw_name));
                                }
                                if let Some(shells_param) = params.get(1) {
                                    for shell_id in extract_entity_refs(shells_param) {
                                        solid_to_shell.insert(entity_id, shell_id);
                                        shell_to_solids
                                            .entry(shell_id)
                                            .or_default()
                                            .push(entity_id);
                                    }
                                }
                            }
                        } else if name.eq_ignore_ascii_case("ADVANCED_BREP_SHAPE_REPRESENTATION")
                            || name.eq_ignore_ascii_case("SHAPE_REPRESENTATION")
                            || name.eq_ignore_ascii_case("MANIFOLD_SURFACE_SHAPE_REPRESENTATION")
                            || name.eq_ignore_ascii_case(
                                "GEOMETRICALLY_BOUNDED_SURFACE_SHAPE_REPRESENTATION",
                            )
                            || name.eq_ignore_ascii_case("REPRESENTATION")
                        {
                            if let Some(params) = param_as_list(&record.parameter) {
                                if let Some(raw_name) = params.first().and_then(param_as_str)
                                    && is_valid_part_name(raw_name)
                                {
                                    rep_names.insert(entity_id, clean_part_name(raw_name));
                                }
                                if let Some(items_param) = params.get(1) {
                                    let items = extract_entity_refs(items_param);
                                    rep_items.insert(entity_id, items);
                                }
                            }
                        } else if name.eq_ignore_ascii_case("REPRESENTATION_RELATIONSHIP")
                            || name.eq_ignore_ascii_case("SHAPE_REPRESENTATION_RELATIONSHIP")
                        {
                            let refs = extract_entity_refs(&record.parameter);
                            if refs.len() >= 2 {
                                rep_links.push((refs[0], refs[1]));
                            }
                        } else if name.eq_ignore_ascii_case("ID_ATTRIBUTE") {
                            if let Some(params) = param_as_list(&record.parameter)
                                && let (Some(raw_val), Some(target_id)) = (
                                    params.first().and_then(param_as_str),
                                    params.get(1).and_then(param_as_ref),
                                )
                                && is_valid_part_name(raw_val)
                            {
                                rep_names.insert(target_id, clean_part_name(raw_val));
                            }
                        } else if name.eq_ignore_ascii_case("SHAPE_DEFINITION_REPRESENTATION") {
                            if let Some(params) = param_as_list(&record.parameter)
                                && let (Some(pds_id), Some(rep_id)) = (
                                    params.first().and_then(param_as_ref),
                                    params.get(1).and_then(param_as_ref),
                                )
                            {
                                shape_rep_to_pds.insert(rep_id, pds_id);
                            }
                        } else if name.eq_ignore_ascii_case("PRODUCT_DEFINITION_SHAPE") {
                            if let Some(params) = param_as_list(&record.parameter) {
                                let raw_name = params.first().and_then(param_as_str);
                                let raw_desc = params.get(1).and_then(param_as_str);
                                let chosen = raw_desc
                                    .filter(|s| is_valid_part_name(s))
                                    .or_else(|| raw_name.filter(|s| is_valid_part_name(s)));
                                if let Some(val) = chosen {
                                    pds_names.insert(entity_id, clean_part_name(val));
                                }
                                if let Some(pd_id) = params.get(2).and_then(param_as_ref) {
                                    pds_to_pd.insert(entity_id, pd_id);
                                }
                            }
                        } else if name.eq_ignore_ascii_case("PRODUCT_DEFINITION") {
                            if let Some(params) = param_as_list(&record.parameter) {
                                let raw_id = params.first().and_then(param_as_str);
                                let raw_desc = params.get(1).and_then(param_as_str);
                                let chosen = raw_id
                                    .filter(|s| is_valid_part_name(s))
                                    .or_else(|| raw_desc.filter(|s| is_valid_part_name(s)));
                                if let Some(val) = chosen {
                                    pd_names.insert(entity_id, clean_part_name(val));
                                }
                                if let Some(pdf_id) = params.get(2).and_then(param_as_ref) {
                                    pd_to_pdf.insert(entity_id, pdf_id);
                                }
                            }
                        } else if name.starts_with("PRODUCT_DEFINITION_FORMATION") {
                            let refs = extract_entity_refs(&record.parameter);
                            if let Some(&prod_id) = refs.first() {
                                pdf_to_prod.insert(entity_id, prod_id);
                            }
                        } else if name.eq_ignore_ascii_case("PRODUCT") {
                            if let Some(params) = param_as_list(&record.parameter) {
                                let raw_id = params.first().and_then(param_as_str);
                                let raw_name = params.get(1).and_then(param_as_str);
                                let raw_desc = params.get(2).and_then(param_as_str);
                                let chosen = raw_name
                                    .filter(|s| is_valid_part_name(s))
                                    .or_else(|| raw_id.filter(|s| is_valid_part_name(s)))
                                    .or_else(|| raw_desc.filter(|s| is_valid_part_name(s)));
                                if let Some(val) = chosen {
                                    prod_names.insert(entity_id, clean_part_name(val));
                                }
                            }
                        } else if name.eq_ignore_ascii_case("NEXT_ASSEMBLY_USAGE_OCCURRENCE")
                            && let Some(params) = param_as_list(&record.parameter)
                        {
                            let raw_id = params.first().and_then(param_as_str);
                            let raw_name = params.get(1).and_then(param_as_str);
                            let raw_desc = params.get(2).and_then(param_as_str);
                            let chosen = raw_desc
                                .filter(|s| is_valid_part_name(s))
                                .or_else(|| raw_id.filter(|s| is_valid_part_name(s)))
                                .or_else(|| raw_name.filter(|s| is_valid_part_name(s)));
                            if let (Some(val), Some(related_pd)) =
                                (chosen, params.get(4).and_then(param_as_ref))
                            {
                                nauo_names.insert(related_pd, clean_part_name(val));
                            }
                        }
                    }
                    EntityInstance::Complex { subsuper, .. } => {
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
                                rep_links.push((all_refs[0], all_refs[1]));
                            }
                        }
                    }
                }
            }
        }

        // Gather all shell IDs found in the file
        let mut all_shells = std::collections::HashSet::new();
        all_shells.extend(shell_direct_names.keys().copied());
        all_shells.extend(solid_to_shell.values().copied());
        all_shells.extend(shell_to_solids.keys().copied());

        let mut shell_names = HashMap::new();

        for shell_id in all_shells {
            let solids = shell_to_solids.get(&shell_id);

            // 1. Check solid name
            let solid_candidate = solids.and_then(|sol_list| {
                sol_list
                    .iter()
                    .find_map(|s| solid_names.get(s))
                    .map(|s| s.as_str())
            });

            // Find all representations that DIRECTLY contain any of this shell's solids or the shell itself
            let matching_reps: Vec<u64> = rep_items
                .iter()
                .filter(|(_, items)| {
                    items.contains(&shell_id)
                        || solids.is_some_and(|s_list| s_list.iter().any(|s| items.contains(s)))
                })
                .map(|(rep_id, _)| *rep_id)
                .collect();

            // 2. Check product name
            let prod_candidate = matching_reps.iter().find_map(|r| {
                let pds = resolve_pds_for_rep(*r, &shape_rep_to_pds, &rep_links)?;
                let pd = pds_to_pd.get(&pds)?;
                let pdf = pd_to_pdf.get(pd)?;
                let prod = pdf_to_prod.get(pdf)?;
                prod_names.get(prod).map(|s| s.as_str())
            });

            // 3. Check assembly instance occurrence name (NAUO)
            let nauo_candidate = matching_reps.iter().find_map(|r| {
                let pds = resolve_pds_for_rep(*r, &shape_rep_to_pds, &rep_links)?;
                let pd = pds_to_pd.get(&pds)?;
                nauo_names.get(pd).map(|s| s.as_str())
            });

            // 4. Check product definition name
            let pd_candidate = matching_reps.iter().find_map(|r| {
                let pds = resolve_pds_for_rep(*r, &shape_rep_to_pds, &rep_links)?;
                let pd = pds_to_pd.get(&pds)?;
                pd_names.get(pd).map(|s| s.as_str())
            });

            // 5. Check product definition shape name/desc
            let pds_candidate = matching_reps.iter().find_map(|r| {
                let pds = resolve_pds_for_rep(*r, &shape_rep_to_pds, &rep_links)?;
                pds_names.get(&pds).map(|s| s.as_str())
            });

            // 6. Check representation name
            let rep_candidate = matching_reps
                .iter()
                .find_map(|r| rep_names.get(r).map(|s| s.as_str()));

            // 7. Check shell direct name
            let shell_candidate = shell_direct_names.get(&shell_id).map(|s| s.as_str());

            // Determine best name candidate
            let chosen = select_best_name(
                solid_candidate,
                prod_candidate,
                nauo_candidate,
                pd_candidate,
                pds_candidate,
                rep_candidate,
                shell_candidate,
            );

            if let Some(name) = chosen {
                shell_names.insert(shell_id, name.to_string());
            }
        }

        Self { shell_names }
    }
}

/// Resolves the associated `PRODUCT_DEFINITION_SHAPE` for a representation,
/// checking both direct mapping and representation relationships.
fn resolve_pds_for_rep(
    rep_id: u64,
    shape_rep_to_pds: &HashMap<u64, u64>,
    rep_links: &[(u64, u64)],
) -> Option<u64> {
    if let Some(&pds) = shape_rep_to_pds.get(&rep_id) {
        return Some(pds);
    }
    for &(r1, r2) in rep_links {
        if r1 == rep_id {
            if let Some(&pds) = shape_rep_to_pds.get(&r2) {
                return Some(pds);
            }
        } else if r2 == rep_id
            && let Some(&pds) = shape_rep_to_pds.get(&r1)
        {
            return Some(pds);
        }
    }
    None
}

/// Chooses the highest quality name candidate using priority rules:
/// - A descriptive solid name (e.g. "Housing", "Pin 1", "PartBody") takes precedence.
/// - If the solid name is purely numeric and a non-numeric product/assembly name is available, prefer the latter.
/// - Otherwise fall back through Product -> NAUO -> Product Definition -> Shape -> Representation -> Shell.
fn select_best_name<'a>(
    solid: Option<&'a str>,
    prod: Option<&'a str>,
    nauo: Option<&'a str>,
    pd: Option<&'a str>,
    pds: Option<&'a str>,
    rep: Option<&'a str>,
    shell: Option<&'a str>,
) -> Option<&'a str> {
    if let Some(s) = solid {
        let is_pure_digit = s.chars().all(|c| c.is_ascii_digit());
        if !is_pure_digit {
            return Some(s);
        }
        // If solid is pure digits like "1", but we have a descriptive product/assembly name, prefer that
        if let Some(p) = prod.filter(|val| val.chars().any(|c| c.is_alphabetic())) {
            return Some(p);
        }
        if let Some(n) = nauo.filter(|val| val.chars().any(|c| c.is_alphabetic())) {
            return Some(n);
        }
        return Some(s);
    }

    if let Some(p) = prod {
        return Some(p);
    }
    if let Some(n) = nauo {
        return Some(n);
    }
    if let Some(d) = pd {
        return Some(d);
    }
    if let Some(s) = pds {
        return Some(s);
    }
    if let Some(r) = rep {
        return Some(r);
    }
    shell
}

/// Validates whether a raw name string is meaningful and suitable for display.
///
/// Filters out:
/// - Empty strings or strings with only whitespace/quotes
/// - CAD null/unspecified placeholders: "NONE", "None", "unspecified", "not specified", "null", "no_name", "default", "undefined"
/// - Entity references such as "#602" or lone "#", "$", "*"
pub fn is_valid_part_name(s: &str) -> bool {
    let clean = clean_part_name(s);
    if clean.is_empty() {
        return false;
    }
    // Filter out STEP entity pointer labels like "#602", "#123"
    if clean.starts_with('#') && clean[1..].chars().all(|c| c.is_ascii_digit()) {
        return false;
    }
    if clean == "#" || clean == "$" || clean == "*" {
        return false;
    }
    let lower = clean.to_ascii_lowercase();
    !(lower == "none"
        || lower == "null"
        || lower == "na"
        || lower == "n/a"
        || lower == "unspecified"
        || lower == "not specified"
        || lower == "no_name"
        || lower == "no name"
        || lower == "default"
        || lower == "none/default"
        || lower == "undefined"
        || lower == "solid"
        || lower == "part")
}

/// Cleans a part name by trimming quotes, whitespace, and descriptive CAD prefixes like "SHAPE FOR ".
pub fn clean_part_name(s: &str) -> String {
    let trimmed = s.trim().trim_matches('\'').trim_matches('"').trim();
    if let Some(stripped) = trimmed.strip_prefix("SHAPE FOR ") {
        stripped.trim().trim_end_matches('.').trim().to_string()
    } else if let Some(stripped) = trimmed.strip_prefix("shape for ") {
        stripped.trim().trim_end_matches('.').trim().to_string()
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wasm_bindgen_test::*;

    wasm_bindgen_test_configure!(run_in_browser);

    #[wasm_bindgen_test]
    fn test_is_valid_part_name() {
        assert!(is_valid_part_name("Housing"));
        assert!(is_valid_part_name("'Pin 1'"));
        assert!(is_valid_part_name("\"PartBody\""));
        assert!(is_valid_part_name("l-bracket_1"));
        assert!(is_valid_part_name("1"));

        assert!(!is_valid_part_name(""));
        assert!(!is_valid_part_name("   "));
        assert!(!is_valid_part_name("''"));
        assert!(!is_valid_part_name("NONE"));
        assert!(!is_valid_part_name("'None'"));
        assert!(!is_valid_part_name("unspecified"));
        assert!(!is_valid_part_name("NOT SPECIFIED"));
        assert!(!is_valid_part_name("no_name"));
        assert!(!is_valid_part_name("#602"));
        assert!(!is_valid_part_name("#1234"));
        assert!(!is_valid_part_name("#"));
        assert!(!is_valid_part_name("$"));
        assert!(!is_valid_part_name("*"));
    }

    #[wasm_bindgen_test]
    fn test_clean_part_name() {
        assert_eq!(clean_part_name("'Housing'"), "Housing");
        assert_eq!(clean_part_name("  \"Pin 1\"  "), "Pin 1");
        assert_eq!(clean_part_name("'SHAPE FOR FW_EXP_CLIP.'"), "FW_EXP_CLIP");
    }

    #[wasm_bindgen_test]
    fn test_step_name_map_synthetic() {
        const STEP_TEXT: &str = "ISO-10303-21;\n\
                                 HEADER;\n\
                                 FILE_DESCRIPTION(('Test'), '2;1');\n\
                                 FILE_NAME('test.stp', '2026-09-01', ('Author'), ('Org'), 'Prep', 'Sys', 'Auth');\n\
                                 FILE_SCHEMA(('AUTOMOTIVE_DESIGN'));\n\
                                 ENDSEC;\n\
                                 DATA;\n\
                                 #10 = CLOSED_SHELL('shell_fallback', (#1));\n\
                                 #20 = MANIFOLD_SOLID_BREP('Bracket_Body', #10);\n\
                                 #30 = ADVANCED_BREP_SHAPE_REPRESENTATION('rep_name', (#20), #5);\n\
                                 #40 = SHAPE_DEFINITION_REPRESENTATION(#50, #30);\n\
                                 #50 = PRODUCT_DEFINITION_SHAPE('', '', #60);\n\
                                 #60 = PRODUCT_DEFINITION('design', '', #70, #8);\n\
                                 #70 = PRODUCT_DEFINITION_FORMATION('1', '', #80);\n\
                                 #80 = PRODUCT('P1', 'Bracket_Product', '', (#9));\n\
                                 ENDSEC;\n\
                                 END-ISO-10303-21;";
        let parsed = crate::ruststep::parser::parse(STEP_TEXT).expect("parsed exchange");
        let map = StepNameMap::from_exchange(&parsed);
        assert_eq!(map.len(), 1);
        // Solid name Bracket_Body takes precedence over product Bracket_Product
        assert_eq!(map.get(10), Some("Bracket_Body"));
    }

    #[wasm_bindgen_test]
    fn test_step_name_map_synthetic_nauo_fallback() {
        const STEP_TEXT: &str = "ISO-10303-21;\n\
                                 HEADER;\n\
                                 FILE_DESCRIPTION(('Test'), '2;1');\n\
                                 FILE_NAME('test.stp', '2026-09-01', ('Author'), ('Org'), 'Prep', 'Sys', 'Auth');\n\
                                 FILE_SCHEMA(('AUTOMOTIVE_DESIGN'));\n\
                                 ENDSEC;\n\
                                 DATA;\n\
                                 #10 = CLOSED_SHELL('', (#1));\n\
                                 #20 = MANIFOLD_SOLID_BREP('#20', #10);\n\
                                 #30 = ADVANCED_BREP_SHAPE_REPRESENTATION('', (#20), #5);\n\
                                 #40 = SHAPE_DEFINITION_REPRESENTATION(#50, #30);\n\
                                 #50 = PRODUCT_DEFINITION_SHAPE('', '', #60);\n\
                                 #60 = PRODUCT_DEFINITION('design', '', #70, #8);\n\
                                 #70 = PRODUCT_DEFINITION_FORMATION('1', '', #80);\n\
                                 #80 = PRODUCT('', '', '', (#9));\n\
                                 #90 = NEXT_ASSEMBLY_USAGE_OCCURRENCE('bolt_1', '', 'bolt_1', #100, #60, $);\n\
                                 ENDSEC;\n\
                                 END-ISO-10303-21;";
        let parsed = crate::ruststep::parser::parse(STEP_TEXT).expect("parsed exchange");
        let map = StepNameMap::from_exchange(&parsed);
        assert_eq!(map.len(), 1);
        // NAUO name bolt_1 is used when product name is empty and solid name is #20 (invalid)
        assert_eq!(map.get(10), Some("bolt_1"));
    }

    #[wasm_bindgen_test]
    fn test_step_name_map_as1_tc_214() {
        const STEP_TEXT: &str = include_str!("../../examples/as1-tc-214.stp");
        let parsed = crate::ruststep::parser::parse(STEP_TEXT).expect("parsed exchange");
        let map = StepNameMap::from_exchange(&parsed);
        assert_eq!(map.len(), 5);

        assert_eq!(map.get(601), Some("l-bracket"));
        assert_eq!(map.get(879), Some("nut"));
        assert_eq!(map.get(1109), Some("bolt"));
        assert_eq!(map.get(1871), Some("plate"));
        assert_eq!(map.get(2018), Some("rod"));
    }

    #[wasm_bindgen_test]
    fn test_step_name_map_part1_ap203() {
        const STEP_TEXT: &str = include_str!("../../examples/Part1.stp");
        let parsed = crate::ruststep::parser::parse(STEP_TEXT).expect("parsed exchange");
        let map = StepNameMap::from_exchange(&parsed);
        assert_eq!(map.len(), 1);
        assert_eq!(map.get(51), Some("PartBody"));
    }

    #[wasm_bindgen_test]
    fn test_step_name_map_kxt_331_lhs() {
        const STEP_TEXT: &str = include_str!("../../examples/KXT_331_LHS.STEP");
        let parsed = crate::ruststep::parser::parse(STEP_TEXT).expect("parsed exchange");
        let map = StepNameMap::from_exchange(&parsed);
        assert_eq!(map.len(), 4);
        assert_eq!(map.get(2543), Some("Pin 1"));
        assert_eq!(map.get(3932), Some("Pin 2"));
        assert_eq!(map.get(4070), Some("Cap"));
        assert_eq!(map.get(4592), Some("Housing"));
    }

    #[wasm_bindgen_test]
    fn test_step_name_map_expansion_card() {
        const STEP_TEXT: &str = include_str!("../../examples/ExpansionCard_SelfTapping.stp");
        let parsed = crate::ruststep::parser::parse(STEP_TEXT).expect("parsed exchange");
        let map = StepNameMap::from_exchange(&parsed);
        assert_eq!(map.len(), 3);
        assert_eq!(map.get(1671), Some("COMPOUND_1"));
        assert_eq!(map.get(8128), Some("FW_EXP_1USBC_FRAME_CLIP_BC_229_"));
        assert_eq!(map.get(9178), Some("STAR_SCREW_M2X3L_298_1"));
    }

    #[wasm_bindgen_test]
    fn test_step_name_map_io1_ca_214_fallback_empty() {
        const STEP_TEXT: &str = include_str!("../../examples/io1-ca-214.stp");
        let parsed = crate::ruststep::parser::parse(STEP_TEXT).expect("parsed exchange");
        let map = StepNameMap::from_exchange(&parsed);
        // All names in io1-ca-214 are 'None' or empty, so no valid names are extracted
        assert!(map.is_empty());
        assert_eq!(map.get(1796), None);
    }
}
