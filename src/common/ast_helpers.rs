//! AST extraction and parsing utilities for `ruststep` types.

use crate::ruststep::ast::Parameter;

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
    let mut refs = Vec::new();
    collect_refs_recursive(param, &mut refs);
    refs
}

#[inline]
pub fn extract_entity_refs_with_capacity(param: &Parameter, cap: usize) -> Vec<u64> {
    let mut refs = Vec::with_capacity(cap);
    collect_refs_recursive(param, &mut refs);
    refs
}

#[inline]
pub fn extract_smallvec_refs<const N: usize>(param: &Parameter) -> smallvec::SmallVec<[u64; N]>
where
    [u64; N]: smallvec::Array<Item = u64>,
{
    let mut refs = smallvec::SmallVec::new();
    collect_refs_recursive_small(&mut refs, param);
    refs
}

/// Helper for recursive collection of entity references within a `Parameter`.
pub fn collect_refs_recursive(param: &Parameter, out: &mut Vec<u64>) {
    match param {
        Parameter::Ref(crate::ruststep::ast::Name::Entity(id)) => out.push(*id),
        Parameter::List(list) => {
            for item in list {
                collect_refs_recursive(item, out);
            }
        }
        _ => {}
    }
}

pub fn collect_refs_recursive_small<const N: usize>(
    out: &mut smallvec::SmallVec<[u64; N]>,
    param: &Parameter,
) where
    [u64; N]: smallvec::Array<Item = u64>,
{
    match param {
        Parameter::Ref(crate::ruststep::ast::Name::Entity(id)) => out.push(*id),
        Parameter::List(list) => {
            for item in list {
                collect_refs_recursive_small(out, item);
            }
        }
        _ => {}
    }
}
