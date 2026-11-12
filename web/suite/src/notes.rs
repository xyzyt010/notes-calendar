//! Notes API: CRUD + list/search/folders/tags.

use axum::{extract::State, http::StatusCode, response::IntoResponse, routing::{get, post}, Json, Router};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth::require_auth;
use crate::state::AppState;
use ycore::model::{Folder, Note};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/notes", get(list_notes).post(create_note))
        .route("/api/notes/{id}", get(get_note).put(update_note).delete(delete_note))
        .route("/api/notes/purge-trash", post(purge_trash))
        .route("/api/folders", get(list_folders).post(create_folder))
        .route("/api/folders/{id}", axum::routing::delete(delete_folder))
        .route("/api/tags", get(list_tags))
}

#[derive(Deserialize)]
struct ListQuery {
    q: Option<String>,
    folder: Option<String>,
    tag: Option<String>,
    archived: Option<bool>,
    trashed: Option<bool>,
    favorites: Option<bool>,
    limit: Option<i64>,
}

#[derive(Serialize)]
struct NoteOut {
    id: String,
    title: String,
    body: String,
    folder_id: Option<String>,
    tags: Vec<String>,
    pinned: bool,
    favorite: bool,
    color: Option<String>,
    archived: bool,
    trashed: bool,
    created_at: String,
    updated_at: String,
}

fn note_out(n: &Note) -> NoteOut {
    NoteOut {
        id: n.id.to_string(),
        title: n.title.clone(),
        body: n.body.clone(),
        folder_id: n.folder_id.map(|f| f.to_string()),
        tags: n.tags.clone(),
        pinned: n.pinned,
        favorite: n.favorite,
        color: n.color.clone(),
        archived: n.archived,
        trashed: n.trashed,
        created_at: n.created_at.to_rfc3339(),
        updated_at: n.updated_at.to_rfc3339(),
    }
}

