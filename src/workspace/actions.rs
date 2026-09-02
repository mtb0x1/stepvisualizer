//! Model interaction callbacks: visibility toggling and geometric metrics calculation.
use crate::common::{LruCache, Metadata, StepModel, save_model};
use crate::workspace::state::StateHandles;
use std::cell::RefCell;
use std::rc::Rc;
use yew::prelude::*;

fn recompute_and_store_metric(
    states: &StateHandles,
    cache: &Rc<RefCell<LruCache>>,
    compute: impl Fn(&StepModel) -> f64,
    apply: impl Fn(&mut Metadata, f64),
) {
    if let Some(model_rc) = states.step_model.as_ref() {
        let total = compute(model_rc);

        let mut new_meta = model_rc.metadata.clone();
        apply(&mut new_meta, total);
        states.metadata.set(Some(new_meta.clone()));

        let mut model_rc = model_rc.clone();
        let model_mut = Rc::make_mut(&mut model_rc);
        model_mut.metadata = new_meta;

        save_model(model_mut);
        cache
            .borrow_mut()
            .insert_rc(model_rc.id.clone(), model_rc.clone());

        states.step_model.set(Some(model_rc));
    }
}

/// Per-model interaction callbacks returned by [`use_model_actions`].
pub(crate) struct ModelActions {
    pub on_visibility_change: Callback<(usize, bool)>,
    pub on_show_all: Callback<()>,
    pub on_hide_all: Callback<()>,
    pub on_calculate_volume: Callback<()>,
    pub on_calculate_surface: Callback<()>,
}

#[hook]
pub(crate) fn use_model_actions(
    states: &StateHandles,
    cache: Rc<RefCell<LruCache>>,
) -> ModelActions {
    let on_visibility_change = {
        let part_visibility = states.part_visibility.clone();
        Callback::from(move |(index, visible): (usize, bool)| {
            let mut new_visibility = (*part_visibility).clone();
            if index < new_visibility.len() {
                new_visibility[index] = visible;
                part_visibility.set(new_visibility);
            }
        })
    };

    let on_show_all = {
        let part_visibility = states.part_visibility.clone();
        Callback::from(move |_| {
            part_visibility.set(vec![true; part_visibility.len()]);
        })
    };

    let on_hide_all = {
        let part_visibility = states.part_visibility.clone();
        Callback::from(move |_| {
            part_visibility.set(vec![false; part_visibility.len()]);
        })
    };

    let on_calculate_volume = {
        let states = states.clone();
        let cache = cache.clone();
        Callback::from(move |_| {
            recompute_and_store_metric(
                &states,
                &cache,
                |m| m.calculate_total_volume(),
                |meta, value| meta.volume = Some(value),
            );
        })
    };

    let on_calculate_surface = {
        let states = states.clone();
        let cache = cache.clone();
        Callback::from(move |_| {
            recompute_and_store_metric(
                &states,
                &cache,
                |m| m.calculate_total_surface_area(),
                |meta, value| meta.surface_area = Some(value),
            );
        })
    };

    ModelActions {
        on_visibility_change,
        on_show_all,
        on_hide_all,
        on_calculate_volume,
        on_calculate_surface,
    }
}
