//! yellod Notes — application logic.
//! UI lives in ui/notes.slint; state transitions are wired here.

#![windows_subsystem = "windows"]

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

use chrono::{DateTime, Local};
use slint::{ModelRc, SharedString, VecModel};
use uuid::Uuid;

use ycore::model::NoteSummary;
use ycore::storage::{notes as notes_store, open_notes_db};

slint::include_modules!();

struct AppState {
    db: rusqlite::Connection,
    config_path: PathBuf,
    dark: bool,
    seeded: bool,
    current_id: Option<Uuid>,
    view: i32,
    search: String,
    active_folder: Option<Uuid>,
    active_tag: Option<String>,
    dirty_generation: u64,
}

impl AppState {
    fn list_filter(&self) -> notes_store::ListFilter<'_> {
        notes_store::ListFilter {
            folder_id: self.active_folder,
            tag: self.active_tag.as_deref(),
            archived: self.view == 3,
            trashed: self.view == 4,
            favorites_only: self.view == 2,
            query: if self.search.trim().is_empty() {
                None
            } else {
                Some(self.search.trim())
            },
            limit: 600,
        }
    }
}

// ---------- helpers ----------

fn data_dir() -> PathBuf {
    let base = dirs::data_dir().unwrap_or_else(|| PathBuf::from("."));
    base.join("yellod").join("Notes")
}

fn rel_time(dt: DateTime<chrono::Utc>) -> String {
    let local = dt.with_timezone(&Local);
    let now = Local::now();
    let time = local.format("%I:%M %p").to_string();
    let time = time.trim_start_matches('0').to_string();
    if local.date_naive() == now.date_naive() {
        format!("Today, {time}")
    } else if local.date_naive() == now.date_naive() - chrono::Duration::days(1) {
        format!("Yesterday, {time}")
    } else {
        local.format("%b %d, %Y").to_string()
    }
}

fn edited_label(dt: DateTime<chrono::Utc>) -> String {
    let local = dt.with_timezone(&Local);
    format!("Edited {}", local.format("%I:%M %p").to_string().trim_start_matches('0'))
}

fn word_count(text: &str) -> i32 {
    text.split_whitespace().count() as i32
}

fn note_to_data(s: &NoteSummary) -> NoteData {
    let title = s.title.as_str();
    let preview = s.preview.as_str();
    NoteData {
        id: s.id.to_string().into(),
        title: title.into(),
        preview: preview.into(),
        date_label: rel_time(s.updated_at).into(),
        pinned: s.pinned,
        favorite: s.favorite,
        word_count: 0,
    }
}

fn shared(s: String) -> SharedString {
    s.into()
}

fn load_config(path: &PathBuf) -> serde_json::Value {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_else(|| serde_json::json!({"dark": true, "seeded": false}))
}

fn save_config(state: &AppState) {
    let cfg = serde_json::json!({ "dark": state.dark, "seeded": state.seeded });
    let _ = std::fs::write(&state.config_path, serde_json::to_string_pretty(&cfg).unwrap_or_default());
}

fn toast(ui: &MainWindow, msg: &str) {
    ui.set_toast_message(shared(msg.to_string()));
    ui.set_toast_visible(true);
}

// ---------- list refresh ----------

