//! User interface module: panels, viewports, dialogs, and reusable components.

pub mod components;
pub mod left_panel;
pub mod main_panel;
pub mod right_panel;

pub use components::{
    confirm_modal::ConfirmModal, upload_bar::UploadBar, webgpu_unavailable::WebGpuUnavailable,
};
pub use left_panel::LeftPanel;
pub use main_panel::AppStepVisualizer;
pub use right_panel::RightPanel;
