//! AST extraction and parsing utilities for `ruststep` types.

use crate::ruststep::ast::Parameter;

/// Extracts a slice of `Parameter`s if the parameter is a `Parameter::List`.
#[inline(always)]
pub const fn param_as_list(param: &Parameter) -> Option<&[Parameter]> {
    match param {
        Parameter::List(list) => Some(list.as_slice()),
        _ => None,
    }
}

/// Extracts the string slice if the parameter is a `Parameter::Enumeration`.
#[inline(always)]
pub const fn param_as_enum(param: &Parameter) -> Option<&str> {
    match param {
        Parameter::Enumeration(value) => Some(value.as_str()),
        _ => None,
    }
}

/// Extracts a string slice if the parameter is either `Parameter::Enumeration` or `Parameter::String`.
#[inline(always)]
pub const fn param_as_str(param: &Parameter) -> Option<&str> {
    match param {
        Parameter::Enumeration(value) => Some(value.as_str()),
        Parameter::String(value) => Some(value.as_str()),
        _ => None,
    }
}

/// Extracts the numeric entity ID if the parameter is a `Parameter::Ref(Name::Entity(id))`.
#[inline(always)]
pub const fn param_as_ref(param: &Parameter) -> Option<u64> {
    match param {
        Parameter::Ref(crate::ruststep::ast::Name::Entity(id)) => Some(*id),
        _ => None,
    }
}

/// Extracts a float value if the parameter is `Parameter::Real` or `Parameter::Integer`.
#[inline(always)]
pub const fn param_as_real(param: &Parameter) -> Option<f64> {
    match param {
        Parameter::Real(v) => Some(*v),
        Parameter::Integer(v) => Some(*v as f64),
        _ => None,
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
