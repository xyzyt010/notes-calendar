//! Notes storage: FTS5-backed full-text search, folders, tags, pinning.

use crate::crypto::{decrypt_bytes, decrypt_text, encrypt_bytes, encrypt_text, VaultKey};
use crate::model::{Folder, Note, NoteImage, NoteImageMeta, NoteSummary, NotesSort};
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use uuid::Uuid;

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS notes (
    id          TEXT PRIMARY KEY,
    title       TEXT NOT NULL DEFAULT '',
    body        TEXT NOT NULL DEFAULT '',
    folder_id   TEXT,
    tags        TEXT NOT NULL DEFAULT '[]',  -- JSON array
    pinned      INTEGER NOT NULL DEFAULT 0,
    favorite    INTEGER NOT NULL DEFAULT 0,
    color       TEXT,
    archived    INTEGER NOT NULL DEFAULT 0,
    trashed     INTEGER NOT NULL DEFAULT 0,
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL,
    encrypted   INTEGER NOT NULL DEFAULT 0,
    sort_order  INTEGER NOT NULL DEFAULT 0,
    FOREIGN KEY (folder_id) REFERENCES folders(id) ON DELETE SET NULL
);
CREATE TABLE IF NOT EXISTS folders (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL,
    parent_id   TEXT,
    icon        TEXT,
    color       TEXT,
    sort_order  INTEGER NOT NULL DEFAULT 0,
    created_at  TEXT NOT NULL,
    FOREIGN KEY (parent_id) REFERENCES folders(id) ON DELETE SET NULL
);
CREATE VIRTUAL TABLE IF NOT EXISTS notes_fts USING fts5(
    title, body,
    content='notes', content_rowid='rowid',
    tokenize='porter unicode61'
);
CREATE TRIGGER IF NOT EXISTS notes_ai AFTER INSERT ON notes BEGIN
    INSERT INTO notes_fts(rowid, title, body) VALUES (new.rowid, new.title, new.body);
END;
CREATE TRIGGER IF NOT EXISTS notes_ad AFTER DELETE ON notes BEGIN
    INSERT INTO notes_fts(notes_fts, rowid, title, body) VALUES('delete', old.rowid, old.title, old.body);
END;
CREATE TRIGGER IF NOT EXISTS notes_au AFTER UPDATE ON notes BEGIN
    INSERT INTO notes_fts(notes_fts, rowid, title, body) VALUES('delete', old.rowid, old.title, old.body);
    INSERT INTO notes_fts(rowid, title, body) VALUES (new.rowid, new.title, new.body);
END;
CREATE TABLE IF NOT EXISTS note_activity (
    id          TEXT PRIMARY KEY,
    note_id     TEXT NOT NULL,
    kind        TEXT NOT NULL,
    created_at  TEXT NOT NULL,
    FOREIGN KEY (note_id) REFERENCES notes(id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_note_activity_note ON note_activity(note_id, created_at DESC);
CREATE TABLE IF NOT EXISTS note_images (
    id          TEXT PRIMARY KEY,
    note_id     TEXT NOT NULL,
    name        TEXT NOT NULL DEFAULT '',
    mime        TEXT NOT NULL DEFAULT 'application/octet-stream',
    size        INTEGER NOT NULL DEFAULT 0,
    bytes       BLOB NOT NULL,
    created_at  TEXT NOT NULL,
    FOREIGN KEY (note_id) REFERENCES notes(id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_note_images_note ON note_images(note_id, created_at);
CREATE INDEX IF NOT EXISTS idx_notes_folder ON notes(folder_id);
CREATE INDEX IF NOT EXISTS idx_notes_updated ON notes(updated_at DESC);
CREATE INDEX IF NOT EXISTS idx_notes_pinned ON notes(pinned, updated_at DESC);
"#;

pub fn migrate(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(SCHEMA)?;
    // Existing databases need the encrypted flag added in place.
    let has_encrypted: i64 = conn.query_row(
        "SELECT COUNT(*) FROM pragma_table_info('notes') WHERE name = 'encrypted'",
        [],
        |r| r.get(0),
    )?;
    if has_encrypted == 0 {
        conn.execute_batch("ALTER TABLE notes ADD COLUMN encrypted INTEGER NOT NULL DEFAULT 0")?;
    }
    let has_sort_order: i64 = conn.query_row(
        "SELECT COUNT(*) FROM pragma_table_info('notes') WHERE name = 'sort_order'",
        [],
        |r| r.get(0),
    )?;
    if has_sort_order == 0 {
        conn.execute_batch("ALTER TABLE notes ADD COLUMN sort_order INTEGER NOT NULL DEFAULT 0")?;
    }
    Ok(())
}

fn uuid_to_str(id: Uuid) -> String {
    id.to_string()
}

fn str_to_uuid(s: &str) -> rusqlite::Result<Uuid> {
    Uuid::parse_str(s)
        .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))
}

fn parse_dt(s: &str) -> rusqlite::Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s)
        .map(|d| d.with_timezone(&Utc))
        .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))
}

