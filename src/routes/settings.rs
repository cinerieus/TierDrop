use askama::Template;
use askama_web::WebTemplate;
use axum::extract::State;
use axum::response::IntoResponse;

use crate::routes::backup::BackupStatus;
use crate::state::AppState;

#[derive(Template, WebTemplate)]
#[template(path = "settings.html")]
pub struct SettingsTemplate {
    pub node_address: Option<String>,
    pub network_count: usize,
    pub backup_type: String,
}

pub async fn settings_page(State(state): State<AppState>) -> impl IntoResponse {
    let status = BackupStatus::fetch(&state).await;
    let backup_type = status.backup_type().to_string();

    SettingsTemplate {
        node_address: status.node_address,
        network_count: status.network_count,
        backup_type,
    }
}