fn refresh_list(state: &Rc<RefCell<AppState>>, ui: &MainWindow) {
    let s = state.borrow();

    // pinned-only partition applies to the unfiltered All view
    let unfiltered = s.search.trim().is_empty()
        && s.active_folder.is_none()
        && s.active_tag.is_none()
        && (s.view == 0 || s.view == 1);

    let filter = s.list_filter();
    let summaries = notes_store::list_notes(&s.db, &filter).unwrap_or_default();

    let mut pinned_v: Vec<NoteData> = Vec::new();
    let mut others_v: Vec<NoteData> = Vec::new();

    if s.view == 1 {
        // Pinned view: show pinned notes, no sections
        others_v = summaries.iter().filter(|n| n.pinned).map(note_to_data).collect();
    } else if unfiltered && s.view == 0 {
        for n in &summaries {
            if n.pinned {
                pinned_v.push(note_to_data(n));
            } else {
                others_v.push(note_to_data(n));
            }
        }
    } else {
        others_v = summaries.iter().map(note_to_data).collect();
    }

    let total = (pinned_v.len() + others_v.len()) as i32;
    let show_sections = s.view == 0 && !pinned_v.is_empty();

    ui.set_pinned_section(show_sections);
    ui.set_pinned_notes(ModelRc::new(VecModel::from(pinned_v)));
    ui.set_notes(ModelRc::new(VecModel::from(others_v)));
    ui.set_note_count(total);

    // Folders with counts
    let mut folder_rows: Vec<FolderData> = Vec::new();
    if let Ok(folders) = notes_store::list_folders(&s.db) {
        for f in folders {
            let count: i64 = s
                .db
                .query_row(
                    "SELECT COUNT(*) FROM notes WHERE folder_id = ?1 AND trashed = 0 AND archived = 0",
                    rusqlite::params![f.id.to_string()],
                    |r| r.get(0),
                )
                .unwrap_or(0);
            folder_rows.push(FolderData {
                id: f.id.to_string().into(),
                name: f.name.into(),
                meta: if count == 0 { "".into() } else { count.to_string().into() },
            });
        }
    }
    ui.set_folders(ModelRc::new(VecModel::from(folder_rows)));

    // Tags with counts
    let mut tag_rows: Vec<TagData> = Vec::new();
    if let Ok(tags) = notes_store::all_tags(&s.db) {
        for (name, count) in tags {
            tag_rows.push(TagData {
                name: name.into(),
                meta: count.to_string().into(),
            });
        }
    }
    ui.set_tags(ModelRc::new(VecModel::from(tag_rows)));
}

fn apply_to_editor(ui: &MainWindow, note: &ycore::model::Note) {
    ui.set_editor_title(note.title.clone().into());
    ui.set_editor_body(note.body.clone().into());
    let tags: Vec<SharedString> = note.tags.iter().map(|t| shared(t.clone())).collect();
    ui.set_editor_tags(ModelRc::new(VecModel::from(tags)));
    ui.set_editor_pinned(note.pinned);
    ui.set_editor_favorite(note.favorite);
    ui.set_word_count(word_count(&note.body));
    ui.set_save_state(0);
    ui.set_edited_label(shared(edited_label(note.updated_at)));
    ui.set_selected_folder_of_editor(note.folder_id.map(|f| f.to_string()).unwrap_or_default().into());
    ui.set_has_note(true);
    ui.set_selected_id(note.id.to_string().into());
}

fn select_note(state: &Rc<RefCell<AppState>>, ui: &MainWindow, id: Uuid) {
    flush_note(state, ui);
    let s = state.borrow();
    if let Ok(Some(note)) = notes_store::get_note(&s.db, id) {
        drop(s);
        state.borrow_mut().current_id = Some(id);
        apply_to_editor(ui, &note);
    }
}

fn flush_note(state: &Rc<RefCell<AppState>>, ui: &MainWindow) {
    let maybe_id = state.borrow().current_id;
    let Some(id) = maybe_id else { return };
    let title: String = ui.get_editor_title().into();
    let body: String = ui.get_editor_body().into();
    let s = state.borrow();
    if let Ok(Some(mut note)) = notes_store::get_note(&s.db, id) {
        if note.title != title || note.body != body {
            note.title = title;
            note.body = body;
            let _ = notes_store::update_note(&s.db, &note);
        }
    }
}

fn show_toast_and_refresh(state: &Rc<RefCell<AppState>>, ui: &MainWindow, msg: &str) {
    flush_note(state, ui);
    refresh_list(state, ui);
    toast(ui, msg);
}

fn deselect_after_leaving_view(state: &Rc<RefCell<AppState>>, ui: &MainWindow) {
    state.borrow_mut().current_id = None;
    ui.set_has_note(false);
    ui.set_selected_id(SharedString::new());
}

// ---------- entry point ----------

pub fn run() -> anyhow::Result<()> {
    run_ui()
}

