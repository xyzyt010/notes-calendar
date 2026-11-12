//! Calendar API: events CRUD + calendars.

use axum::{extract::State, http::StatusCode, response::IntoResponse, routing::get, Json, Router};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::{DateTime, Utc};

use crate::auth::require_auth;
use crate::state::AppState;
use ycore::model::{Calendar, Event};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/calendars", get(list_calendars).post(create_calendar))
        .route("/api/calendars/{id}", axum::routing::delete(delete_calendar).patch(patch_calendar))
        .route("/api/events", get(list_events).post(create_event))
        .route("/api/events/{id}", get(get_event).put(update_event).delete(delete_event))
}

#[derive(Deserialize)]
struct EventsQuery {
    from: Option<String>,
    to: Option<String>,
}

#[derive(Serialize)]
struct CalendarOut {
    id: String,
    name: String,
    color: String,
    visible: bool,
    sort_order: i32,
}

fn calendar_out(c: &Calendar) -> CalendarOut {
    CalendarOut {
        id: c.id.to_string(),
        name: c.name.clone(),
        color: c.color.clone(),
        visible: c.visible,
        sort_order: c.sort_order,
    }
}

#[derive(Serialize)]
struct EventOut {
    id: String,
    title: String,
    notes: String,
    location: Option<String>,
    start: String,
    end: String,
    all_day: bool,
    color: Option<String>,
    recurrence: Option<serde_json::Value>,
    reminders: Vec<String>,
    calendar_id: Option<String>,
    created_at: String,
    updated_at: String,
}

fn event_out(e: &Event) -> EventOut {
    EventOut {
        id: e.id.to_string(),
        title: e.title.clone(),
        notes: e.notes.clone(),
        location: e.location.clone(),
        start: e.start.to_rfc3339(),
        end: e.end.to_rfc3339(),
        all_day: e.all_day,
        color: e.color.clone(),
        recurrence: e.recurrence.as_ref().map(|r| serde_json::to_value(r).unwrap_or_default()),
        reminders: e.reminders.iter().map(|r| format!("{:?}", r)).collect(),
        calendar_id: e.calendar_id.map(|c| c.to_string()),
        created_at: e.created_at.to_rfc3339(),
        updated_at: e.updated_at.to_rfc3339(),
    }
}

