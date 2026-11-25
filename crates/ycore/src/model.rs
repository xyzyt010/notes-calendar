//! Unified data model shared by the Notes and Calendar apps.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ---------- Notes ----------

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NotesSort {
    Updated,
    Created,
    Title,
    Manual,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Note {
    pub id: Uuid,
    pub title: String,
    /// Raw markdown body.
    pub body: String,
    pub folder_id: Option<Uuid>,
    pub tags: Vec<String>,
    pub pinned: bool,
    pub favorite: bool,
    /// Hex color, e.g. "#F4E9C8". None = app default.
    pub color: Option<String>,
    pub archived: bool,
    pub trashed: bool,
    /// User-controlled order for Manual sorting.
    pub sort_order: i64,
    /// True when title/body/tags are stored encrypted at rest.
    pub encrypted: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Note {
    pub fn new(title: impl Into<String>, body: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            title: title.into(),
            body: body.into(),
            folder_id: None,
            tags: Vec::new(),
            pinned: false,
            favorite: false,
            color: None,
            archived: false,
            trashed: false,
            sort_order: 0,
            encrypted: false,
            created_at: now,
            updated_at: now,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Folder {
    pub id: Uuid,
    pub name: String,
    pub parent_id: Option<Uuid>,
    pub icon: Option<String>,
    /// Hex color for the folder glyph.
    pub color: Option<String>,
    pub sort_order: i32,
    pub created_at: DateTime<Utc>,
}

impl Folder {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            parent_id: None,
            icon: None,
            color: None,
            sort_order: 0,
            created_at: Utc::now(),
        }
    }
}

/// A lightweight summary used for list views.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NoteSummary {
    pub id: Uuid,
    pub title: String,
    /// First ~140 visible chars of the body, markdown stripped.
    pub preview: String,
    pub folder_id: Option<Uuid>,
    pub tags: Vec<String>,
    pub pinned: bool,
    pub favorite: bool,
    pub color: Option<String>,
    pub updated_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub sort_order: i64,
    pub image_count: i64,
}

impl From<&Note> for NoteSummary {
    fn from(n: &Note) -> Self {
        // Web parity: card bodies are raw text with newlines preserved
        // (app.js renders searchableBody un-stripped); the card line-clamp
        // (prefs.clamp) handles visual clipping. Cap only absurd bodies.
        let preview = truncate_preview(&n.body, 4096);
        Self {
            id: n.id,
            title: n.title.clone(),
            preview,
            folder_id: n.folder_id,
            tags: n.tags.clone(),
            pinned: n.pinned,
            favorite: n.favorite,
            color: n.color.clone(),
            updated_at: n.updated_at,
            created_at: n.created_at,
            sort_order: n.sort_order,
            image_count: 0,
        }
    }
}

/// Cut a string to at most `max_chars` characters, backing off to the last
/// word boundary so the result never ends mid-word; appends an ellipsis.
fn truncate_preview(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        return s.to_string();
    }
    let cut = s.char_indices().nth(max_chars).map(|(i, _)| i).unwrap_or(s.len());
    let head = &s[..cut];
    match head.rfind(char::is_whitespace) {
        Some(i) if i > 0 => format!("{}…", head[..i].trim_end()),
        _ => format!("{}…", head.trim_end()),
    }
}