fn note_from_row(row: &rusqlite::Row) -> rusqlite::Result<Note> {
    let tags_json: String = row.get("tags")?;
    Ok(Note {
        id: str_to_uuid(&row.get::<_, String>("id")?)?,
        title: row.get("title")?,
        body: row.get("body")?,
        folder_id: row.get::<_, Option<String>>("folder_id")?.map(|s| str_to_uuid(&s)).transpose()?,
        tags: serde_json::from_str(&tags_json).unwrap_or_default(),
        pinned: row.get::<_, i64>("pinned")? != 0,
        favorite: row.get::<_, i64>("favorite")? != 0,
        color: row.get("color")?,
        archived: row.get::<_, i64>("archived")? != 0,
        trashed: row.get::<_, i64>("trashed")? != 0,
        encrypted: row.get::<_, i64>("encrypted")? != 0,
        sort_order: row.get("sort_order")?,
        created_at: parse_dt(&row.get::<_, String>("created_at")?)?,
        updated_at: parse_dt(&row.get::<_, String>("updated_at")?)?,
    })
}

/// Sanitize arbitrary user text into a safe FTS5 query (prefix  AND terms).
fn fts_query(q: &str) -> Option<String> {
    let tokens: Vec<String> = q
        .split_whitespace()
        .map(|t| t.chars().filter(|c| c.is_alphanumeric() || *c == '_').collect::<String>())
        .filter(|t| !t.is_empty())
        .map(|t| format!("\"{}\"*", t.replace('"', "\"\"")))
        .collect();
    if tokens.is_empty() {
        None
    } else {
        Some(tokens.join(" ")) // implicit AND
    }
}

const NOTE_COLS: &str =
    "id, title, body, folder_id, tags, pinned, favorite, color, archived, trashed, encrypted, sort_order, created_at, updated_at";

fn to_sql_err(e: anyhow::Error) -> rusqlite::Error {
    rusqlite::Error::ToSqlConversionFailure(Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string())))
}

fn encode_note(note: &Note, key: Option<&VaultKey>) -> rusqlite::Result<Note> {
    let mut out = note.clone();
    if note.encrypted {
        let key = key.ok_or_else(|| rusqlite::Error::ToSqlConversionFailure(Box::new(std::io::Error::new(std::io::ErrorKind::InvalidInput, "vault key required for encrypted note"))))?;
        out.title = encrypt_text(key, &note.title).map_err(to_sql_err)?;
        out.body = encrypt_text(key, &note.body).map_err(to_sql_err)?;
        out.tags = note.tags.iter().map(|t| encrypt_text(key, t)).collect::<Result<Vec<_>, _>>().map_err(to_sql_err)?;
    }
    Ok(out)
}

fn decode_note(note: Note, key: Option<&VaultKey>) -> rusqlite::Result<Option<Note>> {
    if !note.encrypted {
        return Ok(Some(note));
    }
    let Some(key) = key else { return Ok(None) };
    let mut out = note.clone();
    let mut ctx = None;
    out.title = match decrypt_text(key, &note.title) {
        Ok(v) => v,
        Err(e) => { ctx = Some(e.to_string()); String::new() }
    };
    out.body = match decrypt_text(key, &note.body) {
        Ok(v) => v,
        Err(e) => { ctx = ctx.or(Some(e.to_string())); String::new() }
    };
    let mut tags = Vec::with_capacity(note.tags.len());
    for tag in &note.tags {
        match decrypt_text(key, tag) {
            Ok(v) => tags.push(v),
            Err(e) => { ctx = ctx.or(Some(e.to_string())); }
        }
    }
    out.tags = tags;
    if let Some(err) = ctx {
        return Err(rusqlite::Error::ToSqlConversionFailure(Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, err))));
    }
    Ok(Some(out))
}