async fn list_calendars(
    State(state): State<AppState>,
    headers: axum::http::header::HeaderMap,
) -> impl IntoResponse {
    if require_auth(&state, &headers).is_err() {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let db = state.db.lock().unwrap();
    match ycore::storage::calendar::list_calendars(&db) {
        Ok(list) => Json(list.iter().map(calendar_out).collect::<Vec<_>>()).into_response(),
        Err(e) => {
            tracing::error!("list_calendars: {e}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

#[derive(Deserialize)]
pub struct CreateCalendarRequest {
    pub name: Option<String>,
    pub color: Option<String>,
}

async fn create_calendar(
    State(state): State<AppState>,
    headers: axum::http::header::HeaderMap,
    Json(body): Json<CreateCalendarRequest>,
) -> impl IntoResponse {
    if require_auth(&state, &headers).is_err() {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let name = body.name.unwrap_or_else(|| "New calendar".to_string());
    let color = body.color.unwrap_or_else(|| "#aecbfa".to_string());
    let cal = Calendar::new(name, color);
    let db = state.db.lock().unwrap();
    match ycore::storage::calendar::create_calendar(&db, &cal) {
        Ok(_) => (StatusCode::CREATED, Json(calendar_out(&cal))).into_response(),
        Err(e) => {
            tracing::error!("create_calendar: {e}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

#[derive(Deserialize)]
pub struct PatchCalendarRequest {
    pub name: Option<String>,
    pub color: Option<String>,
    pub visible: Option<bool>,
}

async fn patch_calendar(
    State(state): State<AppState>,
    headers: axum::http::header::HeaderMap,
    axum::extract::Path(id): axum::extract::Path<String>,
    Json(body): Json<PatchCalendarRequest>,
) -> impl IntoResponse {
    if require_auth(&state, &headers).is_err() {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let Ok(id) = Uuid::parse_str(&id) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let db = state.db.lock().unwrap();
    let mut stmt = db.prepare("SELECT id, name, color, visible, sort_order FROM calendars WHERE id = ?1").unwrap();
    let row = stmt.query_row(rusqlite::params![id.to_string()], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, i64>(3)? != 0, r.get::<_, i32>(4)?))
    });
    let Ok((id_str, name, color, visible, sort_order)) = row else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let new_name = body.name.unwrap_or(name);
    let new_color = body.color.unwrap_or(color);
    let new_visible = body.visible.unwrap_or(visible);
    db.execute(
        "UPDATE calendars SET name=?1, color=?2, visible=?3 WHERE id=?4",
        rusqlite::params![new_name, new_color, new_visible as i64, id_str],
    ).unwrap();
    Json(serde_json::json!({
        "id": id_str,
        "name": new_name,
        "color": new_color,
        "visible": new_visible,
        "sort_order": sort_order,
    }))
    .into_response()
}

async fn delete_calendar(
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
    // Delete events in that calendar first, then the calendar itself.
    let events: Vec<Uuid> = ycore::storage::calendar::list_events(&db)
        .unwrap_or_default()
        .into_iter()
        .filter(|e| e.calendar_id == Some(id))
        .map(|e| e.id)
        .collect();
    for ev_id in events {
        let _ = ycore::storage::calendar::delete_event(&db, ev_id);
    }
    let _ = db.execute("DELETE FROM calendars WHERE id = ?1", rusqlite::params![id.to_string()]);
    StatusCode::NO_CONTENT.into_response()
}

async fn list_events(
    State(state): State<AppState>,
    headers: axum::http::header::HeaderMap,
    axum::extract::Query(q): axum::extract::Query<EventsQuery>,
) -> impl IntoResponse {
    if require_auth(&state, &headers).is_err() {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let from: DateTime<Utc> = q
        .from
        .as_deref()
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|d| d.with_timezone(&Utc))
        .unwrap_or_else(|| Utc::now() - chrono::Duration::days(1));
    let to: DateTime<Utc> = q
        .to
        .as_deref()
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|d| d.with_timezone(&Utc))
        .unwrap_or_else(|| from + chrono::Duration::days(90));
    let db = state.db.lock().unwrap();
    match ycore::storage::calendar::events_in_range(&db, from, to) {
        Ok(list) => Json(list.iter().map(event_out).collect::<Vec<_>>()).into_response(),
        Err(e) => {
            tracing::error!("list_events: {e}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

#[derive(Deserialize)]
pub struct CreateEventRequest {
    pub title: Option<String>,
    pub start: Option<String>,
    pub end: Option<String>,
    pub notes: Option<String>,
    pub location: Option<String>,
    pub all_day: Option<bool>,
    pub color: Option<String>,
    pub calendar_id: Option<String>,
}

async fn create_event(
    State(state): State<AppState>,
    headers: axum::http::header::HeaderMap,
    Json(body): Json<CreateEventRequest>,
) -> impl IntoResponse {
    if require_auth(&state, &headers).is_err() {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let start: DateTime<Utc> = body
        .start
        .as_deref()
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|d| d.with_timezone(&Utc))
        .unwrap_or_else(Utc::now);
    let end: DateTime<Utc> = body
        .end
        .as_deref()
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|d| d.with_timezone(&Utc))
        .unwrap_or_else(|| start + chrono::Duration::hours(1));
    let mut ev = Event::new(
        body.title.unwrap_or_else(|| "Untitled event".to_string()),
        start,
        end,
    );
    ev.notes = body.notes.unwrap_or_default();
    ev.location = body.location;
    ev.all_day = body.all_day.unwrap_or(false);
    ev.color = body.color;
    ev.calendar_id = body.calendar_id.as_deref().and_then(|s| Uuid::parse_str(s).ok());
    let db = state.db.lock().unwrap();
    match ycore::storage::calendar::create_event(&db, &ev) {
        Ok(_) => (StatusCode::CREATED, Json(event_out(&ev))).into_response(),
        Err(e) => {
            tracing::error!("create_event: {e}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

async fn get_event(
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
    match ycore::storage::calendar::get_event(&db, id) {
        Ok(Some(e)) => Json(event_out(&e)).into_response(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(e) => {
            tracing::error!("get_event: {e}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

#[derive(Deserialize)]
pub struct UpdateEventRequest {
    pub title: Option<String>,
    pub start: Option<String>,
    pub end: Option<String>,
    pub notes: Option<String>,
    pub location: Option<String>,
    pub all_day: Option<bool>,
    pub color: Option<String>,
    pub calendar_id: Option<String>,
}

async fn update_event(
    State(state): State<AppState>,
    headers: axum::http::header::HeaderMap,
    axum::extract::Path(id): axum::extract::Path<String>,
    Json(body): Json<UpdateEventRequest>,
) -> impl IntoResponse {
    if require_auth(&state, &headers).is_err() {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let Ok(id) = Uuid::parse_str(&id) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let db = state.db.lock().unwrap();
    let mut ev = match ycore::storage::calendar::get_event(&db, id) {
        Ok(Some(e)) => e,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(e) => {
            tracing::error!("get_event for update: {e}");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };
    if let Some(t) = body.title { ev.title = t; }
    if let Some(s) = body.start {
        if let Ok(d) = DateTime::parse_from_rfc3339(&s) {
            ev.start = d.with_timezone(&Utc);
        }
    }
    if let Some(s) = body.end {
        if let Ok(d) = DateTime::parse_from_rfc3339(&s) {
            ev.end = d.with_timezone(&Utc);
        }
    }
    if let Some(n) = body.notes { ev.notes = n; }
    if let Some(l) = body.location { ev.location = Some(l); }
    if let Some(a) = body.all_day { ev.all_day = a; }
    if let Some(c) = body.color { ev.color = Some(c); }
    if let Some(c) = body.calendar_id {
        ev.calendar_id = Uuid::parse_str(&c).ok();
    }
    match ycore::storage::calendar::update_event(&db, &ev) {
        Ok(_) => {
            let updated = ycore::storage::calendar::get_event(&db, ev.id).unwrap().unwrap();
            Json(event_out(&updated)).into_response()
        }
        Err(e) => {
            tracing::error!("update_event: {e}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

async fn delete_event(
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
    match ycore::storage::calendar::delete_event(&db, id) {
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => {
            tracing::error!("delete_event: {e}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}
