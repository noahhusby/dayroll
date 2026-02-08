use axum::{extract::State, routing::get, Json, Router};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use diesel::RunQueryDsl;
use serde::Serialize;
use crate::db;
use crate::state::AppState;

#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
    db: &'static str,
}

pub fn router() -> Router<AppState> {
    Router::new().route("/", get(get_health))
}

async fn get_health(State(state): State<AppState>) -> Json<HealthResponse> {
    let db_ok = db::run_blocking_db(|conn| {
        diesel::sql_query("SELECT 1").execute(conn)?;
        Ok::<(), anyhow::Error>(())
    })
        .await
        .is_ok();

    Json(HealthResponse { status: "ok", db: if db_ok { "ok" } else { "down" } })
}
