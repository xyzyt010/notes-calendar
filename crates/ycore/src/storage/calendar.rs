//! Calendar storage: events, calendars, recurrence, reminders.

use crate::model::{Calendar, Event, Recurrence, ReminderKind};
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use uuid::Uuid;

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS calendars (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL,
    color       TEXT NOT NULL DEFAULT '#4F8DFD',
    visible     INTEGER NOT NULL DEFAULT 1,
    sort_order  INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE IF NOT EXISTS events (
    id           TEXT PRIMARY KEY,
    title        TEXT NOT NULL,
    notes        TEXT NOT NULL DEFAULT '',
    location     TEXT,
    start        TEXT NOT NULL,
    end          TEXT NOT NULL,
    all_day      INTEGER NOT NULL DEFAULT 0,
    color        TEXT,
    recurrence   TEXT,          -- JSON Recurrence
    reminders    TEXT NOT NULL DEFAULT '[]',  -- JSON array of ReminderKind
    calendar_id  TEXT,
    created_at   TEXT NOT NULL,
    updated_at   TEXT NOT NULL,
    FOREIGN KEY (calendar_id) REFERENCES calendars(id) ON DELETE SET NULL
);
CREATE INDEX IF NOT EXISTS idx_events_range ON events(start, end);
CREATE INDEX IF NOT EXISTS idx_events_calendar ON events(calendar_id);
"#;

pub fn migrate(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(SCHEMA)
}

fn s(id: Uuid) -> String {
    id.to_string()
}

fn parse_uuid(s: &str) -> rusqlite::Result<Uuid> {
    Uuid::parse_str(s).map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))
}

fn parse_dt(s: &str) -> rusqlite::Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s)
        .map(|d| d.with_timezone(&Utc))
        .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))
}

pub fn create_calendar(conn: &Connection, c: &Calendar) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO calendars (id, name, color, visible, sort_order) VALUES (?,?,?,?,?)",
        params![s(c.id), c.name, c.color, c.visible as i64, c.sort_order],
    )?;
    Ok(())
}

pub fn list_calendars(conn: &Connection) -> rusqlite::Result<Vec<Calendar>> {
    let mut stmt = conn.prepare(
        "SELECT id, name, color, visible, sort_order FROM calendars ORDER BY sort_order ASC, name ASC",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(Calendar {
            id: parse_uuid(&row.get::<_, String>(0)?).unwrap_or_else(|_| Uuid::nil()),
            name: row.get(1)?,
            color: row.get(2)?,
            visible: row.get::<_, i64>(3)? != 0,
            sort_order: row.get(4)?,
        })
    })?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

pub fn create_event(conn: &Connection, e: &Event) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO events (id, title, notes, location, start, end, all_day, color, recurrence, reminders, calendar_id, created_at, updated_at)
         VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?)",
        params![
            s(e.id),
            e.title,
            e.notes,
            e.location,
            e.start.to_rfc3339(),
            e.end.to_rfc3339(),
            e.all_day as i64,
            e.color,
            e.recurrence.as_ref().map(|r| serde_json::to_string(r).unwrap_or_default()),
            serde_json::to_string(&e.reminders).unwrap_or_else(|_| "[]".into()),
            e.calendar_id.map(s),
            e.created_at.to_rfc3339(),
            e.updated_at.to_rfc3339(),
        ],
    )?;
    Ok(())
}

pub fn update_event(conn: &Connection, e: &Event) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE events SET title=?, notes=?, location=?, start=?, end=?, all_day=?, color=?, recurrence=?, reminders=?, calendar_id=?, updated_at=? WHERE id=?",
        params![
            e.title,
            e.notes,
            e.location,
            e.start.to_rfc3339(),
            e.end.to_rfc3339(),
            e.all_day as i64,
            e.color,
            e.recurrence.as_ref().map(|r| serde_json::to_string(r).unwrap_or_default()),
            serde_json::to_string(&e.reminders).unwrap_or_else(|_| "[]".into()),
            e.calendar_id.map(s),
            Utc::now().to_rfc3339(),
            s(e.id),
        ],
    )?;
    Ok(())
}

pub fn delete_event(conn: &Connection, id: Uuid) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM events WHERE id=?", params![s(id)])?;
    Ok(())
}

fn event_from_row(row: &rusqlite::Row) -> rusqlite::Result<Event> {
    let reminders_json: String = row.get("reminders")?;
    let recurrence_json: Option<String> = row.get("recurrence")?;
    Ok(Event {
        id: parse_uuid(&row.get::<_, String>("id")?)?,
        title: row.get("title")?,
        notes: row.get("notes")?,
        location: row.get("location")?,
        start: parse_dt(&row.get::<_, String>("start")?)?,
        end: parse_dt(&row.get::<_, String>("end")?)?,
        all_day: row.get::<_, i64>("all_day")? != 0,
        color: row.get("color")?,
        recurrence: recurrence_json.and_then(|j| serde_json::from_str::<Recurrence>(&j).ok()),
        reminders: serde_json::from_str::<Vec<ReminderKind>>(&reminders_json).unwrap_or_default(),
        calendar_id: row.get::<_, Option<String>>("calendar_id")?.map(|v| parse_uuid(&v)).transpose()?,
        created_at: parse_dt(&row.get::<_, String>("created_at")?)?,
        updated_at: parse_dt(&row.get::<_, String>("updated_at")?)?,
    })
}

const EVENT_COLS: &str =
    "id, title, notes, location, start, end, all_day, color, recurrence, reminders, calendar_id, created_at, updated_at";

/// All events overlapping [from, to).
pub fn events_in_range(conn: &Connection, from: DateTime<Utc>, to: DateTime<Utc>) -> rusqlite::Result<Vec<Event>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {EVENT_COLS} FROM events WHERE start < ?2 AND end > ?1 ORDER BY all_day DESC, start ASC"
    ))?;
    let rows = stmt.query_map(params![from.to_rfc3339(), to.to_rfc3339()], event_from_row)?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

pub fn get_event(conn: &Connection, id: Uuid) -> rusqlite::Result<Option<Event>> {
    conn.query_row(
        &format!("SELECT {EVENT_COLS} FROM events WHERE id=?"),
        params![s(id)],
        event_from_row,
    )
    .optional()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_range_roundtrip() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let cal = Calendar::new("Personal", "#4F8DFD");
        create_calendar(&conn, &cal).unwrap();

        let start = Utc::now();
        let end = start + chrono::Duration::hours(1);
        let mut ev = crate::model::Event::new("Standup", start, end);
        ev.calendar_id = Some(cal.id);
        ev.reminders = vec![ReminderKind::FifteenMinutes];
        create_event(&conn, &ev).unwrap();

        let range = events_in_range(&conn, start - chrono::Duration::minutes(1), end + chrono::Duration::minutes(1)).unwrap();
        assert_eq!(range.len(), 1);
        assert_eq!(range[0].title, "Standup");
        assert_eq!(range[0].reminders, vec![ReminderKind::FifteenMinutes]);

        delete_event(&conn, ev.id).unwrap();
        assert!(get_event(&conn, ev.id).unwrap().is_none());
    }
}
