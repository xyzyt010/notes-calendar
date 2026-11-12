//! yellod suite — notes & calendar web app backend.
//! Efficient Rust (axum + SQLite via ycore). Serves static frontends for
//! /notes and /calendar plus a JSON API (both apps use it).
//! Auth: opaque random session tokens (httpOnly cookie), verified server-side.
//! Efficient: zero crypto beyond bcrypt for the login itself.

use axum::Router;
use tower_http::services::ServeDir;

mod auth;
mod calendar;
mod notes;
mod state;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env()
            .add_directive("suite_web=info".parse().unwrap()))
        .init();

    let state = state::AppState::from_env()?;
    let bind_addr = state.bind_addr;
    tracing::info!("yellod suite listening on {}", bind_addr);

    let app = Router::new()
        .route("/", axum::routing::get(|| async { axum::response::Redirect::to("/index.html") }))
        .route("/notes", axum::routing::get(|| async { axum::response::Redirect::to("/notes.html") }))
        .route("/calendar", axum::routing::get(|| async { axum::response::Redirect::to("/calendar.html") }))
        .route("/api/health", axum::routing::get(|| async { axum::Json(serde_json::json!({"ok": true})) }))
        .route("/api/auth/login", axum::routing::post(auth::login))
        .route("/api/auth/logout", axum::routing::post(auth::logout))
        .route("/api/auth/me", axum::routing::get(auth::me))
        .merge(notes::routes())
        .merge(calendar::routes())
        .fallback_service(ServeDir::new(&state.static_dir).append_index_html_on_directories(true))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(bind_addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