pub fn create_note(conn: &Connection, note: &Note) -> rusqlite::Result<()> {
    create_note_with_key(conn, note, None)
}

pub fn create_note_with_key(conn: &Connection, note: &Note, key: Option<&VaultKey>) -> rusqlite::Result<()> {
    let encoded = encode_note(note, key)?;
    conn.execute(
        "INSERT INTO notes (id, title, body, folder_id, tags, pinned, favorite, color, archived, trashed, encrypted, sort_order, created_at, updated_at)
         VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
        params![
            uuid_to_str(encoded.id),
            encoded.title,
            encoded.body,
            encoded.folder_id.map(uuid_to_str),
            serde_json::to_string(&encoded.tags).unwrap_or_else(|_| "[]".into()),
            encoded.pinned as i64,
            encoded.favorite as i64,
            encoded.color,
            encoded.archived as i64,
            encoded.trashed as i64,
            encoded.encrypted as i64,
            encoded.sort_order,
            encoded.created_at.to_rfc3339(),
            encoded.updated_at.to_rfc3339(),
        ],
    )?;
    Ok(())
}

pub fn update_note(conn: &Connection, note: &Note) -> rusqlite::Result<()> {
    update_note_with_key(conn, note, None)
}

pub fn update_note_with_key(conn: &Connection, note: &Note, key: Option<&VaultKey>) -> rusqlite::Result<()> {
    let encoded = encode_note(note, key)?;
    conn.execute(
        "UPDATE notes SET title=?, body=?, folder_id=?, tags=?, pinned=?, favorite=?, color=?, archived=?, trashed=?, encrypted=?, sort_order=?, updated_at=? WHERE id=?",
        params![
            encoded.title,
            encoded.body,
            encoded.folder_id.map(uuid_to_str),
            serde_json::to_string(&encoded.tags).unwrap_or_else(|_| "[]".into()),
            encoded.pinned as i64,
            encoded.favorite as i64,
            encoded.color,
            encoded.archived as i64,
            encoded.trashed as i64,
            encoded.encrypted as i64,
            encoded.sort_order,
            Utc::now().to_rfc3339(),
            uuid_to_str(encoded.id),
        ],
    )?;
    Ok(())
}

pub fn delete_note_hard(conn: &Connection, id: Uuid) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM notes WHERE id=?", params![uuid_to_str(id)])?;
    Ok(())
}

fn encode_image(image: &NoteImage, key: Option<&VaultKey>) -> rusqlite::Result<NoteImage> {
    let mut out = image.clone();
    if let Some(k) = key {
        out.meta.name = encrypt_text(k, &image.meta.name).map_err(to_sql_err)?;
        out.bytes = encrypt_bytes(k, &image.bytes).map_err(to_sql_err)?;
    }
    Ok(out)
}

fn decode_image(image: NoteImage, key: Option<&VaultKey>) -> rusqlite::Result<NoteImage> {
    let Some(k) = key else { return Ok(image) };
    let mut out = image.clone();
    out.meta.name = decrypt_text(k, &image.meta.name).unwrap_or_else(|_| "Encrypted image".into());
    out.bytes = decrypt_bytes(k, &image.bytes).map_err(to_sql_err)?;
    Ok(out)
}

pub fn create_image(conn: &Connection, image: &NoteImage, key: Option<&VaultKey>) -> rusqlite::Result<()> {
    let encoded = encode_image(image, key)?;
    conn.execute(
        "INSERT INTO note_images (id, note_id, name, mime, size, bytes, created_at) VALUES (?,?,?,?,?,?,?)",
        params![
            uuid_to_str(encoded.meta.id),
            uuid_to_str(encoded.meta.note_id),
            encoded.meta.name,
            encoded.meta.mime,
            encoded.meta.size,
            encoded.bytes,
            encoded.meta.created_at.to_rfc3339(),
        ],
    )?;
    Ok(())
}

