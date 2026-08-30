use yew::prelude::*;

#[derive(Properties, PartialEq)]
pub struct ConfirmModalProps {
    pub title: AttrValue,
    pub message: AttrValue,
    pub confirm_label: AttrValue,
    pub on_confirm: Callback<()>,
    pub on_cancel: Callback<()>,
}

#[function_component(ConfirmModal)]
pub fn confirm_modal(props: &ConfirmModalProps) -> Html {
    let on_confirm_click = {
        let cb = props.on_confirm.clone();
        Callback::from(move |e: MouseEvent| {
            e.stop_propagation();
            cb.emit(());
        })
    };

    let on_cancel_click = {
        let cb = props.on_cancel.clone();
        Callback::from(move |e: MouseEvent| {
            e.stop_propagation();
            cb.emit(());
        })
    };

    html! {
        <div class="modal-backdrop" onclick={on_cancel_click.clone()}>
            <div class="modal-dialog" onclick={Callback::from(|e: MouseEvent| e.stop_propagation())}>
                <div class="modal-header">
                    <h3 class="modal-title">{ &props.title }</h3>
                </div>
                <div class="modal-body">
                    <p>{ &props.message }</p>
                </div>
                <div class="modal-footer">
                    <button class="modal-btn modal-btn-cancel" onclick={on_cancel_click}>
                        { "Cancel" }
                    </button>
                    <button class="modal-btn modal-btn-danger" onclick={on_confirm_click}>
                        { &props.confirm_label }
                    </button>
                </div>
            </div>
        </div>
    }
}