pub fn run_ui() -> anyhow::Result<()> {
    let ui = MainWindow::new()?;
    let dir = data_dir();
    std::fs::create_dir_all(&dir)?;
    let config_path = dir.join("config.json");
    let cfg = load_config(&config_path);
    let mut state = AppState {
        db: open_notes_db(&dir.join("notes.db"))?,
        config_path,
        dark: cfg.get("dark").and_then(|v| v.as_bool()).unwrap_or(true),
        seeded: cfg.get("seeded").and_then(|v| v.as_bool()).unwrap_or(false),
        current_id: None,
        view: 0,
        search: String::new(),
        active_folder: None,
        active_tag: None,
        dirty_generation: 0,
    };

    // Theme
    ui.set_dark(state.dark);

    // Seed welcome note on first run
    if !state.seeded {
        let count: i64 = state
            .db
            .query_row("SELECT COUNT(*) FROM notes", [], |r| r.get(0))
            .unwrap_or(0);
        if count == 0 {
            let welcome = ycore::model::Note::new(
                "Welcome to Notes 👻",
                "# Welcome to Notes\n\nA fast, private, local-first notes app — part of the yellod suite.\n\n## Get around\n\n- **Ctrl+N** — new note\n- **Ctrl+F** — search\n- **Pin** anything you want close\n- **Archive** finished thoughts\n- **Trash** holds deleted notes until you empty it\n\n## Your data\n\nEverything you write stays on this device in a local SQLite database. Nothing leaves your machine unless you tell it to.\n\n## Tips\n\n- Search uses full-text search — just start typing\n- Add tags with **#** in the tag row\n- Organize with folders in the sidebar (press the ☰ at the top-left)\n- Toggle light / dark from the moon icon at the bottom of the rail\n\nHappy writing.",
            );
            let _ = notes_store::create_note(&state.db, &welcome);
        }
        state.seeded = true;
        save_config(&state);
    }

    let state = Rc::new(RefCell::new(state));

    // Initial refresh + preselect latest note (desktop)
    refresh_list(&state, &ui);
    {
        let s = state.borrow();
        let first = notes_store::list_notes(&s.db, &notes_store::ListFilter::default());        if let Ok(first) = first {
            if let Some(first) = first.first() {
                let id = first.id;
                drop(s);
                select_note(&state, &ui, id);
            }
        }
    }

    // ---------- callback wiring ----------
    {
        let ui2 = ui.clone_strong();
        let st = state.clone();
        ui.on_new_note(move || {
            flush_note(&st, &ui2);
            let note = ycore::model::Note::new("", "");
            let id = note.id;
            {
                let s = st.borrow();
                let _ = notes_store::create_note(&s.db, &note);
            }
            refresh_list(&st, &ui2);
            select_note(&st, &ui2, id);
            ui2.set_save_state(0);
        });
    }

    {
        let ui2 = ui.clone_strong();
        let st = state.clone();
        ui.on_search_changed(move |text| {
            st.borrow_mut().search = text.to_string();
            refresh_list(&st, &ui2);
        });
    }

    {
        let ui2 = ui.clone_strong();
        let st = state.clone();
        ui.on_view_selected(move |view| {
            flush_note(&st, &ui2);
            {
                let mut s = st.borrow_mut();
                s.view = view;
                s.active_folder = None;
                s.active_tag = None;
            }
            deselect_after_leaving_view(&st, &ui2);
            ui2.set_active_view(view);
            ui2.set_active_folder(SharedString::new());
            ui2.set_active_tag(SharedString::new());
            refresh_list(&st, &ui2);
        });
    }

    {
        let ui2 = ui.clone_strong();
        let st = state.clone();
        ui.on_folder_selected(move |id| {
            flush_note(&st, &ui2);
            let fid = Uuid::parse_str(&id).ok();
            {
                let mut s = st.borrow_mut();
                s.view = 0;
                s.active_folder = fid;
                s.active_tag = None;
            }
            deselect_after_leaving_view(&st, &ui2);
            ui2.set_active_view(0);
            ui2.set_active_folder(shared(id.to_string()));
            ui2.set_active_tag(SharedString::new());
            refresh_list(&st, &ui2);
        });
    }

    {
        let ui2 = ui.clone_strong();
        let st = state.clone();
        ui.on_tag_selected(move |name| {
            flush_note(&st, &ui2);
            let n = name.to_string();
            {
                let mut s = st.borrow_mut();
                s.view = 0;
                s.active_tag = Some(if s.active_tag.as_deref() == Some(n.as_str()) { String::new() } else { n.clone() });
                if s.active_tag.as_deref() == Some("") {
                    s.active_tag = None;
                }
                s.active_folder = None;
            }
            deselect_after_leaving_view(&st, &ui2);
            ui2.set_active_tag(st.borrow().active_tag.clone().map(shared).unwrap_or_default());
            ui2.set_active_folder(SharedString::new());
            ui2.set_active_view(0);
            refresh_list(&st, &ui2);
        });
    }

    {
        let ui2 = ui.clone_strong();
        let st = state.clone();
        ui.on_note_activated(move |id| {
            if let Ok(fid) = Uuid::parse_str(&id) {
                select_note(&st, &ui2, fid);
            }
        });
    }

    {
        let ui2 = ui.clone_strong();
        let st = state.clone();
        ui.on_pin_toggled(move |id| {
            if let Ok(nid) = Uuid::parse_str(&id) {
                let s = st.borrow();
                if let Ok(Some(mut note)) = notes_store::get_note(&s.db, nid) {
                    note.pinned = !note.pinned;
                    let _ = notes_store::update_note(&s.db, &note);
                }
                drop(s);
                if st.borrow().current_id == Some(nid) {
                    ui2.set_editor_pinned({
                        let s = st.borrow();
                        notes_store::get_note(&s.db, nid).ok().flatten().map(|n| n.pinned).unwrap_or(false)
                    });
                }
                refresh_list(&st, &ui2);
            }
        });
    }

    {
        let ui2 = ui.clone_strong();
        let st = state.clone();
        ui.on_toggle_pin_editor(move || {
            let id = st.borrow().current_id;
            if let Some(id) = id {
                let s = st.borrow();
                if let Ok(Some(mut note)) = notes_store::get_note(&s.db, id) {
                    note.pinned = !note.pinned;
                    let new_state = note.pinned;
                    let _ = notes_store::update_note(&s.db, &note);
                    drop(s);
                    ui2.set_editor_pinned(new_state);
                    refresh_list(&st, &ui2);
                }
            }
        });
    }

    {
        let ui2 = ui.clone_strong();
        let st = state.clone();
        ui.on_toggle_favorite_editor(move || {
            let id = st.borrow().current_id;
            if let Some(id) = id {
                let s = st.borrow();
                if let Ok(Some(mut note)) = notes_store::get_note(&s.db, id) {
                    note.favorite = !note.favorite;
                    let new_state = note.favorite;
                    let _ = notes_store::update_note(&s.db, &note);
                    drop(s);
                    ui2.set_editor_favorite(new_state);
                    refresh_list(&st, &ui2);
                }
            }
        });
    }

    {
        let ui2 = ui.clone_strong();
        let st = state.clone();
        ui.on_archive_editor(move || {
            let id = st.borrow().current_id;
            if let Some(id) = id {
                let was_archived = {
                    let s = st.borrow();
                    notes_store::get_note(&s.db, id).ok().flatten().map(|n| n.archived).unwrap_or(false)
                };
                let s = st.borrow();
                if let Ok(Some(mut note)) = notes_store::get_note(&s.db, id) {
                    note.archived = !note.archived;
                    let _ = notes_store::update_note(&s.db, &note);
                }
                drop(s);
                show_toast_and_refresh(&st, &ui2, if was_archived { "Unarchived" } else { "Archived" });
            }
        });
    }

    {
        let ui2 = ui.clone_strong();
        let st = state.clone();
        ui.on_trash_editor(move || {
            let id = st.borrow().current_id;
            if let Some(id) = id {
                let s = st.borrow();
                if let Ok(Some(mut note)) = notes_store::get_note(&s.db, id) {
                    note.trashed = true;
                    let _ = notes_store::update_note(&s.db, &note);
                }
                drop(s);
                show_toast_and_refresh(&st, &ui2, "Moved to Trash");
            }
        });
    }

    {
        let ui2 = ui.clone_strong();
        let st = state.clone();
        ui.on_restore_editor(move || {
            let id = st.borrow().current_id;
            if let Some(id) = id {
                let s = st.borrow();
                if let Ok(Some(mut note)) = notes_store::get_note(&s.db, id) {
                    note.trashed = false;
                    let _ = notes_store::update_note(&s.db, &note);
                }
                drop(s);
                show_toast_and_refresh(&st, &ui2, "Restored");
            }
        });
    }

    {
        let ui2 = ui.clone_strong();
        let st = state.clone();
        ui.on_delete_forever_confirmed(move || {
            let id = st.borrow().current_id;
            if let Some(id) = id {
                let s = st.borrow();
                let _ = notes_store::delete_note_hard(&s.db, id);
                drop(s);
                deselect_after_leaving_view(&st, &ui2);
                ui2.set_show_delete_forever(false);
                refresh_list(&st, &ui2);
                toast(&ui2, "Note deleted forever");
            }
        });
    }

    {
        let ui2 = ui.clone_strong();
        let st = state.clone();
        ui.on_empty_trash_confirmed(move || {
            let s = st.borrow();
            if let Ok(notes) = notes_store::list_notes(&s.db, &notes_store::ListFilter {
                trashed: true,
                ..Default::default()
            }) {
                for n in notes {
                    let _ = notes_store::delete_note_hard(&s.db, n.id);
                }
            }
            drop(s);
            deselect_after_leaving_view(&st, &ui2);
            ui2.set_show_empty_trash(false);
            refresh_list(&st, &ui2);
            toast(&ui2, "Trash emptied");
        });
    }

    {
        let ui2 = ui.clone_strong();
        let st = state.clone();
        ui.on_copy_editor(move || {
            let id = st.borrow().current_id;
            if let Some(id) = id {
                let s = st.borrow();
                if let Ok(Some(note)) = notes_store::get_note(&s.db, id) {
                    let text = if note.title.trim().is_empty() {
                        note.body.clone()
                    } else {
                        format!("{}\n\n{}", note.title, note.body)
                    };
                    drop(s);
                    #[cfg(not(target_os = "android"))]
                    if let Ok(mut cb) = arboard::Clipboard::new() {
                        let _ = cb.set_text(text);
                    }
                    #[cfg(target_os = "android")]
                    let _ = text;
                    toast(&ui2, "Copied to clipboard");
                }
            }
        });
    }

    // ---------- editor content / debounce ----------
    {
        let ui2 = ui.clone_strong();
        let st = state.clone();
        ui.on_editor_content_changed(move || {
            let body: String = ui2.get_editor_body().into();
            ui2.set_word_count(word_count(&body));
            ui2.set_save_state(1);
            let generation = {
                let mut s = st.borrow_mut();
                s.dirty_generation += 1;
                s.dirty_generation
            };
            let ui3 = ui2.clone_strong();
            let st3 = st.clone();
            slint::Timer::single_shot(Duration::from_millis(350), move || {
                if st3.borrow().dirty_generation == generation {
                    flush_note(&st3, &ui3);
                    ui3.set_save_state(2);
                    ui3.set_edited_label(shared(edited_label(chrono::Utc::now())));
                    refresh_list(&st3, &ui3);
                }
            });
        });
    }

    // ---------- tags ----------
    {
        let ui2 = ui.clone_strong();
        let st = state.clone();
        ui.on_tag_added(move |raw| {
            let tag = raw.trim().trim_start_matches('#').to_string();
            if tag.is_empty() {
                return;
            }
            let tag_l = tag.to_lowercase();
            let id = st.borrow().current_id;
            if let Some(id) = id {
                let s = st.borrow();
                if let Ok(Some(mut note)) = notes_store::get_note(&s.db, id) {
                    if !note.tags.iter().any(|t| t.to_lowercase() == tag_l) {
                        note.tags.push(tag_l);
                        let _ = notes_store::update_note(&s.db, &note);
                        drop(s);
                        // refresh editor tag chips
                        let s = st.borrow();
                        if let Ok(Some(note)) = notes_store::get_note(&s.db, id) {
                            let tags: Vec<SharedString> = note.tags.iter().map(|t| shared(t.clone())).collect();
                            ui2.set_editor_tags(ModelRc::new(VecModel::from(tags)));
                        }
                        drop(s);
                        refresh_list(&st, &ui2);
                    }
                }
            }
        });
    }

    {
        let ui2 = ui.clone_strong();
        let st = state.clone();
        ui.on_tag_removed(move |tag| {
            let tag = tag.to_string();
            let id = st.borrow().current_id;
            if let Some(id) = id {
                let s = st.borrow();
                if let Ok(Some(mut note)) = notes_store::get_note(&s.db, id) {
                    note.tags.retain(|t| t != &tag);
                    let _ = notes_store::update_note(&s.db, &note);
                    drop(s);
                    let s = st.borrow();
                    if let Ok(Some(note)) = notes_store::get_note(&s.db, id) {
                        let tags: Vec<SharedString> = note.tags.iter().map(|t| shared(t.clone())).collect();
                        ui2.set_editor_tags(ModelRc::new(VecModel::from(tags)));
                    }
                    drop(s);
                    refresh_list(&st, &ui2);
                }
            }
        });
    }

    // ---------- folders ----------
    {
        let ui2 = ui.clone_strong();
        ui.on_new_folder(move || {
            ui2.set_new_folder_open(true);
            ui2.set_new_folder_name(SharedString::new());
        });
    }

    {
        let ui2 = ui.clone_strong();
        let st = state.clone();
        ui.on_create_folder(move |name| {
            let name = name.trim().to_string();
            if name.is_empty() {
                ui2.set_new_folder_open(false);
                return;
            }
            let folder = ycore::model::Folder::new(name);
            {
                let s = st.borrow();
                let _ = notes_store::create_folder(&s.db, &folder);
            }
            ui2.set_new_folder_open(false);
            ui2.set_new_folder_name(SharedString::new());
            refresh_list(&st, &ui2);
            toast(&ui2, "Folder created");
        });
    }

    {
        let ui2 = ui.clone_strong();
        ui.on_cancel_new_folder(move || {
            ui2.set_new_folder_open(false);
            ui2.set_new_folder_name(SharedString::new());
        });
    }

    {
        let ui2 = ui.clone_strong();
        let st = state.clone();
        ui.on_delete_folder(move |id| {
            if let Ok(fid) = Uuid::parse_str(&id) {
                let s = st.borrow();
                let _ = notes_store::delete_folder(&s.db, fid);
                drop(s);
                if st.borrow().active_folder == Some(fid) {
                    st.borrow_mut().active_folder = None;
                    ui2.set_active_folder(SharedString::new());
                }
                refresh_list(&st, &ui2);
                toast(&ui2, "Folder deleted (notes kept)");
            }
        });
    }

    {
        let ui2 = ui.clone_strong();
        let st = state.clone();
        ui.on_assign_folder(move |id| {
            let id = id.to_string();
            let fid = if id.trim().is_empty() { None } else { Uuid::parse_str(&id).ok() };
            let nid = st.borrow().current_id;
            if let Some(nid) = nid {
                let s = st.borrow();
                if let Ok(Some(mut note)) = notes_store::get_note(&s.db, nid) {
                    note.folder_id = fid;
                    let _ = notes_store::update_note(&s.db, &note);
                }
                drop(s);
                ui2.set_selected_folder_of_editor(id.into());
                ui2.set_folder_menu_open(false);
                refresh_list(&st, &ui2);
                toast(&ui2, if fid.is_some() { "Moved to folder" } else { "Removed from folder" });
            }
        });
    }

    // ---------- chrome ----------
    {
        let ui2 = ui.clone_strong();
        ui.on_toggle_sidebar(move || {
            ui2.set_sidebar_open(!ui2.get_sidebar_open());
        });
    }



    {
        let ui2 = ui.clone_strong();
        ui.on_toggle_view_mode(move || {
            ui2.set_list_view(!ui2.get_list_view());
        });
    }

    {
        let ui2 = ui.clone_strong();
        let st = state.clone();
        ui.on_toggle_theme(move || {
            let now_dark = !ui2.get_dark();
            ui2.set_dark(now_dark);
            {
                let mut s = st.borrow_mut();
                s.dark = now_dark;
            }
            save_config(&st.borrow());
        });
    }

    {
        let ui2 = ui.clone_strong();
        let st = state.clone();
        ui.on_back_to_list(move || {
            flush_note(&st, &ui2);
            deselect_after_leaving_view(&st, &ui2);
        });
    }

    {
        let ui2 = ui.clone_strong();
        ui.on_window_resized(move |width| {
            let w = width;
            let compact = w < 880.0;
            let cols = if w < 1250.0 { 2 } else if w < 1650.0 { 3 } else { 4 };
            ui2.set_compact(compact);
            ui2.set_grid_cols(cols);
        });
    }

    // Debounced initial layout pass: sync compact state for small screens
    {
        let ui2 = ui.clone_strong();
        slint::Timer::single_shot(Duration::from_millis(150), move || {
            // Give the window a size that actually fits this screen (handles high-DPI
            // screens where slint's logical units map to fewer physical pixels).
            ui2.window().set_size(slint::LogicalSize::new(880.0, 600.0));
        });
    }


    ui.run()?;
    Ok(())
}

// ---------- Android entry point ----------
#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub fn android_main(app: slint::android::AndroidApp) {
    slint::android::init(app).unwrap();
    if let Err(e) = run_ui() {
        eprintln!("[notes] run_ui failed: {e:?}");
    }
}
