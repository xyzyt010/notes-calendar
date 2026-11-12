//! Notes storage: FTS5-backed full-text search, folders, tags, pinning.

use crate::model::{Folder, Note, NoteSummary};
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
CREATE INDEX IF NOT EXISTS idx_notes_folder ON notes(folder_id);
CREATE INDEX IF NOT EXISTS idx_notes_updated ON notes(updated_at DESC);
CREATE INDEX IF NOT EXISTS idx_notes_pinned ON notes(pinned, updated_at DESC);
"#;

pub fn migrate(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(SCHEMA)
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
    "id, title, body, folder_id, tags, pinned, favorite, color, archived, trashed, created_at, updated_at";

pub fn create_note(conn: &Connection, note: &Note) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO notes (id, title, body, folder_id, tags, pinned, favorite, color, archived, trashed, created_at, updated_at)
         VALUES (?,?,?,?,?,?,?,?,?,?,?,?)",
        params![
            uuid_to_str(note.id),
            note.title,
            note.body,
            note.folder_id.map(uuid_to_str),
            serde_json::to_string(&note.tags).unwrap_or_else(|_| "[]".into()),
            note.pinned as i64,
            note.favorite as i64,
            note.color,
            note.archived as i64,
            note.trashed as i64,
            note.created_at.to_rfc3339(),
            note.updated_at.to_rfc3339(),
        ],
    )?;
    Ok(())
}

pub fn update_note(conn: &Connection, note: &Note) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE notes SET title=?, body=?, folder_id=?, tags=?, pinned=?, favorite=?, color=?, archived=?, trashed=?, updated_at=? WHERE id=?",
        params![
            note.title,
            note.body,
            note.folder_id.map(uuid_to_str),
            serde_json::to_string(&note.tags).unwrap_or_else(|_| "[]".into()),
            note.pinned as i64,
            note.favorite as i64,
            note.color,
            note.archived as i64,
            note.trashed as i64,
            Utc::now().to_rfc3339(),
            uuid_to_str(note.id),
        ],
    )?;
    Ok(())
}

pub fn delete_note_hard(conn: &Connection, id: Uuid) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM notes WHERE id=?", params![uuid_to_str(id)])?;
    Ok(())
}

pub fn get_note(conn: &Connection, id: Uuid) -> rusqlite::Result<Option<Note>> {
    conn.query_row(
        &format!("SELECT {NOTE_COLS} FROM notes WHERE id=?"),
        params![uuid_to_str(id)],
        note_from_row,
    )
    .optional()
}

pub struct ListFilter<'a> {
    pub folder_id: Option<Uuid>,
    pub tag: Option<&'a str>,
    pub archived: bool,
    pub trashed: bool,
    pub favorites_only: bool,
    pub query: Option<&'a str>,
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
            limit: 500,
        }
    }
}

/// List note summaries; pinned notes first, then by updated_at desc.
pub fn list_notes(conn: &Connection, f: &ListFilter) -> rusqlite::Result<Vec<NoteSummary>> {
    let mut sql = format!("SELECT {NOTE_COLS} FROM notes WHERE trashed={} AND archived={}",
        f.trashed as i64, f.archived as i64);
    let mut p: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
    if let Some(fid) = f.folder_id {
        sql.push_str(" AND folder_id=?");
        p.push(Box::new(uuid_to_str(fid)));
    }
    if let Some(tag) = f.tag {
        sql.push_str(" AND EXISTS (SELECT 1 FROM json_each(notes.tags) je WHERE je.value=?)");
        p.push(Box::new(tag.to_string()));
    }
    if f.favorites_only {
        sql.push_str(" AND favorite=1");
    }
    if let Some(q) = f.query {
        if let Some(fq) = fts_query(q) {
            sql.push_str(" AND rowid IN (SELECT rowid FROM notes_fts WHERE notes_fts MATCH ?)");
            p.push(Box::new(fq));
        } else {
            sql.push_str(" AND (title LIKE '%' || ? || '%' OR body LIKE '%' || ? || '%')");
            p.push(Box::new(q.to_string()));
            p.push(Box::new(q.to_string()));
        }
    }
    sql.push_str(" ORDER BY pinned DESC, updated_at DESC LIMIT ?");
    p.push(Box::new(f.limit));
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(rusqlite::params_from_iter(p.iter().map(|b| b.as_ref())), note_from_row)?;
    Ok(rows.filter_map(|r| r.ok()).map(|n| NoteSummary::from(&n)).collect())
}

pub fn all_tags(conn: &Connection) -> rusqlite::Result<Vec<(String, i64)>> {
    let mut stmt = conn.prepare(
        "SELECT je.value AS tag, COUNT(*) AS n FROM notes, json_each(notes.tags) je
         WHERE notes.trashed = 0 GROUP BY je.value ORDER BY n DESC, tag ASC",
    )?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?;
    Ok(rows.filter_map(|r| r.ok()).collect())
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
