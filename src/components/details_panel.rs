use crate::common::{Metadata, NA};
use crate::trace_span;
use yew::prelude::*;

#[derive(Properties, PartialEq)]
pub struct DetailsPanelProps {
    pub metadata: Option<Metadata>,
}

#[function_component(DetailsPanel)]
pub fn details_panel(props: &DetailsPanelProps) -> Html {
    trace_span!("details_panel");
    html! {
        <div class="panel panel-details">
            <div class="panel-header">
                <span>{ "Details " }</span>
                <span class="icon fas fa-circle-info"></span>
            </div>
            <div class="panel-content">
                if let Some(meta) = &props.metadata {
                    <dl class="details-list">
                        <div class="detail-item">
                            <dt class="detail-label">{ "File name :" }</dt>
                            <dd>{ &meta.header.file_name }</dd>
                        </div>
                        <div class="detail-item">
                            <dt class="detail-label">{ "Implementation level :" }</dt>
                            <dd>{ &meta.header.implementation_level }</dd>
                        </div>
                        <div class="detail-item">
                            <dt class="detail-label">{ "Time stamp :" }</dt>
                            <dd>{ &meta.header.time_stamp }</dd>
                        </div>
                        <div class="detail-item">
                            <dt class="detail-label">{ "Author(s) :" }</dt>
                            <dd>{ meta.header.author.join(", ") }</dd>
                        </div>
                        <div class="detail-item">
                            <dt class="detail-label">{ "Organization(s) :" }</dt>
                            <dd>{ meta.header.organization.join(", ") }</dd>
                        </div>
                        <div class="detail-item">
                            <dt class="detail-label">{ "Preprocessor :" }</dt>
                            <dd>{ &meta.header.preprocessor_version }</dd>
                        </div>
                        <div class="detail-item">
                            <dt class="detail-label">{ "Originating system :" }</dt>
                            <dd>{ &meta.header.originating_system }</dd>
                        </div>
                        <div class="detail-item">
                            <dt class="detail-label">{ "Authorization :" }</dt>
                            <dd>{ &meta.header.authorization }</dd>
                        </div>
                        <div class="detail-item">
                            <dt class="detail-label">{ "Description :" }</dt>
                            <dd>{ &meta.header.file_description }</dd>
                        </div>
                        <div class="detail-item">
                            <dt class="detail-label">{ "Schema :" }</dt>
                            <dd>{ &meta.header.file_schema }</dd>
                        </div>
                        <div class="detail-item">
                            <dt class="detail-label">{ "Entity count :" }</dt>
                            <dd>{ meta.entity_count }</dd>
                        </div>
                        <div class="detail-item">
                            <dt class="detail-label">{ "Bounding box :" }</dt>
                            <dd>
                                {
                                    if let Some(bb) = &meta.bounding_box {
                                        html! {
                                            <>
                                                <span class="bbox-value">
                                                    { format!("min: {:.3}, {:.3}, {:.3}", bb.min[0], bb.min[1], bb.min[2]) }
                                                </span>
                                                <br/>
                                                <span class="bbox-value">
                                                    { format!("max: {:.3}, {:.3}, {:.3}", bb.max[0], bb.max[1], bb.max[2]) }
                                                </span>
                                            </>
                                        }
                                    } else {
                                        html! { { NA.to_string() } }
                                    }
                                }
                            </dd>
                        </div>
                        <div class="detail-item">
                            <dt class="detail-label">{ "Units :" }</dt>
                            <dd>{ meta.units.clone().unwrap_or_else(|| NA.to_string()) }</dd>
                        </div>

                        <div class="detail-item">
                            <dt class="detail-label">{ "Vertices :" }</dt>
                            <dd>{ meta.vertex_count }</dd>
                        </div>
                        <div class="detail-item">
                            <dt class="detail-label">{ "Triangles :" }</dt>
                            <dd>{ meta.triangle_count }</dd>
                        </div>
                        <div class="detail-item">
                            <dt class="detail-label">{ "Unit system :" }</dt>
                            <dd>{ meta.units.clone().unwrap_or_else(|| NA.to_string()) }</dd>
                        </div>
                        if let Some(bb) = &meta.bounding_box {
                            <div class="detail-item">
                                <dt class="detail-label">{ "Size X :" }</dt>
                                <dd>{ format!("{:.2}", bb.max[0] - bb.min[0]) }</dd>
                            </div>
                            <div class="detail-item">
                                <dt class="detail-label">{ "Size Y :" }</dt>
                                <dd>{ format!("{:.2}", bb.max[1] - bb.min[1]) }</dd>
                            </div>
                            <div class="detail-item">
                                <dt class="detail-label">{ "Size Z :" }</dt>
                                <dd>{ format!("{:.2}", bb.max[2] - bb.min[2]) }</dd>
                            </div>
                        }
                        // TODO: Implement calculation logic for Volume and Surface Area.
                        // These buttons should trigger backend calculations and display the results.
                        <div class="detail-item">
                            <dt class="detail-label">{ "Volume:" }</dt>
                            <dd class="calculate">{ "Calculate..." }</dd>
                        </div>
                        <div class="detail-item">
                            <dt class="detail-label">{ "Surface:" }</dt>
                            <dd class="calculate">{ "Calculate..." }</dd>
                        </div>
                    </dl>
                } else {
                   <div class="empty-files-message">{ "No file loaded/Selected yet. Please select or upload a file" }</div>
                }
            </div>
        </div>
    }
}