const IMAGE_META_COLS: &str = "id, note_id, name, mime, size, created_at";

pub fn list_images(conn: &Connection, note_id: Uuid, key: Option<&VaultKey>) -> rusqlite::Result<Vec<NoteImageMeta>> {
    let mut stmt = conn.prepare(&format!("SELECT {IMAGE_META_COLS} FROM note_images WHERE note_id=? ORDER BY created_at ASC"))?;
    let rows = stmt.query_map(params![uuid_to_str(note_id)], image_meta_from_row)?;
    let mut out = Vec::new();
    for row in rows {
        let mut meta = row?;
        if let Some(k) = key {
            meta.name = decrypt_text(k, &meta.name).unwrap_or_else(|_| "Encrypted image".into());
        }
        out.push(meta);
    }
    Ok(out)
}

fn image_meta_from_row(row: &rusqlite::Row) -> rusqlite::Result<NoteImageMeta> {
    Ok(NoteImageMeta {
        id: str_to_uuid(&row.get::<_, String>("id")?)?,
        note_id: str_to_uuid(&row.get::<_, String>("note_id")?)?,
        name: row.get("name")?,
        mime: row.get("mime")?,
        size: row.get("size")?,
        created_at: parse_dt(&row.get::<_, String>("created_at")?)?,
    })
}

pub fn get_image(conn: &Connection, id: Uuid, key: Option<&VaultKey>) -> rusqlite::Result<Option<NoteImage>> {
    let mut stmt = conn.prepare(&format!("SELECT {IMAGE_META_COLS}, bytes FROM note_images WHERE id=?"))?;
    let mut rows = stmt.query(params![uuid_to_str(id)])?;
    let Some(row) = rows.next()? else { return Ok(None) };
    let meta = image_meta_from_row(row)?;
    let bytes: Vec<u8> = row.get("bytes")?;
    let image = NoteImage { meta, bytes };
    decode_image(image, key).map(Some)
}

pub fn update_image(conn: &Connection, image: &NoteImage, key: Option<&VaultKey>) -> rusqlite::Result<()> {
    let encoded = encode_image(image, key)?;
    conn.execute(
        "UPDATE note_images SET note_id=?, name=?, mime=?, size=?, bytes=? WHERE id=?",
        params![
            uuid_to_str(encoded.meta.note_id),
            encoded.meta.name,
            encoded.meta.mime,
            encoded.meta.size,
            encoded.bytes,
            uuid_to_str(encoded.meta.id),
        ],
    )?;
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivityRow {
    pub kind: String,
    pub created_at: DateTime<Utc>,
}

pub fn log_activity(conn: &Connection, note_id: Uuid, kind: &str) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO note_activity (id, note_id, kind, created_at) VALUES (?,?,?,?)",
        params![Uuid::new_v4().to_string(), uuid_to_str(note_id), kind, Utc::now().to_rfc3339()],
    )?;
    Ok(())
}

pub fn list_activity(conn: &Connection, note_id: Uuid, limit: i64) -> rusqlite::Result<Vec<ActivityRow>> {
    let mut stmt = conn.prepare(
        "SELECT kind, created_at FROM note_activity WHERE note_id = ? ORDER BY created_at DESC LIMIT ?",
    )?;
    let rows = stmt.query_map(params![uuid_to_str(note_id), limit], |r| {
        Ok(ActivityRow {
            kind: r.get(0)?,
            created_at: parse_dt(&r.get::<_, String>(1)?)?,
        })
    })?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

pub fn delete_image(conn: &Connection, id: Uuid) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM note_images WHERE id=?", params![uuid_to_str(id)])?;
    Ok(())
}

pub fn get_note(conn: &Connection, id: Uuid) -> rusqlite::Result<Option<Note>> {
    get_note_with_key(conn, id, None)
}

pub fn get_note_with_key(conn: &Connection, id: Uuid, key: Option<&VaultKey>) -> rusqlite::Result<Option<Note>> {
    let note = conn.query_row(
        &format!("SELECT {NOTE_COLS} FROM notes WHERE id=?"),
        params![uuid_to_str(id)],
        note_from_row,
    )
    .optional()?;
    note.map(|n| decode_note(n, key)).transpose().map(|v| v.flatten())
}