/// Cheap markdown → plain text for previews. Strips inline formatting
/// (** __ * _ ` ~~), links/images/wikilinks collapse to their visible text,
/// checklist markers drop, HTML tags drop. Keeps the result single-line for
/// the card line-clamp.
pub fn strip_markdown(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for line in input.lines() {
        let t = line
            .trim_start_matches(['#', '>', '-', '*', '+', ' '])
            .trim_start_matches(|c: char| c.is_ascii_digit() || c == '.' || c == ')');
        // drop checklist markers: "- [ ] buy milk" -> "buy milk"
        let t = t
            .strip_prefix("[ ] ").or_else(|| t.strip_prefix("[x] ")).or_else(|| t.strip_prefix("[X] "))
            .unwrap_or(t);
        // ![[target]] / [[target]] / [[target|display]] -> display or target
        let mut t = replace_wikilinks(t);
        // ![alt](src) / [text](url) / [text][ref] -> alt or text
        t = replace_md_links(&t);
        // strip HTML tags
        t = strip_html_tags(&t);
        // inline markers: `code`, ** __ * _ ~~ — remove the delimiters, keep text
        let t = t
            .replace("**", "")
            .replace("__", "")
            .replace("``", "")
            .replace('`', "")
            .replace("~~", "");
        // single * / _ emphasis leftovers (underscores inside words are kept by
        // only removing paired markers — simple approach: drop lone * and
        // `_` at word edges)
        let t = t.replace('*', "");
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(&t);
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn replace_wikilinks(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < s.len() {
        if s[i..].starts_with("[[") {
            if let Some(end) = s[i + 2..].find("]]") {
                let inner = &s[i + 2..i + 2 + end];
                if let Some((_, display)) = inner.split_once('|') {
                    out.push_str(display);
                } else {
                    out.push_str(inner);
                }
                i += 2 + end + 2;
                continue;
            }
        }
        out.push_str(&s[i..i + s[i..].chars().next().map(|c| c.len_utf8()).unwrap_or(1)]);
        i += s[i..].chars().next().map(|c| c.len_utf8()).unwrap_or(1);
    }
    out
}

fn replace_md_links(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(open) = rest.find('[') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        // image? trailing '!' belongs to the image syntax, not the text
        let is_image = out.ends_with('!');
        if is_image {
            out.pop();
        }
        if let Some(close) = after.find(']') {
            let label = &after[..close];
            let after_close = &after[close + 1..];
            if let Some(paren_end) = after_close.strip_prefix('(').and_then(|r| r.find(')')) {
                // [label](url) / ![alt](src) — keep the visible text
                out.push_str(label);
                rest = &after_close[paren_end + 2..];
                continue;
            }
            if after_close.starts_with('[') {
                // [label][ref]
                out.push_str(label);
                if let Some(ref_end) = after_close.find(']') {
                    rest = &after_close[ref_end + 1..];
                    continue;
                }
            }
            out.push('[');
            rest = after;
            continue;
        }
        out.push('[');
        rest = after;
    }
    out.push_str(rest);
    out
}

fn strip_html_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NoteImageMeta {
    pub id: Uuid,
    pub note_id: Uuid,
    pub name: String,
    pub mime: String,
    pub size: i64,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NoteImage {
    #[serde(flatten)]
    pub meta: NoteImageMeta,
    pub bytes: Vec<u8>,
}

// ---------- Calendar ----------

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Recurrence {
    Daily,
    Weekly,
    EveryNWeeks(u32),
    Monthly,
    Yearly,
    Weekdays,
    Custom,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum ReminderKind {
    AtTime,
    FiveMinutes,
    FifteenMinutes,
    ThirtyMinutes,
    OneHour,
    OneDay,
    OneWeek,
}

impl ReminderKind {
    pub fn minutes_before(self) -> i64 {
        match self {
            ReminderKind::AtTime => 0,
            ReminderKind::FiveMinutes => 5,
            ReminderKind::FifteenMinutes => 15,
            ReminderKind::ThirtyMinutes => 30,
            ReminderKind::OneHour => 60,
            ReminderKind::OneDay => 60 * 24,
            ReminderKind::OneWeek => 60 * 24 * 7,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Event {
    pub id: Uuid,
    pub title: String,
    pub notes: String,
    pub location: Option<String>,
    /// ISO-8601 local date-time without tz for all-day events, else UTC instant.
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    pub all_day: bool,
    pub color: Option<String>,
    pub recurrence: Option<Recurrence>,
    pub reminders: Vec<ReminderKind>,
    pub calendar_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Event {
    pub fn new(title: impl Into<String>, start: DateTime<Utc>, end: DateTime<Utc>) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            title: title.into(),
            notes: String::new(),
            location: None,
            start,
            end,
            all_day: false,
            color: None,
            recurrence: None,
            reminders: Vec::new(),
            calendar_id: None,
            created_at: now,
            updated_at: now,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Calendar {
    pub id: Uuid,
    pub name: String,
    pub color: String,
    pub visible: bool,
    pub sort_order: i32,
}

impl Calendar {
    pub fn new(name: impl Into<String>, color: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            color: color.into(),
            visible: true,
            sort_order: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::strip_markdown;

    #[test]
    fn strips_inline_markdown() {
        // the Welcome-note leak: Ctrl+N** must not survive into previews
        assert_eq!(strip_markdown("- **Ctrl+N** — new note"), "Ctrl+N — new note");
        assert_eq!(strip_markdown("__bold__ and `code`"), "bold and code");
        assert_eq!(strip_markdown("see [Docs](https://example.com) now"), "see Docs now");
        assert_eq!(strip_markdown("goto [[Welcome to Notes]]"), "goto Welcome to Notes");
        assert_eq!(strip_markdown("goto [[a|Alias]]"), "goto Alias");
        assert_eq!(strip_markdown("- [ ] buy milk"), "buy milk");
        assert_eq!(strip_markdown("- [x] done task"), "done task");
        assert_eq!(strip_markdown("![cat](cat.png) hello"), "cat hello");
    }
}
