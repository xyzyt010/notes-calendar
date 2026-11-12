//! SQLite storage layer. Two databases: notes.db and calendar.db.
//! Single-writer; WAL mode for smooth concurrent UI + search.

pub mod calendar;
pub mod notes;

use rusqlite::Connection;
use std::path::Path;

pub fn open_db(path: &Path) -> rusqlite::Result<Connection> {
    let conn = Connection::open(path)?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.pragma_update(None, "busy_timeout", 5000_i64)?;
    Ok(conn)
}

pub fn open_memory() -> rusqlite::Result<Connection> {
    let conn = Connection::open_in_memory()?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    Ok(conn)
}

pub fn open_notes_db(path: &Path) -> rusqlite::Result<Connection> {
    let conn = open_db(path)?;
    create_notes_schema(&conn)?;
    Ok(conn)
}

pub fn open_calendar_db(path: &Path) -> rusqlite::Result<Connection> {
    let conn = open_db(path)?;
    create_calendar_schema(&conn)?;
    Ok(conn)
}

/// Apply the notes schema to an existing connection (for the web suite, which
/// keeps notes and calendar in one SQLite file).
pub fn create_notes_schema(conn: &Connection) -> rusqlite::Result<()> {
    notes::migrate(conn)
}

/// Apply the calendar schema to an existing connection.
pub fn create_calendar_schema(conn: &Connection) -> rusqlite::Result<()> {
    calendar::migrate(conn)
}