pub struct ListFilter<'a> {
    pub folder_id: Option<Uuid>,
    pub tag: Option<&'a str>,
    pub archived: bool,
    pub trashed: bool,
    pub favorites_only: bool,
    pub query: Option<&'a str>,
    pub sort: NotesSort,
    pub limit: i64,
}

impl Default for ListFilter<'_> {
    fn default() -> Self {
        Self {
            folder_id: None,
            tag: None,
            archived: false,
            trashed: false,
            favorites_only: false,
            query: None,
            sort: NotesSort::Updated,
            limit: 500,
        }
    }
}

/// List note summaries; pinned notes first, then by updated_at desc.
pub fn list_notes(conn: &Connection, f: &ListFilter) -> rusqlite::Result<Vec<NoteSummary>> {
    list_notes_with_key(conn, f, None)
}

/// List note summaries with an optional vault key. Encrypted notes are skipped
/// when no key is supplied.
pub fn list_notes_with_key(conn: &Connection, f: &ListFilter, key: Option<&VaultKey>) -> rusqlite::Result<Vec<NoteSummary>> {
    let mut sql = format!("SELECT {NOTE_COLS} FROM notes WHERE trashed={} AND archived={}",
        f.trashed as i64, f.archived as i64);
    let mut p: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
    if let Some(fid) = f.folder_id {
        sql.push_str(" AND folder_id=?");
        p.push(Box::new(uuid_to_str(fid)));
    }
    let decrypt_filter = key.is_some();
    if let Some(tag) = f.tag {
        if !decrypt_filter {
            sql.push_str(" AND EXISTS (SELECT 1 FROM json_each(notes.tags) je WHERE je.value=?)");
            p.push(Box::new(tag.to_string()));
        }
    }
    if f.favorites_only {
        sql.push_str(" AND favorite=1");
    }
    if let Some(q) = f.query {
        if !decrypt_filter {
            if let Some(fq) = fts_query(q) {
                sql.push_str(" AND rowid IN (SELECT rowid FROM notes_fts WHERE notes_fts MATCH ?)");
                p.push(Box::new(fq));
            } else {
                sql.push_str(" AND (title LIKE '%' || ? || '%' OR body LIKE '%' || ? || '%')");
                p.push(Box::new(q.to_string()));
                p.push(Box::new(q.to_string()));
            }
        }
    }
    match f.sort {
        NotesSort::Updated => sql.push_str(" ORDER BY pinned DESC, updated_at DESC"),
        NotesSort::Created => sql.push_str(" ORDER BY pinned DESC, created_at DESC"),
        NotesSort::Title => sql.push_str(" ORDER BY pinned DESC, title COLLATE NOCASE ASC"), // encrypted titles are disambiguated by updated desc below
        NotesSort::Manual => sql.push_str(" ORDER BY pinned DESC, sort_order ASC, updated_at DESC"),
    }
    sql.push_str(" LIMIT ?");
    p.push(Box::new(f.limit));
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(rusqlite::params_from_iter(p.iter().map(|b| b.as_ref())), note_from_row)?;
    let mut out = Vec::new();
    for row in rows {
        let note = match decode_note(row?, key) {
            Ok(Some(n)) => n,
            Ok(None) => continue,
            Err(_) => continue, // wrong key or corrupt note: not visible in this session
        };
        if decrypt_filter {
            let hay = format!("{}\n{}\n{}", note.title, note.body, note.tags.join("\n")).to_lowercase();
            if let Some(tag) = f.tag {
                if !note.tags.iter().any(|t| t.eq_ignore_ascii_case(tag)) { continue; }
            }
            if let Some(q) = f.query {
                let terms: Vec<String> = q.split_whitespace().map(|t| t.to_lowercase()).filter(|t| !t.is_empty()).collect();
                if !terms.is_empty() && !terms.iter().all(|t| hay.contains(t)) { continue; }
            }
        }
        let image_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM note_images WHERE note_id = ?",
            params![uuid_to_str(note.id)],
            |r| r.get(0),
        ).unwrap_or(0);
        let mut summary = NoteSummary::from(&note);
        summary.image_count = image_count;
        out.push(summary);
    }
    if key.is_some() {
        match f.sort {
            NotesSort::Updated => out.sort_by(|a, b| b.pinned.cmp(&a.pinned).then_with(|| b.updated_at.cmp(&a.updated_at))),
            NotesSort::Created => out.sort_by(|a, b| b.pinned.cmp(&a.pinned).then_with(|| b.created_at.cmp(&a.created_at))),
            NotesSort::Title => out.sort_by(|a, b| b.pinned.cmp(&a.pinned).then_with(|| a.title.to_lowercase().cmp(&b.title.to_lowercase()))),
            NotesSort::Manual => out.sort_by(|a, b| b.pinned.cmp(&a.pinned).then_with(|| a.sort_order.cmp(&b.sort_order)).then_with(|| b.updated_at.cmp(&a.updated_at))),
        }
    }
    Ok(out)
}

