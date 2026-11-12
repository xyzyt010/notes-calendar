//! Shared app state: DB connection, static dir, session store, bind address.

use rusqlite::Connection;
use std::collections::HashSet;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct AppState {
    pub bind_addr: SocketAddr,
    pub static_dir: PathBuf,
    pub admin_password_hash: String,
    pub db: Arc<Mutex<Connection>>,
    pub sessions: Arc<Mutex<HashSet<String>>>,
}

impl AppState {
    pub fn from_env() -> anyhow::Result<Self> {
        let db_path = std::env::var("SUITE_DB")
            .ok()
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("suite.db"));
        let static_dir = std::env::var("SUITE_STATIC_DIR")
            .ok()
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("static"));
        let bind_addr: SocketAddr = std::env::var("SUITE_BIND")
            .ok()
            .unwrap_or_else(|| "127.0.0.1:8760".to_string())
            .parse()?;
        let admin_password_hash = std::env::var("SUITE_ADMIN_PASSWORD_HASH")
            .unwrap_or_else(|_| bcrypt::hash("admin", 12).expect("bcrypt hash"));

        if !static_dir.exists() {
            std::fs::create_dir_all(&static_dir)?;
        }
        let conn = rusqlite::Connection::open(&db_path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        ycore::storage::create_notes_schema(&conn)?;
        ycore::storage::create_calendar_schema(&conn)?;

        Ok(Self {
            bind_addr,
            static_dir,
            admin_password_hash,
            db: Arc::new(Mutex::new(conn)),
            sessions: Arc::new(Mutex::new(HashSet::new())),
        })
    }
}