async fn list_notes(
    State(state): State<AppState>,
    headers: axum::http::header::HeaderMap,
    axum::extract::Query(q): axum::extract::Query<ListQuery>,
) -> impl IntoResponse {
    if require_auth(&state, &headers).is_err() {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let filter = ycore::storage::notes::ListFilter {
        folder_id: q.folder.as_deref().and_then(|s| Uuid::parse_str(s).ok()),
        tag: q.tag.as_deref(),
        archived: q.archived.unwrap_or(false),
        trashed: q.trashed.unwrap_or(false),
        favorites_only: q.favorites.unwrap_or(false),
        query: q.q.as_deref().filter(|s| !s.is_empty()),
        limit: q.limit.unwrap_or(500),
    };
    let db = state.db.lock().unwrap();
    match ycore::storage::notes::list_notes(&db, &filter) {
        Ok(list) => Json(list).into_response(),
        Err(e) => {
            tracing::error!("list_notes: {e}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

#[derive(Deserialize)]
pub struct CreateNoteRequest {
    pub title: Option<String>,
    pub body: Option<String>,
    pub folder_id: Option<String>,
}

async fn create_note(
    State(state): State<AppState>,
    headers: axum::http::header::HeaderMap,
    Json(body): Json<CreateNoteRequest>,
) -> impl IntoResponse {
    if require_auth(&state, &headers).is_err() {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let mut note = Note::new(
        body.title.unwrap_or_default(),
        body.body.unwrap_or_default(),
    );
    note.folder_id = body.folder_id.as_deref().and_then(|s| Uuid::parse_str(s).ok());
    let db = state.db.lock().unwrap();
    match ycore::storage::notes::create_note(&db, &note) {
        Ok(_) => (StatusCode::CREATED, Json(note_out(&note))).into_response(),
        Err(e) => {
            tracing::error!("create_note: {e}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

async fn get_note(
    State(state): State<AppState>,
    headers: axum::http::header::HeaderMap,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> impl IntoResponse {
    if require_auth(&state, &headers).is_err() {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let Ok(id) = Uuid::parse_str(&id) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let db = state.db.lock().unwrap();
    match ycore::storage::notes::get_note(&db, id) {
        Ok(Some(n)) => Json(note_out(&n)).into_response(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(e) => {
            tracing::error!("get_note: {e}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

#[derive(Deserialize)]
pub struct UpdateNoteRequest {
    pub title: Option<String>,
    pub body: Option<String>,
    pub pinned: Option<bool>,
    pub favorite: Option<bool>,
    pub archived: Option<bool>,
    pub trashed: Option<bool>,
    pub color: Option<String>,
    /// Tags to add/remove
    pub tags: Option<Vec<String>>,
}

async fn update_note(
    State(state): State<AppState>,
    headers: axum::http::header::HeaderMap,
    axum::extract::Path(id): axum::extract::Path<String>,
    Json(body): Json<UpdateNoteRequest>,
) -> impl IntoResponse {
    if require_auth(&state, &headers).is_err() {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let Ok(id) = Uuid::parse_str(&id) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let db = state.db.lock().unwrap();
    let mut note = match ycore::storage::notes::get_note(&db, id) {
        Ok(Some(n)) => n,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(e) => {
            tracing::error!("get_note for update: {e}");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };
    if let Some(t) = body.title { note.title = t; }
    if let Some(b) = body.body { note.body = b; }
    if let Some(p) = body.pinned { note.pinned = p; }
    if let Some(f) = body.favorite { note.favorite = f; }
    if let Some(a) = body.archived { note.archived = a; }
    if let Some(t) = body.trashed { note.trashed = t; }
    if let Some(c) = body.color { note.color = Some(c); }
    if let Some(t) = body.tags { note.tags = t; }
    match ycore::storage::notes::update_note(&db, &note) {
        Ok(_) => {
            let updated = ycore::storage::notes::get_note(&db, note.id).unwrap().unwrap();
            Json(note_out(&updated)).into_response()
        }
        Err(e) => {
            tracing::error!("update_note: {e}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

async fn delete_note(
    State(state): State<AppState>,
    headers: axum::http::header::HeaderMap,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> impl IntoResponse {
    if require_auth(&state, &headers).is_err() {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let Ok(id) = Uuid::parse_str(&id) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let db = state.db.lock().unwrap();
    match ycore::storage::notes::delete_note_hard(&db, id) {
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => {
            tracing::error!("delete_note: {e}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

async fn purge_trash(
    State(state): State<AppState>,
    headers: axum::http::header::HeaderMap,
) -> impl IntoResponse {
    if require_auth(&state, &headers).is_err() {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let db = state.db.lock().unwrap();
    let list = ycore::storage::notes::list_notes(
        &db,
        &ycore::storage::notes::ListFilter {
            trashed: true,
            ..Default::default()
        },
    )
    .unwrap_or_default();
    for n in list {
        let _ = ycore::storage::notes::delete_note_hard(&db, n.id);
    }
    StatusCode::NO_CONTENT.into_response()
}

#[derive(Deserialize)]
pub struct FolderRequest {
    pub name: Option<String>,
}

async fn create_folder(
    State(state): State<AppState>,
    headers: axum::http::header::HeaderMap,
    Json(body): Json<FolderRequest>,
) -> impl IntoResponse {
    if require_auth(&state, &headers).is_err() {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let name = body.name.unwrap_or_else(|| "New folder".to_string());
    let folder = Folder::new(name);
    let db = state.db.lock().unwrap();
    match ycore::storage::notes::create_folder(&db, &folder) {
        Ok(_) => (StatusCode::CREATED, Json(serde_json::json!({"id": folder.id.to_string(), "name": folder.name}))).into_response(),
        Err(e) => {
            tracing::error!("create_folder: {e}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

async fn list_folders(
    State(state): State<AppState>,
    headers: axum::http::header::HeaderMap,
) -> impl IntoResponse {
    if require_auth(&state, &headers).is_err() {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let db = state.db.lock().unwrap();
    match ycore::storage::notes::list_folders(&db) {
        Ok(list) => Json(list).into_response(),
        Err(e) => {
            tracing::error!("list_folders: {e}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

async fn delete_folder(
    State(state): State<AppState>,
    headers: axum::http::header::HeaderMap,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> impl IntoResponse {
    if require_auth(&state, &headers).is_err() {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let Ok(id) = Uuid::parse_str(&id) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let db = state.db.lock().unwrap();
    match ycore::storage::notes::delete_folder(&db, id) {
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => {
            tracing::error!("delete_folder: {e}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

async fn list_tags(
    State(state): State<AppState>,
    headers: axum::http::header::HeaderMap,
) -> impl IntoResponse {
    if require_auth(&state, &headers).is_err() {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let db = state.db.lock().unwrap();
    match ycore::storage::notes::all_tags(&db) {
        Ok(tags) => Json(tags.into_iter().map(|(name, count)| serde_json::json!({"name": name, "count": count})).collect::<Vec<_>>()).into_response(),
        Err(e) => {
            tracing::error!("list_tags: {e}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}