pub fn all_tags(conn: &Connection) -> rusqlite::Result<Vec<(String, i64)>> {
    all_tags_with_key(conn, None)
}

pub fn all_tags_with_key(conn: &Connection, key: Option<&VaultKey>) -> rusqlite::Result<Vec<(String, i64)>> {
    let mut stmt = conn.prepare(
        "SELECT je.value AS tag, COUNT(*) AS n FROM notes, json_each(notes.tags) je
         WHERE notes.trashed = 0 GROUP BY je.value ORDER BY n DESC, tag ASC",
    )?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?;
    let mut counts = std::collections::HashMap::<String, i64>::new();
    for row in rows {
        let Ok((raw, count)) = row else { continue };
        let tag = if let Some(k) = key {
            decrypt_text(k, &raw).unwrap_or(raw)
        } else {
            raw
        };
        *counts.entry(tag).or_default() += count;
    }
    let mut tags: Vec<(String, i64)> = counts.into_iter().collect();
    tags.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    Ok(tags)
}

// ---------- Folders ----------

pub fn create_folder(conn: &Connection, folder: &Folder) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO folders (id, name, parent_id, icon, color, sort_order, created_at) VALUES (?,?,?,?,?,?,?)",
        params![
            uuid_to_str(folder.id),
            folder.name,
            folder.parent_id.map(uuid_to_str),
            folder.icon,
            folder.color,
            folder.sort_order,
            folder.created_at.to_rfc3339(),
        ],
    )?;
    Ok(())
}

pub fn list_folders(conn: &Connection) -> rusqlite::Result<Vec<Folder>> {
    let mut stmt = conn.prepare(
        "SELECT id, name, parent_id, icon, color, sort_order, created_at FROM folders ORDER BY sort_order ASC, name ASC",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(Folder {
            id: str_to_uuid(&row.get::<_, String>("id")?)?,
            name: row.get("name")?,
            parent_id: row.get::<_, Option<String>>("parent_id")?.map(|s| str_to_uuid(&s)).transpose()?,
            icon: row.get("icon")?,
            color: row.get("color")?,
            sort_order: row.get("sort_order")?,
            created_at: parse_dt(&row.get::<_, String>("created_at")?)?,
        })
    })?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

pub fn set_note_order(conn: &Connection, ordered_ids: &[Uuid]) -> rusqlite::Result<()> {
    let tx = conn.unchecked_transaction()?;
    for (idx, id) in ordered_ids.iter().enumerate() {
        tx.execute("UPDATE notes SET sort_order=? WHERE id=?", params![idx as i64, uuid_to_str(*id)])?;
    }
    tx.commit()
}

pub fn rename_folder(conn: &Connection, id: Uuid, name: &str) -> rusqlite::Result<()> {
    conn.execute("UPDATE folders SET name=? WHERE id=?", params![name, uuid_to_str(id)])?;
    Ok(())
}

