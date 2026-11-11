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
    notes::migrate(&conn)?;
    Ok(conn)
}

pub fn open_calendar_db(path: &Path) -> rusqlite::Result<Connection> {
    let conn = open_db(path)?;
    calendar::migrate(&conn)?;
    Ok(conn)
}
