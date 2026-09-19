use std::{cell::RefCell, rc::Rc};

use yew::prelude::*;

use crate::{
    common::{Metadata, StepModel},
    storage::{LruCache, save_model},
    workspace::state::StateHandles,
};

fn recompute_and_store_metric(
    states: &StateHandles, cache: &Rc<RefCell<LruCache>>, compute: impl Fn(&StepModel) -> f64,
    apply: impl Fn(&mut Metadata, f64),
) {
    if let Some(view) = states.step_model.as_ref() {
        let total = compute(&view.model);

        let mut new_meta = view.model.metadata.clone();
        apply(&mut new_meta, total);
        states.metadata.set(Some(new_meta.clone()));

        let mut model_rc = view.model.clone();
        let model_mut = Rc::make_mut(&mut model_rc);
        model_mut.metadata = new_meta;

        let states_async = states.clone();
        let new_model_owned = (*model_mut).clone();
        wasm_bindgen_futures::spawn_local(async move {
            if let Err(e) = save_model(&new_model_owned).await {
                states_async.set_result(format!("Persistence error: {}", e), true);
            }
        });

        cache.borrow_mut().insert_rc(model_rc.id.clone(), model_rc.clone());

        let new_view =
            crate::workspace::state::ModelView { model: model_rc, generation: view.generation };
        states.step_model.set(Some(new_view));
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
    states: &StateHandles, cache: Rc<RefCell<LruCache>>,
) -> ModelActions {
    let on_visibility_change = {
        let step_model = states.step_model.clone();
        let states_for_cb = states.clone();
        Callback::from(move |(index, visible): (usize, bool)| {
            if let Some(view) = step_model.as_ref() {
                let mut new_model = (*view.model).clone();
                if index < new_model.part_visibility.len() {
                    new_model.part_visibility[index] = visible;
                    new_model.visibility_generation += 1;

                    let new_view = crate::workspace::state::ModelView {
                        model: Rc::new(new_model.clone()),
                        generation: view.generation + 1,
                    };
                    step_model.set(Some(new_view));

                    let states_async = states_for_cb.clone();
                    wasm_bindgen_futures::spawn_local(async move {
                        if let Err(e) = save_model(&new_model).await {
                            states_async.set_result(format!("Persistence error: {}", e), true);
                        }
                    });
                }
            }
        })
    };

    let on_show_all = {
        let step_model = states.step_model.clone();
        let states_for_cb = states.clone();
        Callback::from(move |_| {
            if let Some(view) = step_model.as_ref() {
                let mut new_model = (*view.model).clone();
                new_model.part_visibility = vec![true; new_model.part_visibility.len()];
                new_model.visibility_generation += 1;

                let new_view = crate::workspace::state::ModelView {
                    model: Rc::new(new_model.clone()),
                    generation: view.generation + 1,
                };
                step_model.set(Some(new_view));

                let states_async = states_for_cb.clone();
                wasm_bindgen_futures::spawn_local(async move {
                    if let Err(e) = save_model(&new_model).await {
                        states_async.set_result(format!("Persistence error: {}", e), true);
                    }
                });
            }
        })
    };

    let on_hide_all = {
        let step_model = states.step_model.clone();
        let states_for_cb = states.clone();
        Callback::from(move |_| {
            if let Some(view) = step_model.as_ref() {
                let mut new_model = (*view.model).clone();
                new_model.part_visibility = vec![false; new_model.part_visibility.len()];
                new_model.visibility_generation += 1;

                let new_view = crate::workspace::state::ModelView {
                    model: Rc::new(new_model.clone()),
                    generation: view.generation + 1,
                };
                step_model.set(Some(new_view));

                let states_async = states_for_cb.clone();
                wasm_bindgen_futures::spawn_local(async move {
                    if let Err(e) = save_model(&new_model).await {
                        states_async.set_result(format!("Persistence error: {}", e), true);
                    }
                });
            }
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
