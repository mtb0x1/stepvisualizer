//! AST extraction and parsing utilities for `ruststep` types.

use crate::ruststep::ast::{Parameter, Record};

/// Trait for types that can be extracted from a `ruststep` AST `Parameter`.
pub trait TryExtractParam<'a>: Sized {
    fn try_extract(param: &'a Parameter) -> Option<Self>;
}

impl<'a> TryExtractParam<'a> for &'a [Parameter] {
    #[inline(always)]
    fn try_extract(param: &'a Parameter) -> Option<&'a [Parameter]> {
        match param {
            Parameter::List(list) => Some(list.as_slice()),
            _ => None,
        }
    }
}

impl<'a> TryExtractParam<'a> for &'a str {
    #[inline(always)]
    fn try_extract(param: &'a Parameter) -> Option<&'a str> {
        match param {
            Parameter::String(value) | Parameter::Enumeration(value) => Some(value.as_str()),
            _ => None,
        }
    }
}

impl<'a> TryExtractParam<'a> for u64 {
    #[inline(always)]
    fn try_extract(param: &'a Parameter) -> Option<u64> {
        match param {
            Parameter::Ref(crate::ruststep::ast::Name::Entity(id)) => Some(*id),
            _ => None,
        }
    }
}

impl<'a> TryExtractParam<'a> for f64 {
    #[inline(always)]
    fn try_extract(param: &'a Parameter) -> Option<f64> {
        match param {
            Parameter::Real(v) => Some(*v),
            Parameter::Integer(v) => Some(*v as f64),
            _ => None,
        }
    }
}

impl<'a> TryExtractParam<'a> for glam::DVec3 {
    #[inline]
    fn try_extract(param: &'a Parameter) -> Option<glam::DVec3> {
        let list = param.try_extract::<&[Parameter]>()?;
        if list.len() < 3 {
            return None;
        }
        Some(glam::DVec3::new(
            list[0].try_extract::<f64>()?,
            list[1].try_extract::<f64>()?,
            list[2].try_extract::<f64>()?,
        ))
    }
}

/// Extension trait for `Parameter` to allow ergonomic extraction.
pub trait ParameterExt {
    fn try_extract<'a, T: TryExtractParam<'a>>(&'a self) -> Option<T>;
}

impl ParameterExt for Parameter {
    #[inline(always)]
    fn try_extract<'a, T: TryExtractParam<'a>>(&'a self) -> Option<T> {
        T::try_extract(self)
    }
}

/// Recursively extracts all numeric entity IDs referenced within a `Parameter` (handling nested lists).
#[inline]
pub fn extract_entity_refs(param: &Parameter) -> Vec<u64> {
    extract_entity_refs_with_capacity(param, 0)
}

#[inline]
pub fn extract_entity_refs_with_capacity(param: &Parameter, cap: usize) -> Vec<u64> {
    let mut refs = Vec::with_capacity(cap);
    collect_refs_recursive(param, &mut |id| refs.push(id));
    refs
}

#[inline]
pub fn extract_smallvec_refs<const N: usize>(param: &Parameter) -> smallvec::SmallVec<[u64; N]>
where
    [u64; N]: smallvec::Array<Item = u64>,
{
    let mut refs = smallvec::SmallVec::new();
    collect_refs_recursive(param, &mut |id| refs.push(id));
    refs
}

/// Helper for recursive collection of entity references within a `Parameter`.
#[inline]
pub fn collect_refs_recursive(param: &Parameter, push: &mut impl FnMut(u64)) {
    match param {
        Parameter::Ref(crate::ruststep::ast::Name::Entity(id)) => push(*id),
        Parameter::List(list) => {
            for item in list {
                collect_refs_recursive(item, push);
            }
        }
        _ => {}
    }
}

/// Extracts the 3D direction vector `[dx, dy, dz]` from a `DIRECTION` entity record as `DVec3`.
#[inline]
pub fn extract_direction_coords(record: &Record) -> Option<glam::DVec3> {
    record
        .parameter
        .try_extract::<&[Parameter]>()?
        .get(1)?
        .try_extract::<glam::DVec3>()
}

/// Tests whether a direction vector is approximately the positive Z unit vector `(0, 0, 1)`.
#[inline]
pub fn is_unit_z_direction(v: glam::DVec3) -> bool {
    let norm = v.normalize_or_zero();
    (norm - glam::DVec3::Z).length_squared() < 1e-8
}

/// Tests whether a direction vector is collinear or antiparallel with the global X-axis `(1, 0, 0)`.
///
/// Uses the normalized squared cross product with `(1, 0, 0)`:
/// `sin^2(theta) = ||v x X||^2 / ||v||^2`
#[inline]
pub fn is_collinear_with_x(v: glam::DVec3) -> bool {
    let len_sq = v.length_squared();
    if len_sq < 1e-12 {
        return false;
    }
    v.cross(glam::DVec3::X).length_squared() / len_sq < 1e-4
}