pub fn delete_folder(conn: &Connection, id: Uuid) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM folders WHERE id=?", params![uuid_to_str(id)])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Note;

    #[test]
    fn crud_and_search() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let mut n = Note::new("Hello world", "This is a banana note about Rust and SQLite.");
        create_note(&conn, &n).unwrap();

        let got = get_note(&conn, n.id).unwrap().unwrap();
        assert_eq!(got.title, "Hello world");

        let listed = list_notes(&conn, &ListFilter::default()).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].title, "Hello world");

        let hit = list_notes(&conn, &ListFilter { query: Some("banana"), ..Default::default() }).unwrap();
        assert_eq!(hit.len(), 1);

        n.body = "totally different body".into();
        update_note(&conn, &n).unwrap();
        let miss = list_notes(&conn, &ListFilter { query: Some("banana"), ..Default::default() }).unwrap();
        assert_eq!(miss.len(), 0);

        delete_note_hard(&conn, n.id).unwrap();
        assert!(get_note(&conn, n.id).unwrap().is_none());
    }

    #[test]
    fn pinned_notes_sort_first() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let a = Note::new("alpha", "first");
        let mut b = Note::new("beta", "second");
        create_note(&conn, &a).unwrap();
        create_note(&conn, &b).unwrap();
        b.pinned = true;
        update_note(&conn, &b).unwrap();
        let listed = list_notes(&conn, &ListFilter::default()).unwrap();
        assert_eq!(listed[0].title, "beta");
        assert_eq!(listed[1].title, "alpha");
    }

    #[test]
    fn trashed_and_archived_are_split_from_default_list() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let mut keep = Note::new("keep", "visible");
        let mut arch = Note::new("arch", "archived");
        let mut trash = Note::new("trash", "trashed");
        create_note(&conn, &keep).unwrap();
        create_note(&conn, &arch).unwrap();
        create_note(&conn, &trash).unwrap();
        arch.archived = true;
        trash.trashed = true;
        update_note(&conn, &arch).unwrap();
        update_note(&conn, &trash).unwrap();

        let default_list = list_notes(&conn, &ListFilter::default()).unwrap();
        assert_eq!(default_list.iter().map(|n| n.title.as_str()).collect::<Vec<_>>(), vec!["keep"]);

        let archived = list_notes(&conn, &ListFilter { archived: true, ..Default::default() }).unwrap();
        assert_eq!(archived.iter().map(|n| n.title.as_str()).collect::<Vec<_>>(), vec!["arch"]);

        let trashed = list_notes(&conn, &ListFilter { trashed: true, ..Default::default() }).unwrap();
        assert_eq!(trashed.iter().map(|n| n.title.as_str()).collect::<Vec<_>>(), vec!["trash"]);

        // keep var to avoid warnings; silence explicitly
        let _ = &mut keep;
    }

    #[test]
    fn tag_filter_and_all_tags_counts() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let mut n1 = Note::new("one", "body");
        n1.tags = vec!["rust".into(), "sqlite".into()];
        let mut n2 = Note::new("two", "body");
        n2.tags = vec!["rust".into()];
        create_note(&conn, &n1).unwrap();
        create_note(&conn, &n2).unwrap();

        let rust = list_notes(&conn, &ListFilter { tag: Some("rust"), ..Default::default() }).unwrap();
        assert_eq!(rust.len(), 2);
        let sqlite = list_notes(&conn, &ListFilter { tag: Some("sqlite"), ..Default::default() }).unwrap();
        assert_eq!(sqlite.len(), 1);
        assert_eq!(sqlite[0].title, "one");

        let tags = all_tags(&conn).unwrap();
        assert_eq!(tags.iter().find(|(t, _)| t == "rust").unwrap().1, 2);
        assert_eq!(tags.iter().find(|(t, _)| t == "sqlite").unwrap().1, 1);
    }

    #[test]
    fn fts_query_sanitizes_injection_like_input() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        // These must not error even though they contain FTS5 special syntax
        let payloads = [
            "c++ rust",
            "hello\" OR 1=1 --",
            "drop table notes",
            "( ) : * ^",
            "   ",
            "rust_",
        ];
        for p in payloads {
            let _ = list_notes(&conn, &ListFilter { query: Some(p), ..Default::default() }).unwrap();
        }
    }

    #[test]
    fn favorites_only_filter() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let mut fav = Note::new("fav", "starred");
        fav.favorite = true;
        let plain = Note::new("plain", "ordinary");
        create_note(&conn, &fav).unwrap();
        create_note(&conn, &plain).unwrap();
        let favs = list_notes(&conn, &ListFilter { favorites_only: true, ..Default::default() }).unwrap();
        assert_eq!(favs.len(), 1);
        assert_eq!(favs[0].title, "fav");
    }

    #[test]
    fn encrypted_notes_roundtrip_and_require_key() {
        use crate::crypto::new_vault;
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let (key, cfg) = new_vault("correct horse battery staple");
        let unlocked = crate::crypto::unlock("correct horse battery staple", &cfg).unwrap();
        assert_eq!(key.bytes(), unlocked.bytes());

        let mut note = Note::new("Secret dinner", "Book the table at eight");
        note.tags = vec!["private".into()];
        note.encrypted = true;
        create_note_with_key(&conn, &note, Some(&key)).unwrap();

        // Raw row is ciphertext, not plaintext.
        let raw_title: String = conn.query_row("SELECT title FROM notes", [], |r| r.get(0)).unwrap();
        assert!(raw_title.starts_with("v1."));
        assert!(!raw_title.contains("Secret"));

        let hidden = list_notes(&conn, &ListFilter::default()).unwrap();
        assert!(hidden.is_empty());
        let visible = list_notes_with_key(&conn, &ListFilter::default(), Some(&key)).unwrap();
        assert_eq!(visible.len(), 1);
        assert_eq!(visible[0].title, "Secret dinner");
        assert_eq!(visible[0].preview, "Book the table at eight");

        let decrypted = get_note_with_key(&conn, note.id, Some(&key)).unwrap().unwrap();
        assert_eq!(decrypted.body, "Book the table at eight");
        assert_eq!(decrypted.tags, vec!["private"]);

        assert!(crate::crypto::unlock("wrong password", &cfg).is_err());
    }

    #[test]
    fn encrypted_note_images_roundtrip() {
        use crate::crypto::new_vault;
        use crate::model::{NoteImage, NoteImageMeta};
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let (key, _cfg) = new_vault("image-password");
        let mut note = Note::new("encrypted note", "body");
        note.encrypted = true;
        create_note_with_key(&conn, &note, Some(&key)).unwrap();
        let image = NoteImage {
            meta: NoteImageMeta {
                id: Uuid::new_v4(),
                note_id: note.id,
                name: "sketch.png".into(),
                mime: "image/png".into(),
                size: 4,
                created_at: Utc::now(),
            },
            bytes: vec![1, 2, 3, 4],
        };
        create_image(&conn, &image, Some(&key)).unwrap();
        let listed = list_images(&conn, note.id, Some(&key)).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].name, "sketch.png");
        let full = get_image(&conn, image.meta.id, Some(&key)).unwrap().unwrap();
        assert_eq!(full.bytes, vec![1, 2, 3, 4]);
        let raw_name: String = conn.query_row("SELECT name FROM note_images", [], |r| r.get(0)).unwrap();
        assert!(raw_name.starts_with("v1."));
    }

    #[test]
    fn folders_crud_and_filter_by_folder() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let folder = Folder::new("Work");
        create_folder(&conn, &folder).unwrap();
        let mut in_folder = Note::new("work note", "body");
        in_folder.folder_id = Some(folder.id);
        let outside = Note::new("outside", "body");
        create_note(&conn, &in_folder).unwrap();
        create_note(&conn, &outside).unwrap();

        let folders = list_folders(&conn).unwrap();
        assert_eq!(folders.len(), 1);
        assert_eq!(folders[0].name, "Work");

        let listed = list_notes(&conn, &ListFilter { folder_id: Some(folder.id), ..Default::default() }).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].title, "work note");

        rename_folder(&conn, folder.id, "Work 2").unwrap();
        assert_eq!(list_folders(&conn).unwrap()[0].name, "Work 2");
        delete_folder(&conn, folder.id).unwrap();
        assert!(list_folders(&conn).unwrap().is_empty());
        // Deleting a folder unsets folder_id on notes instead of deleting them
        let n = get_note(&conn, in_folder.id).unwrap().unwrap();
        assert_eq!(n.folder_id, None);
    }
}
