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
        let preview = strip_markdown(&n.body).chars().take(160).collect();
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

/// Cheap markdown → plain text for previews.
pub fn strip_markdown(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for line in input.lines() {
        let t = line
            .trim_start_matches(['#', '>', '-', '*', '+', ' '])
            .trim_start_matches(|c: char| c.is_ascii_digit() || c == '.' || c == ')');
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(t);
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
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
