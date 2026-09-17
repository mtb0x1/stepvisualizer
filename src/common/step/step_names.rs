//! STEP ISO 10303 part and mesh name extraction across solids, products,
//! representations, and assembly occurrences.

use smol_str::SmolStr;

use crate::common::{exchange_index::ExchangeIndex, fast_hash::FastU64Map};

/// Extracted mapping of STEP shell entity IDs to resolved, human-readable part names.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct StepNameMap {
    /// Maps STEP entity ID (typically `CLOSED_SHELL` or `OPEN_SHELL`) to a cleaned part name.
    pub shell_names: FastU64Map<SmolStr>,
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

    /// Extracts part names and associates them with shells from a pre-built [`ExchangeIndex`].
    pub fn from_index(index: &ExchangeIndex) -> Self {
        // Gather all shell IDs found in the file and deduplicate in contiguous memory
        let mut all_shells = Vec::with_capacity(
            index.shell_direct_names.len()
                + index.solid_to_shell.len()
                + index.shell_to_solids.len(),
        );
        all_shells.extend(index.shell_direct_names.keys().copied());
        all_shells.extend(index.solid_to_shell.values().copied());
        all_shells.extend(index.shell_to_solids.keys().copied());
        all_shells.sort_unstable();
        all_shells.dedup();

        let mut shell_names = FastU64Map::default();

        for shell_id in all_shells {
            let solids = index.shell_to_solids.get(&shell_id);

            // 1. Check solid name
            let solid_candidate = solids.and_then(|sol_list| {
                sol_list.iter().find_map(|s| index.solid_names.get(s)).map(|s| s.as_str())
            });

            // Find all representations that DIRECTLY contain any of this shell's solids or the
            // shell itself
            let mut matching_reps: Vec<u64> = Vec::new();
            if let Some(reps) = index.item_to_reps.get(&shell_id) {
                matching_reps.extend(reps);
            }
            if let Some(s_list) = solids {
                for s in s_list {
                    if let Some(reps) = index.item_to_reps.get(s) {
                        for &r in reps {
                            if !matching_reps.contains(&r) {
                                matching_reps.push(r);
                            }
                        }
                    }
                }
            }

            // Extract candidate names from matching representations in a single consolidated pass
            let mut prod_candidate = None;
            let mut nauo_candidate = None;
            let mut pd_candidate = None;
            let mut pds_candidate = None;
            let mut rep_candidate = None;

            for &r in &matching_reps {
                if rep_candidate.is_none() {
                    rep_candidate = index.rep_names.get(&r).map(|s| s.as_str());
                }

                if prod_candidate.is_some()
                    && nauo_candidate.is_some()
                    && pd_candidate.is_some()
                    && pds_candidate.is_some()
                {
                    continue;
                }

                if let Some(pds) = resolve_pds_for_rep(r, &index.shape_rep_to_pds, &index.rep_links)
                {
                    if pds_candidate.is_none() {
                        pds_candidate = index.pds_names.get(&pds).map(|s| s.as_str());
                    }

                    if let Some(pd) = index.pds_to_pd.get(&pds) {
                        if nauo_candidate.is_none() {
                            nauo_candidate = index.nauo_names.get(pd).map(|s| s.as_str());
                        }
                        if pd_candidate.is_none() {
                            pd_candidate = index.pd_names.get(pd).map(|s| s.as_str());
                        }

                        if prod_candidate.is_none()
                            && let Some(pdf) = index.pd_to_pdf.get(pd)
                            && let Some(prod) = index.pdf_to_prod.get(pdf)
                        {
                            prod_candidate = index.prod_names.get(prod).map(|s| s.as_str());
                        }
                    }
                }
            }

            // 7. Check shell direct name
            let shell_candidate = index.shell_direct_names.get(&shell_id).map(|s| s.as_str());

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
                shell_names.insert(shell_id, SmolStr::new(name));
            }
        }

        Self { shell_names }
    }
}

/// Resolves the associated `PRODUCT_DEFINITION_SHAPE` for a representation,
/// checking both direct mapping and representation relationships.
fn resolve_pds_for_rep(
    rep_id: u64, shape_rep_to_pds: &FastU64Map<u64>, rep_links: &[(u64, u64)],
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
/// - If the solid name is purely numeric and a non-numeric product/assembly name is available,
///   prefer the latter.
/// - Otherwise fall back through Product -> NAUO -> Product Definition -> Shape -> Representation
///   -> Shell.
fn select_best_name<'a>(
    solid: Option<&'a str>, prod: Option<&'a str>, nauo: Option<&'a str>, pd: Option<&'a str>,
    pds: Option<&'a str>, rep: Option<&'a str>, shell: Option<&'a str>,
) -> Option<&'a str> {
    if let Some(s) = solid {
        let is_pure_digit = s.chars().all(|c| c.is_ascii_digit());
        if !is_pure_digit {
            return Some(s);
        }
        // If solid is pure digits like "1", but we have a descriptive product/assembly name, prefer
        // that
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
/// - CAD null/unspecified placeholders: "NONE", "None", "unspecified", "not specified", "null",
///   "no_name", "default", "undefined"
/// - Entity references such as "#602" or lone "#", "$", "*"
#[inline]
pub fn is_valid_part_name(s: &str) -> bool {
    let clean = clean_part_name_str(s);
    if clean.is_empty() {
        return false;
    }
    // Filter out STEP entity pointer labels like "#602", "#123"
    if clean.starts_with('#') && clean[1 ..].chars().all(|c| c.is_ascii_digit()) {
        return false;
    }
    if clean == "#" || clean == "$" || clean == "*" {
        return false;
    }
    !(clean.eq_ignore_ascii_case("none")
        || clean.eq_ignore_ascii_case("null")
        || clean.eq_ignore_ascii_case("na")
        || clean.eq_ignore_ascii_case("n/a")
        || clean.eq_ignore_ascii_case("unspecified")
        || clean.eq_ignore_ascii_case("not specified")
        || clean.eq_ignore_ascii_case("no_name")
        || clean.eq_ignore_ascii_case("no name")
        || clean.eq_ignore_ascii_case("default")
        || clean.eq_ignore_ascii_case("none/default")
        || clean.eq_ignore_ascii_case("undefined")
        || clean.eq_ignore_ascii_case("solid")
        || clean.eq_ignore_ascii_case("part"))
}

/// Zero-allocation view of a cleaned part name, trimming quotes, whitespace, and descriptive CAD
/// prefixes like "SHAPE FOR ".
#[inline]
pub fn clean_part_name_str(s: &str) -> &str {
    let trimmed = s.trim().trim_matches('\'').trim_matches('"').trim();
    if let Some(stripped) = trimmed.strip_prefix("SHAPE FOR ") {
        stripped.trim().trim_end_matches('.').trim()
    } else if let Some(stripped) = trimmed.strip_prefix("shape for ") {
        stripped.trim().trim_end_matches('.').trim()
    } else {
        trimmed
    }
}

/// Cleans a part name by trimming quotes, whitespace, and descriptive CAD prefixes like "SHAPE FOR
/// ".
#[inline]
pub fn clean_part_name(s: &str) -> SmolStr {
    SmolStr::new(clean_part_name_str(s))
}
