//! yellod Notes — application logic.
//! UI lives in ui/notes.slint; state transitions are wired here.

#![windows_subsystem = "windows"]

use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

use chrono::{DateTime, Local, Utc};
use slint::{Image, ModelRc, Rgba8Pixel, SharedPixelBuffer, SharedString, VecModel};
use uuid::Uuid;

use ycore::crypto::{self, VaultConfig, VaultKey};
use ycore::model::{NoteImage, NoteImageMeta, NoteSummary, NotesSort};
use ycore::storage::{notes as notes_store, open_notes_db};

/// Canonical note-card background colors (Keep-style dark palette, matches web).
/// Index in this array == (note color index - 1). Stored on the Note.color field.
const NOTE_COLOR_HEX: [&str; 11] = [
    "#5C2B29", "#614A19", "#635D18", "#345920", "#16504B", "#1E3A5F", "#2D555E", "#42275E", "#5B2245", "#442F19", "#2C2D2F",
];

fn color_index(color: &Option<String>) -> i32 {
    match color {
        Some(c) => NOTE_COLOR_HEX
            .iter()
            .position(|h| h.eq_ignore_ascii_case(c))
            .map(|i| i as i32 + 1)
            .unwrap_or(0),
        None => 0,
    }
}

fn color_hex(idx: i32) -> Option<String> {
    if idx <= 0 {
        None
    } else {
        NOTE_COLOR_HEX.get((idx - 1) as usize).map(|s| s.to_string())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
struct Prefs {
    layout: String,          // keep | notion | obsidian | neo | graph
    density: String,         // compact | cozy | roomy
    mobile_two: bool,
    sort: NotesSort,
    pinned_first: bool,
    md_cards: bool,
    show_dates: bool,
    clamp: i32,              // 0 = unlimited; otherwise max preview lines
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            layout: "keep".into(),
            density: "cozy".into(),
            mobile_two: true,
            sort: NotesSort::Updated,
            pinned_first: true,
            md_cards: false,
            show_dates: true,
            clamp: 5,
        }
    }
}

impl Prefs {
    fn layout_index(&self) -> i32 {
        match self.layout.as_str() {
            "notion" => 1,
            "obsidian" => 2,
            "neo" => 3,
            "graph" => 4,
            _ => 0,
        }
    }
    fn density_index(&self) -> i32 {
        match self.density.as_str() {
            "compact" => 0,
            "roomy" => 2,
            _ => 1,
        }
    }
    fn sort_index(&self) -> i32 {
        match self.sort {
            NotesSort::Updated => 0,
            NotesSort::Created => 1,
            NotesSort::Title => 2,
            NotesSort::Manual => 3,
        }
    }
}

#[derive(Debug, Clone)]
struct GraphPosition {
    x: f32,
    y: f32,
    z: f32,
}

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
    vault_cfg: VaultConfig,
    vault_key: Option<VaultKey>,
    vault_mismatch: bool,
    prefs: Prefs,
    graph_positions: std::collections::HashMap<Uuid, GraphPosition>,
    graph_yaw: f32,
    graph_pitch: f32,
    graph_zoom: f32,
    graph_selected: Option<Uuid>,
    graph_drag: Option<(Uuid, f32, f32)>,
    graph_rotate: Option<(f32, f32)>,
    graph_press: Option<(Option<Uuid>, f32, f32)>,
    graph_last_click: Option<(Uuid, std::time::Instant)>,
    graph_screen_positions: Vec<(Uuid, f32, f32, f32)>,
    doodle_context: u8, // 0 editor · 1 composer
    doodle_strokes: Vec<DoodleInternalStroke>,
    pending_note_images: Vec<NoteImage>,
    current_images: Vec<NoteImageMeta>,
}

#[derive(Debug, Clone)]
struct DoodleInternalStroke {
    id: i32,
    color: String,
    size: f32,
    points: Vec<(f32, f32)>,
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
            sort: self.prefs.sort,
            limit: 600,
        }
    }
}

// ---------- helpers ----------

fn data_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("YELLOD_NOTES_DATA_DIR") {
        if !dir.trim().is_empty() {
            return PathBuf::from(dir);
        }
    }
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

fn estimate_card_height_px(title: &str, preview: &str, clamp_lines: i32) -> f32 {
    let preview_chars = preview.chars().count();
    let title_lines = if title.chars().count() > 34 { 2 } else { 1 };
    let raw_lines = preview_chars.div_ceil(42);
    let mut body_lines = raw_lines.max(1);
    if clamp_lines > 0 {
        body_lines = body_lines.clamp(2, clamp_lines as usize);
    } else {
        body_lines = body_lines.clamp(2, 16);
    }
    (64.0 + (title_lines as f32 * 24.0) + (body_lines as f32 * 20.0)).clamp(150.0, 460.0)
}

fn styled_markdown(text: &str) -> slint::StyledText {
    slint::StyledText::from_markdown(text).unwrap_or_else(|_| slint::StyledText::from_plain_text(text))
}

fn parse_checklist_items(body: &str) -> Vec<ChecklistItemData> {
    let mut items = Vec::new();
    for (line_index, line) in body.lines().enumerate() {
        let t = line.trim_start();
        let (checked, rest) = if let Some(rest) = t.strip_prefix("- [ ]").or_else(|| t.strip_prefix("* [ ]")).or_else(|| t.strip_prefix("+ [ ]")) {
            (false, rest)
        } else if let Some(rest) = t.strip_prefix("- [x]").or_else(|| t.strip_prefix("- [X]")).or_else(|| t.strip_prefix("* [x]")).or_else(|| t.strip_prefix("* [X]")).or_else(|| t.strip_prefix("+ [x]")).or_else(|| t.strip_prefix("+ [X]")) {
            (true, rest)
        } else {
            continue;
        };
        items.push(ChecklistItemData {
            line: line_index as i32,
            text: rest.trim_start().to_string().into(),
            checked,
        });
    }
    items
}

fn replace_line(body: &str, line: usize, replacement: String) -> String {
    body.lines()
        .enumerate()
        .map(|(i, l)| if i == line { replacement.as_str() } else { l })
        .collect::<Vec<_>>()
        .join("\n")
}

fn toggle_checklist_body(body: &str, line: usize) -> String {
    let mut out = String::with_capacity(body.len() + 2);
    for (i, l) in body.lines().enumerate() {
        if i > 0 { out.push('\n'); }
        if i != line {
            out.push_str(l);
            continue;
        }
        if l.contains("[ ]") { out.push_str(&l.replacen("[ ]", "[x]", 1)); }
        else if l.contains("[x]") { out.push_str(&l.replacen("[x]", "[ ]", 1)); }
        else if l.contains("[X]") { out.push_str(&l.replacen("[X]", "[ ]", 1)); }
        else { out.push_str(l); }
    }
    if body.ends_with('\n') { out.push('\n'); }
    out
}

fn delete_line_body(body: &str, line: usize) -> String {
    body.lines()
        .enumerate()
        .filter(|(i, _)| *i != line)
        .map(|(_, l)| l)
        .collect::<Vec<_>>()
        .join("\n")
}

fn safe_filename(name: &str, fallback: &str) -> String {
    let cleaned = name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || matches!(c, ' ' | '-' | '_' | '.') { c } else { '_' })
        .collect::<String>()
        .trim()
        .to_string();
    if cleaned.is_empty() { fallback.to_string() } else { cleaned }
}

fn format_bytes(size: i64) -> String {
    if size < 1024 { format!("{size} B") }
    else if size < 1024 * 1024 { format!("{:.1} KB", size as f64 / 1024.0) }
    else { format!("{:.1} MB", size as f64 / (1024.0 * 1024.0)) }
}

fn decode_image_bytes(bytes: &[u8], max_dim: Option<u32>) -> Option<Image> {
    let decoded = image::load_from_memory(bytes).ok()?;
    let rgba = match max_dim {
        Some(dim) => decoded.resize(dim, dim, image::imageops::FilterType::Triangle).to_rgba8(),
        None => decoded.to_rgba8(),
    };
    let (w, h) = rgba.dimensions();
    let mut buffer = SharedPixelBuffer::<Rgba8Pixel>::new(w, h);
    buffer.make_mut_bytes().copy_from_slice(rgba.as_raw());
    Some(Image::from_rgba8(buffer))
}

fn mime_for_path(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).as_deref() {
        Some("png") => "image/png",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("bmp") => "image/bmp",
        Some("ico") => "image/x-icon",
        Some("jpg" | "jpeg") => "image/jpeg",
        _ => "application/octet-stream",
    }
}

fn doodle_png(strokes: &[DoodleInternalStroke]) -> anyhow::Result<Vec<u8>> {
    const W: u32 = 1200;
    const H: u32 = 800;
    let mut img = image::RgbaImage::from_pixel(W, H, image::Rgba([0, 0, 0, 0]));
    for stroke in strokes {
        let rgba = parse_hex_rgba(&stroke.color).unwrap_or([93, 216, 255, 255]);
        let radius = (stroke.size / 2.0).max(2.0) as i32;
        for window in stroke.points.windows(2) {
            let (x0, y0) = window[0];
            let (x1, y1) = window[1];
            let dx = x1 - x0;
            let dy = y1 - y0;
            let steps = (dx.abs().max(dy.abs()) / 2.0).ceil().max(1.0) as i32;
            for step in 0..=steps {
                let t = step as f32 / steps as f32;
                let x = (x0 + dx * t).round() as i32;
                let y = (y0 + dy * t).round() as i32;
                draw_disc(&mut img, x, y, radius, rgba);
            }
        }
    }
    let mut bytes = Vec::new();
    image::DynamicImage::ImageRgba8(img).write_to(&mut std::io::Cursor::new(&mut bytes), image::ImageFormat::Png)?;
    Ok(bytes)
}

fn parse_hex_rgba(s: &str) -> Option<[u8; 4]> {
    let h = s.trim_start_matches('#');
    let v = match h.len() {
        6 => u32::from_str_radix(h, 16).ok()? << 8 | 0xff,
        8 => u32::from_str_radix(h, 16).ok()?,
        _ => return None,
    };
    Some([((v >> 24) & 0xff) as u8, ((v >> 16) & 0xff) as u8, ((v >> 8) & 0xff) as u8, (v & 0xff) as u8])
}

fn draw_disc(img: &mut image::RgbaImage, cx: i32, cy: i32, radius: i32, color: [u8; 4]) {
    let (w, h) = img.dimensions();
    for y in (cy - radius)..=(cy + radius) {
        for x in (cx - radius)..=(cx + radius) {
            if x < 0 || y < 0 || x >= w as i32 || y >= h as i32 { continue; }
            let dx = x - cx;
            let dy = y - cy;
            if dx * dx + dy * dy <= radius * radius {
                img.put_pixel(x as u32, y as u32, image::Rgba(color));
            }
        }
    }
}

fn move_note_in_order(state: &Rc<RefCell<AppState>>, ui: &MainWindow, id: Uuid, delta: i32) {
    let s = state.borrow();
    let filter = s.list_filter();
    let summaries = notes_store::list_notes_with_key(&s.db, &filter, s.vault_key.as_ref()).unwrap_or_default();
    let mut ids: Vec<Uuid> = summaries.iter().map(|n| n.id).collect();
    let Some(pos) = ids.iter().position(|n| *n == id) else { return };
    let target = pos as i32 + delta;
    if target < 0 || target >= ids.len() as i32 { return; }
    let current_pinned = summaries[pos].pinned;
    if summaries[target as usize].pinned != current_pinned { return; }
    ids.swap(pos, target as usize);
    let _ = notes_store::set_note_order(&s.db, &ids);
    drop(s);
    refresh_list(state, ui);
    refresh_graph(state, ui);
}

fn note_to_data(s: &NoteSummary, prefs: &Prefs) -> NoteData {
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
        color_index: color_index(&s.color),
        image_count: s.image_count as i32,
        card_height: estimate_card_height_px(title, preview, prefs.clamp),
        render_md: prefs.md_cards,
        preview_styled: styled_markdown(preview),
    }
}

fn shared(s: String) -> SharedString {
    s.into()
}

#[derive(Clone)]
struct LoadedConfig {
    value: serde_json::Value,
    dark: bool,
    seeded: bool,
    vault: VaultConfig,
    prefs: Prefs,
}

fn load_config(path: &PathBuf) -> LoadedConfig {
    let value = std::fs::read_to_string(path)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_else(|| serde_json::json!({"dark": true, "seeded": false}));
    let dark = value.get("dark").and_then(|v| v.as_bool()).unwrap_or(true);
    let seeded = value.get("seeded").and_then(|v| v.as_bool()).unwrap_or(false);
    let vault = value.get("vault")
        .cloned()
        .and_then(|v| serde_json::from_value::<VaultConfig>(v).ok())
        .unwrap_or_default();
    let prefs = value.get("prefs")
        .cloned()
        .and_then(|v| serde_json::from_value::<Prefs>(v).ok())
        .unwrap_or_default();
    LoadedConfig { value, dark, seeded, vault, prefs }
}

fn save_config(state: &AppState) {
    let cfg = serde_json::json!({
        "dark": state.dark,
        "seeded": state.seeded,
        "vault": state.vault_cfg,
        "prefs": state.prefs,
    });
    let _ = std::fs::write(&state.config_path, serde_json::to_string_pretty(&cfg).unwrap_or_default());
}

fn get_from_state(s: &AppState, id: Uuid) -> Option<ycore::model::Note> {
    notes_store::get_note_with_key(&s.db, id, s.vault_key.as_ref()).ok().flatten()
}

fn update_from_state(s: &AppState, note: &ycore::model::Note) -> rusqlite::Result<()> {
    let old = get_from_state(s, note.id);
    let mut note = note.clone();
    note.encrypted = s.vault_cfg.enabled;
    notes_store::update_note_with_key(&s.db, &note, s.vault_key.as_ref())?;
    if old.as_ref().map(|o| (o.title.as_str(), o.body.as_str()) != (note.title.as_str(), note.body.as_str())).unwrap_or(false) {
        let _ = notes_store::log_activity(&s.db, note.id, "Edited");
    }
    Ok(())
}

fn record_activity(s: &AppState, id: Uuid, kind: &str) {
    let _ = notes_store::log_activity(&s.db, id, kind);
}

fn activity_strings(s: &AppState, id: Uuid) -> Vec<String> {
    notes_store::list_activity(&s.db, id, 8)
        .unwrap_or_default()
        .into_iter()
        .map(|row| format!("{} — {}", row.kind, row.created_at.with_timezone(&Local).format("%b %d, %I:%M %p")))
        .collect()
}

fn graph_dimensions(ui: &MainWindow) -> (f32, f32) {
    let physical = ui.window().size();
    let scale = ui.window().scale_factor().max(0.5);
    let w = physical.width as f32 / scale;
    let h = physical.height as f32 / scale;
    let sidebar = if ui.get_sidebar_open() { sidebar_width() } else { 0.0 };
    ((w - 72.0 - sidebar).max(320.0), (h - 64.0 - 26.0).max(240.0))
}

fn sidebar_width() -> f32 { 264.0 }

fn hash_uuid(id: Uuid) -> u64 {
    let bytes = id.as_bytes();
    u64::from_be_bytes(bytes[..8].try_into().unwrap_or([0; 8]))
}

fn random_graph_position(seed: Option<u64>) -> GraphPosition {
    use rand::{Rng, SeedableRng};
    let mut rng = match seed {
        Some(seed) => rand::rngs::StdRng::seed_from_u64(seed.max(1)),
        None => rand::rngs::StdRng::from_entropy(),
    };
    loop {
        let x: f32 = rng.gen_range(-1.0..=1.0);
        let y: f32 = rng.gen_range(-1.0..=1.0);
        let z: f32 = rng.gen_range(-1.0..=1.0);
        let len = (x * x + y * y + z * z).sqrt();
        if len > 0.25 && len <= 1.0 {
            return GraphPosition { x: x / len * 0.92, y: y / len * 0.92, z: z / len * 0.92 };
        }
    }
}

fn graph_project(p: &GraphPosition, yaw: f32, pitch: f32, zoom: f32, width: f32, height: f32) -> (f32, f32, f32) {
    let sy = yaw.sin();
    let cy = yaw.cos();
    let sp = pitch.sin();
    let cp = pitch.cos();
    let x1 = p.x * cy - p.z * sy;
    let z1 = p.x * sy + p.z * cy;
    let y2 = p.y * cp - z1 * sp;
    let z2 = p.y * sp + z1 * cp;
    let perspective = 3.6 / (3.6 + z2);
    let radius = width.min(height) * 0.34 * zoom;
    ((width / 2.0) + x1 * radius * perspective, (height / 2.0) + y2 * radius * perspective, perspective)
}

fn all_graph_notes(state: &AppState) -> Vec<ycore::model::Note> {
    let mut out = Vec::new();
    for id in list_all_ids(state) {
        let Some(note) = get_from_state(state, id) else { continue };
        if note.trashed { continue; }
        out.push(note);
    }
    out.sort_by_key(|n| std::cmp::Reverse(n.updated_at));
    out
}

fn refresh_graph(state: &Rc<RefCell<AppState>>, ui: &MainWindow) {
    let notes = {
        let s = state.borrow();
        all_graph_notes(&s)
    };
    let (width, height) = graph_dimensions(ui);
    let (yaw, pitch, zoom, selected) = {
        let s = state.borrow();
        (s.graph_yaw, s.graph_pitch, s.graph_zoom, s.graph_selected)
    };

    {
        let mut s = state.borrow_mut();
        let keep: std::collections::HashSet<Uuid> = notes.iter().map(|n| n.id).collect();
        s.graph_positions.retain(|id, _| keep.contains(id));
        for note in &notes {
            s.graph_positions.entry(note.id).or_insert_with(|| random_graph_position(Some(hash_uuid(note.id))));
        }
    }

    let mut by_title: std::collections::HashMap<String, Uuid> = std::collections::HashMap::new();
    let mut degree: std::collections::HashMap<Uuid, i32> = std::collections::HashMap::new();
    let mut edges: Vec<(Uuid, Uuid)> = Vec::new();
    for note in &notes {
        let title = note.title.trim().to_lowercase();
        if !title.is_empty() { by_title.insert(title, note.id); }
    }
    for note in &notes {
        for link in parse_wiki_links(&note.body) {
            let Some(target) = by_title.get(&link.to_lowercase()).copied() else { continue };
            if target == note.id { continue; }
            let mut pair = (note.id, target);
            if pair.0.as_bytes() > pair.1.as_bytes() { pair = (target, note.id); }
            if !edges.contains(&pair) { edges.push(pair); }
            *degree.entry(note.id).or_default() += 1;
            *degree.entry(target).or_default() += 1;
        }
    }

    let s = state.borrow();
    let mut screen: Vec<(Uuid, f32, f32, f32)> = Vec::with_capacity(notes.len());
    let mut ui_nodes: Vec<GraphNodeData> = Vec::with_capacity(notes.len());
    for note in &notes {
        let Some(pos) = s.graph_positions.get(&note.id) else { continue };
        let (x, y, perspective) = graph_project(pos, yaw, pitch, zoom, width, height);
        let deg = *degree.get(&note.id).unwrap_or(&0);
        let radius = (9.0 + deg as f32 * 2.4).clamp(10.0, 19.0) * perspective.clamp(0.72, 1.25);
        screen.push((note.id, x, y, radius));
        ui_nodes.push(GraphNodeData {
            id: note.id.to_string().into(),
            title: note.title.clone().into(),
            x,
            y,
            radius,
            linked: deg,
            active: selected == Some(note.id),
        });
    }
    let node_by_id: std::collections::HashMap<Uuid, &(Uuid, f32, f32, f32)> = screen.iter().map(|p| (p.0, p)).collect();
    let mut ui_links: Vec<GraphLinkData> = Vec::with_capacity(edges.len());
    for (a, b) in edges {
        let Some(a) = node_by_id.get(&a).copied() else { continue };
        let Some(b) = node_by_id.get(&b).copied() else { continue };
        let active = selected == Some(a.0) || selected == Some(b.0);
        ui_links.push(GraphLinkData {
            id: format!("{}:{}", a.0, b.0).into(),
            commands: format!("M {:.1} {:.1} L {:.1} {:.1}", a.1, a.2, b.1, b.2).into(),
            active,
        });
    }
    drop(s);
    state.borrow_mut().graph_screen_positions = screen;
    ui.set_graph_nodes(ModelRc::new(VecModel::from(ui_nodes)));
    ui.set_graph_links(ModelRc::new(VecModel::from(ui_links)));
    update_graph_panel(state, ui, selected);
}

fn update_graph_panel(state: &Rc<RefCell<AppState>>, ui: &MainWindow, selected: Option<Uuid>) {
    let Some(id) = selected else {
        ui.set_graph_has_selection(false);
        ui.set_graph_selected_id(SharedString::new());
        return;
    };
    let s = state.borrow();
    let Some(note) = get_from_state(&s, id) else {
        drop(s);
        state.borrow_mut().graph_selected = None;
        ui.set_graph_has_selection(false);
        ui.set_graph_selected_id(SharedString::new());
        return;
    };
    ui.set_graph_has_selection(true);
    ui.set_graph_selected_id(id.to_string().into());
    ui.set_graph_selected_title(note.title.clone().into());
    ui.set_graph_selected_meta(shared(format!("Created {} · Edited {}", note.created_at.with_timezone(&Local).format("%b %d, %Y"), note.updated_at.with_timezone(&Local).format("%b %d, %Y %I:%M %p"))));
    ui.set_graph_selected_markdown(styled_markdown(&note.body));
    let links: Vec<SharedString> = parse_wiki_links(&note.body).into_iter().map(shared).collect();
    ui.set_graph_selected_links(ModelRc::new(VecModel::from(links)));
}

fn graph_hit(state: &Rc<RefCell<AppState>>, x: f32, y: f32) -> Option<Uuid> {
    state.borrow()
        .graph_screen_positions
        .iter()
        .find(|(_, nx, ny, radius)| (x - *nx).hypot(y - *ny) <= radius + 5.0)
        .map(|p| p.0)
}

fn append_wiki_link(body: &str, title: &str) -> String {
    let link = format!("[[{title}]]");
    if body.to_lowercase().contains(&link.to_lowercase()) {
        body.to_string()
    } else if body.trim().is_empty() {
        link
    } else {
        format!("{body}\n\n{link}")
    }
}

fn remove_wiki_links(body: &str) -> String {
    let mut out = String::with_capacity(body.len());
    let mut rest = body;
    while let Some(start) = rest.find("[[") {
        out.push_str(&rest[..start]);
        rest = &rest[start + 2..];
        let Some(end) = rest.find("]]") else {
            out.push_str("[[");
            break;
        };
        rest = &rest[end + 2..];
    }
    out.push_str(rest);
    out.trim_end().to_string()
}

fn lowercase_with_map(body: &str) -> (String, Vec<usize>) {
    let mut lowered = String::with_capacity(body.len());
    let mut map = Vec::with_capacity(body.len() + 1);
    for (original_offset, ch) in body.char_indices() {
        for lower_ch in ch.to_lowercase() {
            let start = lowered.len();
            let mut buf = [0; 4];
            lowered.push_str(lower_ch.encode_utf8(&mut buf));
            map.extend(std::iter::repeat_n(original_offset, lowered.len() - start));
        }
    }
    map.push(body.len());
    (lowered, map)
}

fn find_matches(body: &str, query: &str) -> Vec<usize> {
    if query.trim().is_empty() { return Vec::new(); }
    let (hay, map) = lowercase_with_map(body);
    let needle = query.to_lowercase();
    hay.match_indices(&needle)
        .map(|(lower_idx, _)| map.get(lower_idx).copied().unwrap_or(lower_idx))
        .collect()
}

fn set_find_selection(ui: &MainWindow, offset: usize, length: usize) {
    ui.set_find_selection_offset(offset as i32);
    ui.set_find_selection_length(length as i32);
    ui.set_find_selection_token(ui.get_find_selection_token() + 1);
}

fn refresh_vault(state: &Rc<RefCell<AppState>>, ui: &MainWindow) {
    let (first_id, selected_id) = {
        let s = state.borrow();
        let key = s.vault_key.clone();
        let list = notes_store::list_notes_with_key(&s.db, &s.list_filter(), key.as_ref()).unwrap_or_default();
        let prefs = s.prefs.clone();
        let vault_filter = ui.get_vault_filter().to_string().trim().to_lowercase();
        let data: Vec<NoteData> = list
            .iter()
            .filter(|n| vault_filter.is_empty() || n.title.to_lowercase().contains(&vault_filter) || n.preview.to_lowercase().contains(&vault_filter))
            .map(|n| note_to_data(n, &prefs))
            .collect();
        let first = data.first().and_then(|n| Uuid::parse_str(&n.id).ok());
        ui.set_vault_notes(ModelRc::new(VecModel::from(data)));
        let parsed = Uuid::parse_str(&ui.get_vault_selected_id()).ok();
        (first, parsed)
    };

    if selected_id.is_none() {
        if let Some(id) = first_id {
            apply_vault_selection(state, ui, id);
        } else {
            ui.set_vault_has_note(false);
            ui.set_vault_selected_id(SharedString::new());
        }
    }
}

fn apply_vault_selection(state: &Rc<RefCell<AppState>>, ui: &MainWindow, id: Uuid) {
    flush_note(state, ui);
    let note = {
        let s = state.borrow();
        get_from_state(&s, id)
    };
    let Some(note) = note else { return };
    {
        let mut s = state.borrow_mut();
        s.current_id = Some(id);
        s.graph_selected = Some(id);
    }
    ui.set_vault_selected_id(id.to_string().into());
    ui.set_vault_has_note(true);
    ui.set_vault_title(note.title.clone().into());
    ui.set_vault_meta(shared(format!("Created {} · Last edited {}", note.created_at.with_timezone(&Local).format("%b %d, %Y · %I:%M %p"), note.updated_at.with_timezone(&Local).format("%b %d, %Y · %I:%M %p"))));
    ui.set_vault_source(note.body.clone().into());
    ui.set_vault_markdown(styled_markdown(&note.body));
    ui.set_vault_pinned(note.pinned);
    let links: Vec<SharedString> = parse_wiki_links(&note.body).into_iter().map(shared).collect();
    ui.set_vault_links_out(ModelRc::new(VecModel::from(links)));
    let mut backlinks: Vec<SharedString> = Vec::new();
    let title_lc = note.title.trim().to_lowercase();
    {
        let s = state.borrow();
        for other_id in list_all_ids(&s) {
            if other_id == id { continue; }
            let Some(other) = get_from_state(&s, other_id) else { continue };
            if parse_wiki_links(&other.body).iter().any(|l| l.trim().eq_ignore_ascii_case(&title_lc)) {
                backlinks.push(other.title.clone().into());
            }
        }
    }
    ui.set_vault_backlinks(ModelRc::new(VecModel::from(backlinks)));
    load_note_images(state, ui, id);
}

fn create_from_state(s: &AppState, note: &ycore::model::Note) -> rusqlite::Result<()> {
    let mut note = note.clone();
    note.encrypted = s.vault_cfg.enabled;
    notes_store::create_note_with_key(&s.db, &note, s.vault_key.as_ref())?;
    record_activity(s, note.id, "Created");
    Ok(())
}

fn list_all_ids(s: &AppState) -> Vec<Uuid> {
    s.db.prepare("SELECT id FROM notes").and_then(|mut stmt| {
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        Ok(rows.filter_map(|r| r.ok()).filter_map(|v| Uuid::parse_str(&v).ok()).collect())
    }).unwrap_or_default()
}

fn reencrypt_all(state: &AppState, old_key: Option<&VaultKey>, new_key: &VaultKey) -> anyhow::Result<()> {
    for id in list_all_ids(state) {
        let note = notes_store::get_note_with_key(&state.db, id, old_key)
            .map_err(|e| anyhow::anyhow!("load note {id}: {e}"))?
            .ok_or_else(|| anyhow::anyhow!("note {id} is not readable with the current vault key"))?;
        let mut note = note;
        note.encrypted = true;
        notes_store::update_note_with_key(&state.db, &note, Some(new_key))
            .map_err(|e| anyhow::anyhow!("re-encrypt note {id}: {e}"))?;
        for meta in notes_store::list_images(&state.db, id, old_key).unwrap_or_default() {
            if let Some(image) = notes_store::get_image(&state.db, meta.id, old_key)? {
                notes_store::update_image(&state.db, &image, Some(new_key))
                    .map_err(|e| anyhow::anyhow!("re-encrypt image {}: {e}", image.meta.id))?;
            }
        }
    }
    Ok(())
}

fn enable_vault(state: &mut AppState, password: &str) -> anyhow::Result<()> {
    if password.len() < 8 {
        anyhow::bail!("Password must be at least 8 characters");
    }
    let (key, cfg) = crypto::new_vault(password);
    reencrypt_all(state, None, &key)?;
    state.vault_cfg = cfg;
    state.vault_key = Some(key);
    state.vault_mismatch = false;
    Ok(())
}

fn change_vault_password(state: &mut AppState, current: &str, new_password: &str, require_current: bool) -> anyhow::Result<()> {
    if new_password.len() < 8 {
        anyhow::bail!("New password must be at least 8 characters");
    }
    let old_key = if require_current {
        Some(crypto::unlock(current, &state.vault_cfg)?)
    } else {
        state.vault_key.clone().ok_or_else(|| anyhow::anyhow!("vault is locked"))?;
        None
    };
    let source_key = old_key.as_ref().or(state.vault_key.as_ref()).unwrap();
    let (new_key, new_cfg) = crypto::new_vault(new_password);
    reencrypt_all(state, Some(source_key), &new_key)?;
    state.vault_cfg = new_cfg;
    state.vault_key = Some(new_key);
    state.vault_mismatch = false;
    Ok(())
}

fn export_note_markdown(state: &AppState) -> anyhow::Result<()> {
    let Some(id) = state.current_id else { return Ok(()) };
    let Some(note) = get_from_state(state, id) else { anyhow::bail!("note not found") };
    let filename = safe_filename(&note.title, "note") + ".md";
    let Some(path) = rfd::FileDialog::new().add_filter("Markdown", &["md"]).set_file_name(&filename).save_file() else { return Ok(()) };
    std::fs::write(path, format!("# {}\n\n{}\n", note.title, note.body))?;
    Ok(())
}

fn export_all_markdown(state: &AppState) -> anyhow::Result<()> {
    let ids = list_all_ids(state);
    let mut parts = Vec::new();
    for id in ids {
        let Some(note) = get_from_state(state, id) else { continue };
        if note.trashed { continue; }
        parts.push(format!("# {}\n\n{}\n", if note.title.trim().is_empty() { "Untitled" } else { &note.title }, note.body));
    }
    if parts.is_empty() {
        anyhow::bail!("there are no notes to export");
    }
    let content = format!("<!-- Exported from Notes · {} notes -->\n\n{}", parts.len(), parts.join("\n---\n\n"));
    let Some(path) = rfd::FileDialog::new().add_filter("Markdown", &["md"]).set_file_name("notes-vault.md").save_file() else { return Ok(()) };
    std::fs::write(path, content)?;
    Ok(())
}

fn import_markdown_files(state: &mut AppState) -> anyhow::Result<usize> {
    let Some(paths) = rfd::FileDialog::new()
        .add_filter("Markdown", &["md", "markdown", "txt"])
        .set_title("Import markdown notes")
        .pick_files() else { return Ok(0) };
    let mut imported = 0usize;
    for path in paths {
        let Ok(text) = std::fs::read_to_string(&path) else { continue };
        let mut note_lines = text.lines();
        let raw_first = note_lines.by_ref().find(|l| !l.trim().is_empty()).unwrap_or("").trim();
        let heading = raw_first.strip_prefix('#').map(str::trim).filter(|h| !h.is_empty());
        let fallback = path.file_stem().and_then(|s| s.to_str()).unwrap_or("Imported note").to_string();
        let (title, body) = if let Some(h) = heading {
            let mut body_lines = note_lines.collect::<Vec<_>>();
            while body_lines.first().map(|l| l.trim().is_empty()).unwrap_or(false) { body_lines.remove(0); }
            (h.to_string(), body_lines.join("\n"))
        } else {
            (fallback, text)
        };
        let mut note = ycore::model::Note::new(title, body);
        note.encrypted = state.vault_cfg.enabled;
        notes_store::create_note_with_key(&state.db, &note, state.vault_key.as_ref())?;
        imported += 1;
    }
    Ok(imported)
}

fn recent_note_titles(state: &AppState, prefix: &str) -> Vec<String> {
    let key = state.vault_key.clone();
    let p = prefix.to_lowercase();
    notes_store::list_notes_with_key(&state.db, &state.list_filter(), key.as_ref())
        .unwrap_or_default()
        .into_iter()
        .filter(|n| p.is_empty() || n.title.to_lowercase().starts_with(&p) || n.title.to_lowercase().contains(&p))
        .take(5)
        .map(|n| n.title)
        .collect()
}

fn trailing_wiki_prefix(body: &str) -> Option<String> {
    let start = body.rfind("[[")?;
    let rest = &body[start + 2..];
    if rest.contains(']') || rest.contains('\n') { return None; }
    Some(rest.to_string())
}

fn replace_trailing_wiki(body: &str, title: &str) -> String {
    if let Some(start) = body.rfind("[[") {
        let mut out = body[..start].to_string();
        out.push_str("[[");
        out.push_str(title);
        out.push_str("]]");
        out
    } else {
        format!("{body}[[{title}]]")
    }
}

fn suggestions_for_body(state: &AppState, body: &str) -> Vec<String> {
    let Some(prefix) = trailing_wiki_prefix(body) else { return Vec::new() };
    recent_note_titles(state, &prefix)
}

fn editor_body_changed(state: &Rc<RefCell<AppState>>, ui: &MainWindow) {
    let body: String = ui.get_editor_body().into();
    ui.set_word_count(word_count(&body));
    ui.set_editor_markdown(styled_markdown(&body));
    ui.set_checklist_items(ModelRc::new(VecModel::from(parse_checklist_items(&body))));
    let links: Vec<SharedString> = parse_wiki_links(&body).into_iter().map(shared).collect();
    ui.set_editor_links(ModelRc::new(VecModel::from(links)));
    let suggestions = suggestions_for_body(&state.borrow(), &body);
    ui.set_editor_suggestions(ModelRc::new(VecModel::from(suggestions.iter().cloned().map(SharedString::from).collect::<Vec<_>>())));
    ui.set_editor_suggestions_visible(!suggestions.is_empty());
    ui.set_save_state(1);
    let generation = {
        let mut s = state.borrow_mut();
        s.dirty_generation += 1;
        s.dirty_generation
    };
    let ui3 = ui.clone_strong();
    let st3 = state.clone();
    slint::Timer::single_shot(Duration::from_millis(350), move || {
        if st3.borrow().dirty_generation == generation {
            flush_note(&st3, &ui3);
            ui3.set_save_state(2);
            ui3.set_edited_label(shared(edited_label(chrono::Utc::now())));
            refresh_list(&st3, &ui3);
            refresh_graph(&st3, &ui3);
        }
    });
}

fn refresh_editor_after_body_edit(state: &Rc<RefCell<AppState>>, ui: &MainWindow) {
    editor_body_changed(state, ui);
    if ui.get_layout_style() == 2 {
        refresh_vault(state, ui);
    }
}

fn composer_body_changed(_state: &Rc<RefCell<AppState>>, ui: &MainWindow) {
    let body: String = ui.get_composer_body().into();
    ui.set_composer_preview_styled(styled_markdown(&body));
    ui.set_composer_checklist_items(ModelRc::new(VecModel::from(parse_checklist_items(&body))));
}

fn set_pending_images_ui(state: &AppState, ui: &MainWindow) {
    let mut rows: Vec<NoteImageData> = Vec::with_capacity(state.pending_note_images.len());
    for image in &state.pending_note_images {
        let thumb = decode_image_bytes(&image.bytes, Some(240)).unwrap_or_default();
        rows.push(NoteImageData {
            id: image.meta.id.to_string().into(),
            name: image.meta.name.clone().into(),
            size_label: format_bytes(image.meta.size).into(),
            thumbnail: thumb,
        });
    }
    ui.set_composer_pending_images(ModelRc::new(VecModel::from(rows)));
}

fn reset_composer(ui: &MainWindow) {
    ui.set_composer_open(false);
    ui.set_composer_title(SharedString::new());
    ui.set_composer_body(SharedString::new());
    ui.set_composer_pinned(false);
    ui.set_composer_color_index(0);
    ui.set_composer_checklist_mode(false);
    ui.set_composer_preview_mode(false);
    ui.set_composer_checklist_items(ModelRc::new(VecModel::from(Vec::<ChecklistItemData>::new())));
    ui.set_composer_preview_styled(slint::StyledText::default());
    ui.set_composer_pending_images(ModelRc::new(VecModel::from(Vec::<NoteImageData>::new())));
    ui.set_composer_color_popover_open(false);
}

fn create_note_from_composer(state: &mut AppState, ui: &MainWindow) -> Option<Uuid> {
    let title: String = ui.get_composer_title().into();
    let body: String = ui.get_composer_body().into();
    let pending = state.pending_note_images.len();
    if title.trim().is_empty() && body.trim().is_empty() && pending == 0 {
        return None;
    }
    let mut note = ycore::model::Note::new(title, body);
    note.pinned = ui.get_composer_pinned();
    note.color = color_hex(ui.get_composer_color_index());
    note.encrypted = state.vault_cfg.enabled;
    let id = note.id;
    if create_from_state(state, &note).is_err() { return None; }
    finish_pending_images(state, id);
    Some(id)
}

fn lock_vault(state: &Rc<RefCell<AppState>>, ui: &MainWindow) {
    flush_note(state, ui);
    {
        let mut s = state.borrow_mut();
        s.vault_key = None;
        s.current_id = None;
        s.graph_selected = None;
    }
    ui.set_vault_unlocked(false);
    ui.set_vault_password(SharedString::new());
    ui.set_vault_confirm(SharedString::new());
    ui.set_vault_error(SharedString::new());
    ui.set_has_note(false);
    ui.set_selected_id(SharedString::new());
    ui.set_editor_title(SharedString::new());
    ui.set_editor_body(SharedString::new());
    ui.set_editor_tags(ModelRc::new(VecModel::from(Vec::<SharedString>::new())));
    ui.set_editor_links(ModelRc::new(VecModel::from(Vec::<SharedString>::new())));
    ui.set_checklist_items(ModelRc::new(VecModel::from(Vec::<ChecklistItemData>::new())));
    ui.set_current_images(ModelRc::new(VecModel::from(Vec::<NoteImageData>::new())));
    ui.set_current_image_count(0);
    refresh_list(state, ui);
    toast(ui, "Vault locked");
}

fn vault_stats(state: &AppState) -> String {
    let notes: i64 = state.db.query_row("SELECT COUNT(*) FROM notes", [], |r| r.get(0)).unwrap_or(0);
    let archived: i64 = state.db.query_row("SELECT COUNT(*) FROM notes WHERE archived=1 AND trashed=0", [], |r| r.get(0)).unwrap_or(0);
    let trashed: i64 = state.db.query_row("SELECT COUNT(*) FROM notes WHERE trashed=1", [], |r| r.get(0)).unwrap_or(0);
    let encrypted: i64 = state.db.query_row("SELECT COUNT(*) FROM notes WHERE encrypted=1", [], |r| r.get(0)).unwrap_or(0);
    let images: i64 = state.db.query_row("SELECT COUNT(*) FROM note_images", [], |r| r.get(0)).unwrap_or(0);
    let bytes: i64 = state.db.query_row("SELECT COALESCE(SUM(size), 0) FROM note_images", [], |r| r.get(0)).unwrap_or(0);
    format!("{notes} notes · {archived} archived · {trashed} in trash\n{encrypted} encrypted · {images} attachments · {}", format_bytes(bytes))
}

fn set_settings_stats(state: &Rc<RefCell<AppState>>, ui: &MainWindow) {
    let s = state.borrow();
    ui.set_settings_stats(shared(vault_stats(&s)));
}

fn unlock_and_seed(state: &mut AppState) -> anyhow::Result<()> {
    if state.seeded {
        return Ok(());
    }
    let count: i64 = state.db.query_row("SELECT COUNT(*) FROM notes", [], |r| r.get(0)).unwrap_or(0);
    if count == 0 {
        let welcome = ycore::model::Note::new(
            "Welcome to Notes",
            "# Welcome to Notes\n\nEverything stays on this device, encrypted when your vault is enabled.\n\n- **Ctrl+N** — new note\n- **Ctrl+F** — search / find\n- **[[Note title]]** — link notes into the graph\n- Tap the pen to draw, the image icon to attach a picture\n\nHappy writing.",
        );
        let mut ideas = ycore::model::Note::new("Graph ideas", "Link this note to another with [[Welcome to Notes]] and open **Graph** from the rail.");
        ideas.color = Some("#1E3A5F".to_string());
        ideas.pinned = true;
        if state.vault_cfg.enabled {
            let mut welcome = welcome;
            welcome.encrypted = true;
            ideas.encrypted = true;
            create_from_state(state, &welcome)?;
            create_from_state(state, &ideas)?;
        } else {
            create_from_state(state, &welcome)?;
            create_from_state(state, &ideas)?;
        }
    }
    state.seeded = true;
    save_config(state);
    Ok(())
}

fn doodle_dimensions(ui: &MainWindow) -> (f32, f32) {
    let physical = ui.window().size();
    let scale = ui.window().scale_factor().max(0.5);
    let w = physical.width as f32 / scale;
    let h = physical.height as f32 / scale;
    ((w - 32.0).min(980.0) - 24.0, (h - 32.0).min(640.0) - 74.0)
}

fn refresh_doodle_ui(state: &AppState, ui: &MainWindow) {
    let (w, h) = doodle_dimensions(ui);
    let strokes: Vec<DoodleStrokeData> = state.doodle_strokes.iter().map(|stroke| {
        let mut commands = String::new();
        for (i, (x, y)) in stroke.points.iter().enumerate() {
            if i == 0 { commands.push_str(&format!("M {:.1} {:.1}", x * w, y * h)); }
            else { commands.push_str(&format!(" L {:.1} {:.1}", x * w, y * h)); }
        }
        let color_rgb = parse_hex_rgba(&stroke.color).unwrap_or([93, 216, 255, 255]);
        DoodleStrokeData {
            id: stroke.id,
            color: slint::Color::from_rgb_u8(color_rgb[0], color_rgb[1], color_rgb[2]),
            size: stroke.size,
            commands: commands.into(),
        }
    }).collect();
    ui.set_doodle_strokes(ModelRc::new(VecModel::from(strokes)));
}

fn read_image_file(path: &Path, note_id: Uuid) -> anyhow::Result<NoteImage> {
    if path.metadata()?.len() > 25 * 1024 * 1024 {
        anyhow::bail!("{} is larger than 25 MB", path.display());
    }
    let bytes = std::fs::read(path)?;
    let img = image::load_from_memory(&bytes).map_err(|_| anyhow::anyhow!("{} is not a readable image", path.display()))?;
    if img.width() > 12000 || img.height() > 12000 {
        anyhow::bail!("{} is too large to decode safely", path.display());
    }
    Ok(NoteImage {
        meta: NoteImageMeta {
            id: Uuid::new_v4(),
            note_id,
            name: path.file_name().and_then(|s| s.to_str()).unwrap_or("image").to_string(),
            mime: mime_for_path(path).to_string(),
            size: bytes.len() as i64,
            created_at: Utc::now(),
        },
        bytes,
    })
}

fn pick_image_files() -> Option<Vec<PathBuf>> {
    rfd::FileDialog::new()
        .add_filter("Images", &["png", "jpg", "jpeg", "gif", "webp", "bmp", "ico"])
        .set_title("Attach images")
        .pick_files()
}

fn finish_pending_images(state: &mut AppState, note_id: Uuid) -> usize {
    let pending = std::mem::take(&mut state.pending_note_images);
    let mut saved = 0;
    let key = state.vault_key.clone();
    for mut image in pending {
        image.meta.note_id = note_id;
        if notes_store::create_image(&state.db, &image, key.as_ref()).is_ok() {
            saved += 1;
        }
    }
    saved
}

fn toast(ui: &MainWindow, msg: &str) {
    ui.set_toast_message(shared(msg.to_string()));
    ui.set_toast_visible(true);
}

fn note_entry(s: &NoteSummary, prefs: &Prefs) -> (NoteData, f32) {
    let height = estimate_card_height_px(&s.title, &s.preview, prefs.clamp);
    (note_to_data(s, prefs), height)
}

fn distribute_columns(entries: Vec<(NoteData, f32)>, cols: usize) -> [Vec<NoteData>; 5] {
    let cols = cols.clamp(1, 5);
    let mut out: [Vec<NoteData>; 5] = Default::default();
    let mut heights = [0.0_f32; 5];
    for (note, height) in entries {
        let mut best = 0;
        for i in 1..cols {
            if heights[i] < heights[best] {
                best = i;
            }
        }
        heights[best] += height + 12.0; // keep in sync with board spacing
        out[best].push(note);
    }
    out
}

fn set_masonry_model(ui: &MainWindow, pinned: [Vec<NoteData>; 5], others: [Vec<NoteData>; 5]) {
    let [p0, p1, p2, p3, p4] = pinned;
    let [o0, o1, o2, o3, o4] = others;
    ui.set_pinned_col_0(ModelRc::new(VecModel::from(p0)));
    ui.set_pinned_col_1(ModelRc::new(VecModel::from(p1)));
    ui.set_pinned_col_2(ModelRc::new(VecModel::from(p2)));
    ui.set_pinned_col_3(ModelRc::new(VecModel::from(p3)));
    ui.set_pinned_col_4(ModelRc::new(VecModel::from(p4)));
    ui.set_notes_col_0(ModelRc::new(VecModel::from(o0)));
    ui.set_notes_col_1(ModelRc::new(VecModel::from(o1)));
    ui.set_notes_col_2(ModelRc::new(VecModel::from(o2)));
    ui.set_notes_col_3(ModelRc::new(VecModel::from(o3)));
    ui.set_notes_col_4(ModelRc::new(VecModel::from(o4)));
}

// ---------- list refresh ----------

fn refresh_list(state: &Rc<RefCell<AppState>>, ui: &MainWindow) {
    let s = state.borrow();
    let vault_locked = s.vault_cfg.enabled && s.vault_key.is_none();
    ui.set_vault_unlocked(!vault_locked);
    ui.set_vault_mismatch(s.vault_mismatch);
    if vault_locked {
        ui.set_pinned_section(false);
        ui.set_pinned_notes(ModelRc::new(VecModel::from(Vec::<NoteData>::new())));
        ui.set_notes(ModelRc::new(VecModel::from(Vec::<NoteData>::new())));
        ui.set_note_count(0);
        set_masonry_model(ui, Default::default(), Default::default());
        ui.set_folders(ModelRc::new(VecModel::from(Vec::<FolderData>::new())));
        ui.set_tags(ModelRc::new(VecModel::from(Vec::<TagData>::new())));
        return;
    }

    // pinned-only partition applies to the unfiltered All view
    let unfiltered = s.search.trim().is_empty()
        && s.active_folder.is_none()
        && s.active_tag.is_none()
        && (s.view == 0 || s.view == 1);

    let key = s.vault_key.clone();
    let filter = s.list_filter();
    let summaries = notes_store::list_notes_with_key(&s.db, &filter, key.as_ref()).unwrap_or_default();
    let prefs = s.prefs.clone();

    let mut pinned_e: Vec<(NoteData, f32)> = Vec::new();
    let mut others_e: Vec<(NoteData, f32)> = Vec::new();

    if s.view == 1 {
        others_e = summaries.iter().filter(|n| n.pinned).map(|n| note_entry(n, &prefs)).collect();
    } else if unfiltered && s.view == 0 {
        for n in &summaries {
            if n.pinned && prefs.pinned_first {
                pinned_e.push(note_entry(n, &prefs));
            } else {
                others_e.push(note_entry(n, &prefs));
            }
        }
    } else {
        others_e = summaries.iter().map(|n| note_entry(n, &prefs)).collect();
    }

    let cols = ui.get_grid_cols().clamp(1, 5) as usize;
    let pinned_cols = distribute_columns(pinned_e, cols);
    let others_cols = distribute_columns(others_e, cols);
    let pinned_v: Vec<NoteData> = pinned_cols.iter().flatten().cloned().collect();
    let others_v: Vec<NoteData> = others_cols.iter().flatten().cloned().collect();
    set_masonry_model(ui, pinned_cols, others_cols);

    let total = (pinned_v.len() + others_v.len()) as i32;
    let show_sections = s.view == 0 && !pinned_v.is_empty() && prefs.pinned_first;

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
    if let Ok(tags) = notes_store::all_tags_with_key(&s.db, key.as_ref()) {
        for (name, count) in tags {
            tag_rows.push(TagData {
                name: name.into(),
                meta: count.to_string().into(),
            });
        }
    }
    ui.set_tags(ModelRc::new(VecModel::from(tag_rows)));
}

fn parse_wiki_links(body: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut rest = body;
    while let Some(start) = rest.find("[[") {
        rest = &rest[start + 2..];
        let Some(end) = rest.find("]]") else { break };
        let title = rest[..end].trim().to_string();
        if !title.is_empty() && !out.iter().any(|t| t.eq_ignore_ascii_case(title.as_str())) {
            out.push(title);
        }
        rest = &rest[end + 2..];
    }
    out
}

fn load_note_images(state: &Rc<RefCell<AppState>>, ui: &MainWindow, note_id: Uuid) {
    let s = state.borrow();
    let key = s.vault_key.clone();
    let mut rows = notes_store::list_images(&s.db, note_id, key.as_ref()).unwrap_or_default();
    rows.sort_by_key(|m| m.created_at);
    let mut ui_rows: Vec<NoteImageData> = Vec::with_capacity(rows.len());
    for meta in &rows {
        let thumbnail = notes_store::get_image(&s.db, meta.id, key.as_ref())
            .ok()
            .flatten()
            .and_then(|img| decode_image_bytes(&img.bytes, Some(320)))
            .unwrap_or_default();
        ui_rows.push(NoteImageData {
            id: meta.id.to_string().into(),
            name: meta.name.clone().into(),
            size_label: format_bytes(meta.size).into(),
            thumbnail,
        });
    }
    ui.set_current_images(ModelRc::new(VecModel::from(ui_rows)));
    ui.set_current_image_count(rows.len() as i32);
    drop(s);
    state.borrow_mut().current_images = rows;
}

fn apply_to_editor(state: &Rc<RefCell<AppState>>, ui: &MainWindow, note: &ycore::model::Note) {
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
    ui.set_current_note_color_index(color_index(&note.color));
    ui.set_color_popover_open(false);
    ui.set_editor_markdown(styled_markdown(&note.body));
    ui.set_checklist_items(ModelRc::new(VecModel::from(parse_checklist_items(&note.body))));
    let links: Vec<SharedString> = parse_wiki_links(&note.body).into_iter().map(shared).collect();
    ui.set_editor_links(ModelRc::new(VecModel::from(links)));
    ui.set_timestamp_created(shared(note.created_at.with_timezone(&Local).format("%b %d, %Y · %I:%M %p").to_string()));
    ui.set_timestamp_edited(shared(note.updated_at.with_timezone(&Local).format("%b %d, %Y · %I:%M %p").to_string()));
    let activity: Vec<SharedString> = activity_strings(&state.borrow(), note.id).into_iter().map(shared).collect();
    ui.set_timestamp_activity(ModelRc::new(VecModel::from(activity)));
    load_note_images(state, ui, note.id);
}

fn select_note(state: &Rc<RefCell<AppState>>, ui: &MainWindow, id: Uuid) {
    flush_note(state, ui);
    let note = {
        let s = state.borrow();
        get_from_state(&s, id)
    };
    if let Some(note) = note {
        state.borrow_mut().current_id = Some(id);
        apply_to_editor(state, ui, &note);
    }
}

fn flush_note(state: &Rc<RefCell<AppState>>, ui: &MainWindow) {
    let maybe_id = state.borrow().current_id;
    let Some(id) = maybe_id else { return };
    let title: String = ui.get_editor_title().into();
    let body: String = ui.get_editor_body().into();
    let s = state.borrow();
    if let Some(mut note) = get_from_state(&s, id) {
        if note.title != title || note.body != body {
            note.title = title;
            note.body = body;
            let _ = update_from_state(&s, &note);
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
    let loaded = load_config(&config_path);
    let state = AppState {
        db: open_notes_db(&dir.join("notes.db"))?,
        config_path,
        dark: loaded.dark,
        seeded: loaded.seeded,
        current_id: None,
        view: 0,
        search: String::new(),
        active_folder: None,
        active_tag: None,
        dirty_generation: 0,
        vault_cfg: loaded.vault.clone(),
        vault_key: None,
        vault_mismatch: false,
        prefs: loaded.prefs.clone(),
        graph_positions: std::collections::HashMap::new(),
        graph_yaw: 0.7,
        graph_pitch: -0.35,
        graph_zoom: 1.0,
        graph_selected: None,
        graph_drag: None,
        graph_rotate: None,
        graph_press: None,
        graph_last_click: None,
        graph_screen_positions: Vec::new(),
        doodle_context: 0,
        doodle_strokes: Vec::new(),
        pending_note_images: Vec::new(),
        current_images: Vec::new(),
    };
    let _ = loaded.value; // retained for forward-compatible config fields

    // Theme
    ui.set_dark(state.dark);
    ui.set_neo(state.prefs.layout == "neo");
    ui.set_layout_style(state.prefs.layout_index());
    ui.set_density_setting(state.prefs.density_index());
    ui.set_mobile_two(state.prefs.mobile_two);
    ui.set_sort_setting(state.prefs.sort_index());
    ui.set_pinned_first(state.prefs.pinned_first);
    ui.set_md_cards(state.prefs.md_cards);
    ui.set_show_dates(state.prefs.show_dates);
    ui.set_card_clamp(state.prefs.clamp);
    ui.set_vault_enabled(state.vault_cfg.enabled);
    ui.set_vault_unlocked(!state.vault_cfg.enabled);
    ui.set_vault_mode(if state.vault_cfg.enabled { 1 } else { 0 });

    // Seed happens after vault state is known (before unlock for plaintext vaults).
    let state = Rc::new(RefCell::new(state));
    if !state.borrow().vault_cfg.enabled {
        let _ = unlock_and_seed(&mut state.borrow_mut());
    }

    // Initial refresh + preselect latest note (desktop, unlocked only)
    refresh_list(&state, &ui);
    let preselect_last = std::env::var("YELLOD_NOTES_OPEN_LAST").map(|v| v != "0").unwrap_or(true);
    if !state.borrow().vault_cfg.enabled && preselect_last {
        let s = state.borrow();
        let first = notes_store::list_notes_with_key(&s.db, &notes_store::ListFilter::default(), s.vault_key.as_ref());
        if let Ok(first) = first {
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
            if st.borrow().vault_cfg.enabled && st.borrow().vault_key.is_none() {
                return;
            }
            flush_note(&st, &ui2);
            let mut note = ycore::model::Note::new("", "");
            note.encrypted = st.borrow().vault_cfg.enabled;
            let id = note.id;
            {
                let s = st.borrow();
                let _ = create_from_state(&s, &note);
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
                if let Some(mut note) = get_from_state(&s, nid) {
                    note.pinned = !note.pinned;
                    let kind = if note.pinned { "Pinned" } else { "Unpinned" };
                    let _ = update_from_state(&s, &note);
                    record_activity(&s, nid, kind);
                }
                drop(s);
                if st.borrow().current_id == Some(nid) {
                    ui2.set_editor_pinned({
                        let s = st.borrow();
                        get_from_state(&s, nid).map(|n| n.pinned).unwrap_or(false)
                    });
                }
                refresh_list(&st, &ui2);
            }
        });
    }
    {
        let ui2 = ui.clone_strong();
        let st = state.clone();
        ui.on_manual_note_moved(move |id, direction| {
            if let Ok(nid) = Uuid::parse_str(&id) {
                move_note_in_order(&st, &ui2, nid, direction);
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
                if let Some(mut note) = get_from_state(&s, id) {
                    note.pinned = !note.pinned;
                    let new_state = note.pinned;
                    let _ = update_from_state(&s, &note);
                    record_activity(&s, id, if new_state { "Pinned" } else { "Unpinned" });
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
                if let Some(mut note) = get_from_state(&s, id) {
                    note.favorite = !note.favorite;
                    let new_state = note.favorite;
                    let _ = update_from_state(&s, &note);
                    record_activity(&s, id, if new_state { "Favorited" } else { "Unfavorited" });
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
        ui.on_color_picked(move |idx: i32| {
            let id = st.borrow().current_id;
            if let Some(id) = id {
                {
                    let s = st.borrow();
                    if let Some(mut note) = get_from_state(&s, id) {
                        note.color = color_hex(idx);
                        let _ = update_from_state(&s, &note);
                    }
                }
                ui2.set_current_note_color_index(idx);
                ui2.set_color_popover_open(false);
                refresh_list(&st, &ui2);
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
                    get_from_state(&s, id).map(|n| n.archived).unwrap_or(false)
                };
                let s = st.borrow();
                if let Some(mut note) = get_from_state(&s, id) {
                    note.archived = !note.archived;
                    let _ = update_from_state(&s, &note);
                    record_activity(&s, id, if note.archived { "Archived" } else { "Unarchived" });
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
                if let Some(mut note) = get_from_state(&s, id) {
                    note.trashed = true;
                    let _ = update_from_state(&s, &note);
                    record_activity(&s, id, "Moved to trash");
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
                if let Some(mut note) = get_from_state(&s, id) {
                    note.trashed = false;
                    let _ = update_from_state(&s, &note);
                    record_activity(&s, id, "Restored");
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
            if let Ok(notes) = notes_store::list_notes_with_key(&s.db, &notes_store::ListFilter {
                trashed: true,
                ..Default::default()
            }, s.vault_key.as_ref()) {
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
                if let Some(note) = get_from_state(&s, id) {
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
            editor_body_changed(&st, &ui2);
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
                if let Some(mut note) = get_from_state(&s, id) {
                    if !note.tags.iter().any(|t| t.to_lowercase() == tag_l) {
                        note.tags.push(tag_l);
                        let _ = update_from_state(&s, &note);
                        drop(s);
                        // refresh editor tag chips
                        let s = st.borrow();
                        if let Some(note) = get_from_state(&s, id) {
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
                if let Some(mut note) = get_from_state(&s, id) {
                    note.tags.retain(|t| t != &tag);
                    let _ = update_from_state(&s, &note);
                    drop(s);
                    let s = st.borrow();
                    if let Some(note) = get_from_state(&s, id) {
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
                if let Some(mut note) = get_from_state(&s, nid) {
                    note.folder_id = fid;
                    let _ = update_from_state(&s, &note);
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
        let st = state.clone();
        ui.on_refresh_masonry(move || {
            refresh_list(&st, &ui2);
        });
    }

    // ---------- layout / vault / settings ----------
    {
        let ui2 = ui.clone_strong();
        let st = state.clone();
        ui.on_layout_selected(move |idx: i32| {
            let layout = match idx { 1 => "notion", 2 => "obsidian", 3 => "neo", 4 => "graph", _ => "keep" };
            {
                let mut s = st.borrow_mut();
                s.prefs.layout = layout.to_string();
            }
            ui2.set_layout_style(idx);
            ui2.set_neo(idx == 3);
            save_config(&st.borrow());
            refresh_list(&st, &ui2);
            if idx == 2 { refresh_vault(&st, &ui2); }
            if idx == 4 { refresh_graph(&st, &ui2); }
        });
    }

    {
        let ui2 = ui.clone_strong();
        let st = state.clone();
        ui.on_vault_unlock(move || {
            let password: String = ui2.get_vault_password().into();
            let result = {
                let s = st.borrow();
                crypto::unlock(&password, &s.vault_cfg)
            };
            match result {
                Ok(key) => {
                    {
                        let mut s = st.borrow_mut();
                        s.vault_key = Some(key);
                        s.vault_mismatch = false;
                    }
                    ui2.set_vault_unlocked(true);
                    ui2.set_vault_password(SharedString::new());
                    ui2.set_vault_error(SharedString::new());
                    {
                        let mut s = st.borrow_mut();
                        let _ = unlock_and_seed(&mut s);
                    }
                    refresh_list(&st, &ui2);
                    set_settings_stats(&st, &ui2);
                    let first = {
                        let s = st.borrow();
                        notes_store::list_notes_with_key(&s.db, &s.list_filter(), s.vault_key.as_ref()).ok().and_then(|v| v.first().map(|n| n.id))
                    };
                    if let Some(id) = first { select_note(&st, &ui2, id); }
                    if ui2.get_layout_style() == 2 { refresh_vault(&st, &ui2); }
                    if ui2.get_layout_style() == 4 { refresh_graph(&st, &ui2); }
                    toast(&ui2, "Vault unlocked");
                }
                Err(_) => {
                    ui2.set_vault_error(shared("Wrong password. Your notes remain encrypted.".to_string()));
                }
            }
        });
    }

    {
        let ui2 = ui.clone_strong();
        ui.on_vault_create_from_unlock(move || {
            ui2.set_vault_error(shared("Create the vault from Settings → Security.".to_string()));
        });
    }

    {
        let ui2 = ui.clone_strong();
        ui.on_vault_forgot(move || {
            ui2.set_vault_error(shared("There is no server-side recovery. Reset the password from an unlocked settings session, or the encrypted data cannot be opened.".to_string()));
        });
    }

    {
        let ui2 = ui.clone_strong();
        let st = state.clone();
        ui.on_lock_now(move || lock_vault(&st, &ui2));
    }

    {
        let ui2 = ui.clone_strong();
        let st = state.clone();
        ui.on_create_vault(move |password, confirm| {
            if password != confirm {
                ui2.set_settings_security_error(shared("Passwords do not match".to_string()));
                return;
            }
            let result = {
                let mut s = st.borrow_mut();
                enable_vault(&mut s, &password)
            };
            match result {
                Ok(()) => {
                    ui2.set_vault_enabled(true);
                    ui2.set_settings_security_error(SharedString::new());
                    {
                        let mut s = st.borrow_mut();
                        let _ = unlock_and_seed(&mut s);
                    }
                    set_settings_stats(&st, &ui2);
                    refresh_list(&st, &ui2);
                    if ui2.get_layout_style() == 4 { refresh_graph(&st, &ui2); }
                    toast(&ui2, "Vault created and notes encrypted");
                }
                Err(e) => ui2.set_settings_security_error(shared(e.to_string())),
            }
        });
    }

    {
        let ui2 = ui.clone_strong();
        let st = state.clone();
        ui.on_change_password(move |current, new_password| {
            let result = {
                let mut s = st.borrow_mut();
                change_vault_password(&mut s, &current, &new_password, true)
            };
            match result {
                Ok(()) => {
                    ui2.set_settings_security_error(SharedString::new());
                    set_settings_stats(&st, &ui2);
                    refresh_list(&st, &ui2);
                    toast(&ui2, "Vault password changed");
                }
                Err(e) => ui2.set_settings_security_error(shared(e.to_string())),
            }
        });
    }

    {
        let ui2 = ui.clone_strong();
        let st = state.clone();
        ui.on_reset_password(move |new_password, confirm, acknowledged| {
            if !acknowledged {
                ui2.set_settings_security_error(shared("Confirm that other devices will lose access.".to_string()));
                return;
            }
            if new_password != confirm {
                ui2.set_settings_security_error(shared("Passwords do not match".to_string()));
                return;
            }
            let result = {
                let mut s = st.borrow_mut();
                change_vault_password(&mut s, "", &new_password, false)
            };
            match result {
                Ok(()) => {
                    ui2.set_settings_security_error(SharedString::new());
                    set_settings_stats(&st, &ui2);
                    refresh_list(&st, &ui2);
                    toast(&ui2, "Vault password reset");
                }
                Err(e) => ui2.set_settings_security_error(shared(e.to_string())),
            }
        });
    }

    // ---------- composer ----------
    {
        let ui2 = ui.clone_strong();
        ui.on_composer_open_requested(move || ui2.set_composer_open(true));
    }
    {
        let ui2 = ui.clone_strong();
        let st = state.clone();
        ui.on_composer_close_requested(move || {
            let id = {
                let mut s = st.borrow_mut();
                create_note_from_composer(&mut s, &ui2)
            };
            st.borrow_mut().pending_note_images.clear();
            reset_composer(&ui2);
            refresh_list(&st, &ui2);
            if st.borrow().prefs.layout == "graph" { refresh_graph(&st, &ui2); }
            if let Some(id) = id { select_note(&st, &ui2, id); }
            set_settings_stats(&st, &ui2);
        });
    }
    {
        let ui2 = ui.clone_strong();
        let st = state.clone();
        ui.on_composer_content_changed(move || composer_body_changed(&st, &ui2));
    }
    {
        let ui2 = ui.clone_strong();
        ui.on_composer_toggle_pin(move || ui2.set_composer_pinned(!ui2.get_composer_pinned()));
    }
    {
        let ui2 = ui.clone_strong();
        ui.on_composer_color_picked(move |idx| {
            ui2.set_composer_color_index(idx);
            ui2.set_composer_color_popover_open(false);
        });
    }
    {
        let ui2 = ui.clone_strong();
        let st = state.clone();
        ui.on_composer_image_requested(move || {
            let Some(paths) = pick_image_files() else { return };
            let mut added = 0;
            let mut failed = 0;
            for path in paths {
                match read_image_file(&path, Uuid::nil()) {
                    Ok(image) => { st.borrow_mut().pending_note_images.push(image); added += 1; }
                    Err(_) => failed += 1,
                }
            }
            set_pending_images_ui(&st.borrow(), &ui2);
            ui2.set_composer_open(true);
            toast(&ui2, &format!("Attached {added} image{}{}", if added == 1 { "" } else { "s" }, if failed > 0 { " (some skipped)" } else { "" }));
        });
    }
    {
        let ui2 = ui.clone_strong();
        let st = state.clone();
        ui.on_composer_draw_requested(move || {
            {
                let mut s = st.borrow_mut();
                s.doodle_context = 1;
                s.doodle_strokes.clear();
            }
            refresh_doodle_ui(&st.borrow(), &ui2);
            ui2.set_doodle_open(true);
        });
    }
    {
        let ui2 = ui.clone_strong();
        let st = state.clone();
        ui.on_composer_checklist_toggle(move || {
            let next = !ui2.get_composer_checklist_mode();
            ui2.set_composer_checklist_mode(next);
            ui2.set_composer_preview_mode(false);
            if next { composer_body_changed(&st, &ui2); }
        });
    }
    {
        let ui2 = ui.clone_strong();
        ui.on_composer_preview_toggle(move || {
            let next = !ui2.get_composer_preview_mode();
            ui2.set_composer_preview_mode(next);
            ui2.set_composer_checklist_mode(false);
            if next {
                let body: String = ui2.get_composer_body().into();
                ui2.set_composer_preview_styled(styled_markdown(&body));
            }
        });
    }
    {
        let ui2 = ui.clone_strong();
        let st = state.clone();
        ui.on_composer_checklist_add(move |text| {
            let text = text.trim().to_string();
            if text.is_empty() { return; }
            let mut body: String = ui2.get_composer_body().into();
            if !body.is_empty() && !body.ends_with('\n') { body.push('\n'); }
            body.push_str("- [ ] ");
            body.push_str(&text);
            ui2.set_composer_body(body.into());
            composer_body_changed(&st, &ui2);
        });
    }
    {
        let ui2 = ui.clone_strong();
        let st = state.clone();
        ui.on_composer_checklist_toggle_item(move |line| {
            let body: String = ui2.get_composer_body().into();
            ui2.set_composer_body(toggle_checklist_body(&body, line as usize).into());
            composer_body_changed(&st, &ui2);
        });
    }
    {
        let ui2 = ui.clone_strong();
        let st = state.clone();
        ui.on_composer_checklist_edit_item(move |line, text| {
            let body: String = ui2.get_composer_body().into();
            let line_idx = line as usize;
            let original = body.lines().nth(line_idx).unwrap_or("");
            let prefix = if original.contains("[x]") || original.contains("[X]") { "- [x] " } else { "- [ ] " };
            let updated = format!("{prefix}{}", text.trim());
            ui2.set_composer_body(replace_line(&body, line_idx, updated).into());
            composer_body_changed(&st, &ui2);
        });
    }
    {
        let ui2 = ui.clone_strong();
        let st = state.clone();
        ui.on_composer_checklist_remove_item(move |line| {
            let body: String = ui2.get_composer_body().into();
            ui2.set_composer_body(delete_line_body(&body, line as usize).into());
            composer_body_changed(&st, &ui2);
        });
    }
    {
        let ui2 = ui.clone_strong();
        let st = state.clone();
        ui.on_composer_remove_pending_image(move |id| {
            if let Ok(nid) = Uuid::parse_str(&id) {
                st.borrow_mut().pending_note_images.retain(|i| i.meta.id != nid);
                set_pending_images_ui(&st.borrow(), &ui2);
            }
        });
    }

    // ---------- images / checklist ----------
    {
        let ui2 = ui.clone_strong();
        let st = state.clone();
        ui.on_attach_images(move || {
            if st.borrow().vault_cfg.enabled && st.borrow().vault_key.is_none() { return; }
            let id = st.borrow().current_id.unwrap_or_else(|| {
                let mut note = ycore::model::Note::new("", "");
                note.encrypted = st.borrow().vault_cfg.enabled;
                let id = note.id;
                let s = st.borrow();
                let _ = create_from_state(&s, &note);
                id
            });
            let Some(paths) = pick_image_files() else { return };
            let mut added = 0;
            let mut failed = 0;
            for path in paths {
                match read_image_file(&path, id) {
                    Ok(image) => {
                        let key = st.borrow().vault_key.clone();
                        if notes_store::create_image(&st.borrow().db, &image, key.as_ref()).is_ok() { added += 1; } else { failed += 1; }
                    }
                    Err(_) => failed += 1,
                }
            }
            load_note_images(&st, &ui2, id);
            refresh_list(&st, &ui2);
            toast(&ui2, &format!("Attached {added} image{}{}", if added == 1 { "" } else { "s" }, if failed > 0 { " (some skipped)" } else { "" }));
        });
    }
    {
        let ui2 = ui.clone_strong();
        let st = state.clone();
        ui.on_draw_editor(move || {
            {
                let mut s = st.borrow_mut();
                s.doodle_context = 0;
                s.doodle_strokes.clear();
            }
            refresh_doodle_ui(&st.borrow(), &ui2);
            ui2.set_doodle_open(true);
        });
    }
    {
        let ui2 = ui.clone_strong();
        let st = state.clone();
        ui.on_image_preview(move |id| {
            if let Ok(nid) = Uuid::parse_str(&id) {
                let s = st.borrow();
                if let Ok(Some(image)) = notes_store::get_image(&s.db, nid, s.vault_key.as_ref()) {
                    if let Some(img) = decode_image_bytes(&image.bytes, None) {
                        ui2.set_image_preview_image(img);
                        ui2.set_image_preview_id(id);
                        ui2.set_image_preview_name(image.meta.name.into());
                        ui2.set_image_preview_size(format_bytes(image.meta.size).into());
                        ui2.set_image_preview_open(true);
                    }
                }
            }
        });
    }
    {
        let ui2 = ui.clone_strong();
        let st = state.clone();
        ui.on_image_delete(move |id| {
            if let Ok(nid) = Uuid::parse_str(&id) {
                let s = st.borrow();
                let _ = notes_store::delete_image(&s.db, nid);
                if let Some(note_id) = s.current_id {
                    drop(s);
                    load_note_images(&st, &ui2, note_id);
                }
                refresh_list(&st, &ui2);
                toast(&ui2, "Image deleted");
            }
        });
    }
    {
        let ui2 = ui.clone_strong();
        let st = state.clone();
        ui.on_image_download_preview(move || {
            let id = ui2.get_image_preview_id().to_string();
            let Ok(nid) = Uuid::parse_str(&id) else { return };
            let s = st.borrow();
            let Some(image) = notes_store::get_image(&s.db, nid, s.vault_key.as_ref()).ok().flatten() else { return };
            let Some(path) = rfd::FileDialog::new().set_file_name(&image.meta.name).save_file() else { return };
            let _ = std::fs::write(path, image.bytes);
        });
    }
    {
        let ui2 = ui.clone_strong();
        ui.on_close_image_preview(move || {
            ui2.set_image_preview_open(false);
            ui2.set_image_preview_id(SharedString::new());
        });
    }
    {
        let ui2 = ui.clone_strong();
        ui.on_checklist_toggle(move || ui2.set_editor_checklist_mode(!ui2.get_editor_checklist_mode()));
    }
    {
        let ui2 = ui.clone_strong();
        let st = state.clone();
        ui.on_checklist_add(move |text| {
            let text = text.trim().to_string();
            if text.is_empty() { return; }
            let mut body: String = ui2.get_editor_body().into();
            if !body.is_empty() && !body.ends_with('\n') { body.push('\n'); }
            body.push_str("- [ ] ");
            body.push_str(&text);
            ui2.set_editor_body(body.into());
            refresh_editor_after_body_edit(&st, &ui2);
        });
    }
    {
        let ui2 = ui.clone_strong();
        let st = state.clone();
        ui.on_checklist_toggle_item(move |line| {
            let body: String = ui2.get_editor_body().into();
            ui2.set_editor_body(toggle_checklist_body(&body, line as usize).into());
            refresh_editor_after_body_edit(&st, &ui2);
        });
    }
    {
        let ui2 = ui.clone_strong();
        let st = state.clone();
        ui.on_checklist_edit_item(move |line, text| {
            let body: String = ui2.get_editor_body().into();
            let line_idx = line as usize;
            let original = body.lines().nth(line_idx).unwrap_or("");
            let prefix = if original.contains("[x]") || original.contains("[X]") { "- [x] " } else { "- [ ] " };
            ui2.set_editor_body(replace_line(&body, line_idx, format!("{prefix}{}", text.trim())).into());
            refresh_editor_after_body_edit(&st, &ui2);
        });
    }
    {
        let ui2 = ui.clone_strong();
        let st = state.clone();
        ui.on_checklist_remove_item(move |line| {
            let body: String = ui2.get_editor_body().into();
            ui2.set_editor_body(delete_line_body(&body, line as usize).into());
            refresh_editor_after_body_edit(&st, &ui2);
        });
    }

    // ---------- links / find ----------
    {
        let ui2 = ui.clone_strong();
        let st = state.clone();
        ui.on_link_clicked(move |title| {
            let s = st.borrow();
            let key = s.vault_key.clone();
            let title_lc = title.trim().to_lowercase();
            let found = notes_store::list_notes_with_key(&s.db, &s.list_filter(), key.as_ref())
                .unwrap_or_default()
                .into_iter()
                .find(|n| n.title.trim().eq_ignore_ascii_case(&title_lc))
                .map(|n| n.id);
            drop(s);
            if let Some(id) = found { select_note(&st, &ui2, id); } else { toast(&ui2, "Linked note not found"); }
        });
    }
    {
        let ui2 = ui.clone_strong();
        ui.on_find_changed(move || {
            let body: String = ui2.get_editor_body().into();
            let query: String = ui2.get_editor_find_query().into();
            let matches = find_matches(&body, &query);
            let count = matches.len() as i32;
            ui2.set_editor_find_count(count);
            ui2.set_editor_find_index(0);
            if let Some(offset) = matches.first().copied() {
                set_find_selection(&ui2, offset, query.len());
            }
        });
    }
    {
        let ui2 = ui.clone_strong();
        ui.on_find_next(move || {
            let body: String = ui2.get_editor_body().into();
            let query: String = ui2.get_editor_find_query().into();
            let matches = find_matches(&body, &query);
            let count = matches.len() as i32;
            if count > 0 {
                let idx = (ui2.get_editor_find_index() + 1).rem_euclid(count);
                ui2.set_editor_find_index(idx);
                if let Some(offset) = matches.get(idx as usize).copied() {
                    set_find_selection(&ui2, offset, query.len());
                }
            }
        });
    }
    {
        let ui2 = ui.clone_strong();
        ui.on_find_previous(move || {
            let body: String = ui2.get_editor_body().into();
            let query: String = ui2.get_editor_find_query().into();
            let matches = find_matches(&body, &query);
            let count = matches.len() as i32;
            if count > 0 {
                let idx = (ui2.get_editor_find_index() - 1).rem_euclid(count);
                ui2.set_editor_find_index(idx);
                if let Some(offset) = matches.get(idx as usize).copied() {
                    set_find_selection(&ui2, offset, query.len());
                }
            }
        });
    }
    {
        let ui2 = ui.clone_strong();
        ui.on_close_find(move || {
            ui2.set_editor_find_open(false);
            ui2.set_editor_find_query(SharedString::new());
            ui2.set_editor_find_count(0);
            ui2.set_editor_find_index(0);
        });
    }
    {
        let ui2 = ui.clone_strong();
        let st = state.clone();
        ui.on_suggestion_picked(move |title| {
            let body: String = ui2.get_editor_body().into();
            ui2.set_editor_body(replace_trailing_wiki(&body, &title).into());
            ui2.set_editor_suggestions_visible(false);
            refresh_editor_after_body_edit(&st, &ui2);
        });
    }

    // ---------- vault view ----------
    {
        let ui2 = ui.clone_strong();
        let st = state.clone();
        ui.on_vault_filter_changed(move |_| refresh_vault(&st, &ui2));
    }
    {
        let ui2 = ui.clone_strong();
        let st = state.clone();
        ui.on_vault_note_selected(move |id| {
            if let Ok(nid) = Uuid::parse_str(&id) { apply_vault_selection(&st, &ui2, nid); }
        });
    }
    {
        let ui2 = ui.clone_strong();
        let st = state.clone();
        ui.on_vault_new_note(move || {
            let mut note = ycore::model::Note::new("", "");
            note.encrypted = st.borrow().vault_cfg.enabled;
            let id = note.id;
            let s = st.borrow();
            let _ = create_from_state(&s, &note);
            drop(s);
            apply_vault_selection(&st, &ui2, id);
            refresh_list(&st, &ui2);
            refresh_graph(&st, &ui2);
        });
    }
    {
        let ui2 = ui.clone_strong();
        let st = state.clone();
        ui.on_vault_pin_toggled(move || {
            let id = st.borrow().current_id;
            let Some(id) = id else { return };
            let s = st.borrow();
            if let Some(mut note) = get_from_state(&s, id) {
                note.pinned = !note.pinned;
                let new_state = note.pinned;
                let _ = update_from_state(&s, &note);
                drop(s);
                ui2.set_vault_pinned(new_state);
                ui2.set_editor_pinned(new_state);
                refresh_list(&st, &ui2);
            }
        });
    }
    {
        let ui2 = ui.clone_strong();
        let st = state.clone();
        ui.on_vault_edit_source(move || {
            let next = !ui2.get_vault_source_mode();
            ui2.set_vault_source_mode(next);
            if next {
                if let Some(id) = st.borrow().current_id {
                    let s = st.borrow();
                    if let Some(note) = get_from_state(&s, id) { ui2.set_vault_source(note.body.clone().into()); }
                }
            }
        });
    }
    {
        let ui2 = ui.clone_strong();
        let st = state.clone();
        ui.on_vault_source_edited(move || {
            let body: String = ui2.get_vault_source().into();
            let Some(id) = st.borrow().current_id else { return };
            let s = st.borrow();
            if let Some(mut note) = get_from_state(&s, id) {
                note.body = body;
                let _ = update_from_state(&s, &note);
            }
            drop(s);
            ui2.set_vault_markdown(styled_markdown(ui2.get_vault_source().as_str()));
            refresh_list(&st, &ui2);
            refresh_graph(&st, &ui2);
        });
    }
    {
        let ui2 = ui.clone_strong();
        ui.on_vault_open(move || ui2.set_has_note(true));
    }
    {
        let ui2 = ui.clone_strong();
        let st = state.clone();
        ui.on_vault_title_edited(move |title| {
            let Some(id) = st.borrow().current_id else { return };
            let s = st.borrow();
            if let Some(mut note) = get_from_state(&s, id) {
                note.title = title.to_string();
                let _ = update_from_state(&s, &note);
            }
            drop(s);
            refresh_list(&st, &ui2);
            refresh_graph(&st, &ui2);
        });
    }

    // ---------- graph ----------
    {
        let st = state.clone();
        ui.on_graph_pointer_down(move |x, y, _alt| {
            let hit = graph_hit(&st, x, y);
            let mut s = st.borrow_mut();
            match hit {
                Some(id) => {
                    s.graph_drag = Some((id, x, y));
                    s.graph_press = Some((Some(id), x, y));
                }
                None => {
                    s.graph_rotate = Some((x, y));
                    s.graph_press = Some((None, x, y));
                }
            }
        });
    }
    {
        let st = state.clone();
        let ui2 = ui.clone_strong();
        ui.on_graph_pointer_move(move |x, y, _alt| {
            let mut needs_refresh = false;
            if let Some((id, _last_x, _last_y)) = st.borrow().graph_drag {
                let screen = {
                    let s = st.borrow();
                    s.graph_screen_positions.iter().find(|p| p.0 == id).copied()
                };
                {
                    let mut s = st.borrow_mut();
                    s.graph_drag = Some((id, x, y));
                    let zoom = s.graph_zoom.max(0.35);
                    if let (Some(pos), Some((_, screen_x, screen_y, _))) = (s.graph_positions.get_mut(&id), screen) {
                        pos.x += (x - screen_x) * 0.0038 / zoom;
                        pos.y += (y - screen_y) * 0.0038 / zoom;
                    }
                }
                needs_refresh = true;
            } else if let Some((last_x, last_y)) = st.borrow().graph_rotate {
                {
                    let mut s = st.borrow_mut();
                    let dx = x - last_x;
                    let dy = y - last_y;
                    s.graph_yaw += dx * 0.0085;
                    s.graph_pitch = (s.graph_pitch + dy * 0.0085).clamp(-1.2, 1.2);
                    s.graph_rotate = Some((x, y));
                    needs_refresh = true;
                }
            }
            if needs_refresh { refresh_graph(&st, &ui2); }
        });
    }
    {
        let st = state.clone();
        let ui2 = ui.clone_strong();
        ui.on_graph_pointer_up(move |x, y, alt| {
            let press = st.borrow_mut().graph_press.take();
            let dragged = st.borrow_mut().graph_drag.take().map(|d| d.0);
            st.borrow_mut().graph_rotate = None;
            let moved = press.map(|(_, sx, sy)| (x - sx).hypot(y - sy)).unwrap_or(999.0);
            match dragged {
                Some(id) => {
                    let target = if moved > 5.0 {
                        graph_hit(&st, x, y).filter(|hit| *hit != id)
                    } else { None };
                    if moved <= 5.0 {
                        let now = std::time::Instant::now();
                        let double = st.borrow().graph_last_click.map(|(last_id, last_time)| last_id == id && now.duration_since(last_time).as_millis() < 350).unwrap_or(false);
                        if double {
                            st.borrow_mut().graph_selected = Some(id);
                            select_note(&st, &ui2, id);
                        } else {
                            st.borrow_mut().graph_selected = Some(id);
                            st.borrow_mut().graph_last_click = Some((id, now));
                        }
                    } else if let Some(target_id) = target {
                        let target_title = {
                            let s = st.borrow();
                            get_from_state(&s, target_id).map(|n| n.title).unwrap_or_default()
                        };
                        if !target_title.trim().is_empty() {
                            let s = st.borrow();
                            if let Some(mut note) = get_from_state(&s, id) {
                                note.body = append_wiki_link(&note.body, target_title.trim());
                                let _ = update_from_state(&s, &note);
                            }
                            drop(s);
                            toast(&ui2, &format!("Linked to {target_title}"));
                            refresh_list(&st, &ui2);
                        }
                    } else if alt {
                        let s = st.borrow();
                        if let Some(mut note) = get_from_state(&s, id) {
                            note.body = remove_wiki_links(&note.body);
                            let _ = update_from_state(&s, &note);
                        }
                        drop(s);
                        toast(&ui2, "All links removed");
                        refresh_list(&st, &ui2);
                    }
                    st.borrow_mut().graph_selected = Some(id);
                }
                None => {
                    if alt && moved > 5.0 {
                        if let Some(id) = st.borrow().graph_selected {
                            let s = st.borrow();
                            if let Some(mut note) = get_from_state(&s, id) {
                                note.body = remove_wiki_links(&note.body);
                                let _ = update_from_state(&s, &note);
                            }
                            drop(s);
                            toast(&ui2, "All links removed");
                            refresh_list(&st, &ui2);
                        }
                    }
                }
            }
            refresh_graph(&st, &ui2);
        });
    }
    {
        let st = state.clone();
        let ui2 = ui.clone_strong();
        ui.on_graph_wheel(move |delta, _alt| {
            {
                let mut s = st.borrow_mut();
                s.graph_zoom = (s.graph_zoom * if delta < 0.0 { 1.10 } else { 0.90 }).clamp(0.35, 3.0);
            }
            refresh_graph(&st, &ui2);
        });
    }
    {
        let st = state.clone();
        let ui2 = ui.clone_strong();
        ui.on_graph_relabel(move || {
            {
                let mut s = st.borrow_mut();
                s.graph_positions.clear();
            }
            refresh_graph(&st, &ui2);
            toast(&ui2, "Graph re-laid out");
        });
    }
    {
        let st = state.clone();
        let ui2 = ui.clone_strong();
        ui.on_graph_close_selection(move || {
            st.borrow_mut().graph_selected = None;
            refresh_graph(&st, &ui2);
        });
    }
    {
        let st = state.clone();
        let ui2 = ui.clone_strong();
        ui.on_graph_open_note(move || {
            let id = st.borrow().graph_selected;
            if let Some(id) = id { select_note(&st, &ui2, id); }
        });
    }
    {
        let st = state.clone();
        let ui2 = ui.clone_strong();
        ui.on_graph_unlink_all(move || {
            let id = st.borrow().graph_selected;
            let Some(id) = id else { return };
            let s = st.borrow();
            if let Some(mut note) = get_from_state(&s, id) {
                note.body = remove_wiki_links(&note.body);
                let _ = update_from_state(&s, &note);
            }
            drop(s);
            refresh_list(&st, &ui2);
            refresh_graph(&st, &ui2);
            toast(&ui2, "All links removed");
        });
    }

    // ---------- doodle ----------
    {
        let st = state.clone();
        let ui2 = ui.clone_strong();
        ui.on_doodle_draw_down(move |x, y, _shift| {
            let color: String = {
                let c = ui2.get_doodle_pen_color();
                let rgba = c.to_argb_u8();
                format!("#{:02X}{:02X}{:02X}", rgba.red, rgba.green, rgba.blue)
            };
            let (width, height) = doodle_dimensions(&ui2);
            let mut s = st.borrow_mut();
            let id = s.doodle_strokes.last().map(|s| s.id + 1).unwrap_or(1);
            s.doodle_strokes.push(DoodleInternalStroke { id, color, size: ui2.get_doodle_pen_size(), points: vec![(x / width, y / height)] });
            drop(s);
            refresh_doodle_ui(&st.borrow(), &ui2);
        });
    }
    {
        let st = state.clone();
        let ui2 = ui.clone_strong();
        ui.on_doodle_draw_move(move |x, y, shift| {
            let (width, height) = doodle_dimensions(&ui2);
            let mut s = st.borrow_mut();
            if let Some(stroke) = s.doodle_strokes.last_mut() {
                let nx = x / width;
                let ny = if shift { stroke.points.first().map(|p| p.1).unwrap_or(y / height) } else { y / height };
                let last = stroke.points.last().copied().unwrap_or((nx, ny));
                if (nx - last.0).hypot(ny - last.1) > 0.004 {
                    stroke.points.push((nx, ny));
                }
            }
            drop(s);
            refresh_doodle_ui(&st.borrow(), &ui2);
        });
    }
    {
        let st = state.clone();
        let ui2 = ui.clone_strong();
        ui.on_doodle_draw_up(move |_x, _y| refresh_doodle_ui(&st.borrow(), &ui2));
    }
    {
        let st = state.clone();
        let ui2 = ui.clone_strong();
        ui.on_doodle_undo(move || {
            st.borrow_mut().doodle_strokes.pop();
            refresh_doodle_ui(&st.borrow(), &ui2);
        });
    }
    {
        let st = state.clone();
        let ui2 = ui.clone_strong();
        ui.on_doodle_clear(move || {
            st.borrow_mut().doodle_strokes.clear();
            refresh_doodle_ui(&st.borrow(), &ui2);
        });
    }
    {
        let st = state.clone();
        let ui2 = ui.clone_strong();
        ui.on_doodle_cancel(move || {
            st.borrow_mut().doodle_strokes.clear();
            ui2.set_doodle_open(false);
        });
    }
    {
        let st = state.clone();
        let ui2 = ui.clone_strong();
        ui.on_doodle_save(move || {
            let strokes = st.borrow().doodle_strokes.clone();
            if strokes.is_empty() {
                ui2.set_doodle_open(false);
                return;
            }
            let (width, height) = doodle_dimensions(&ui2);
            let mut png_ready = strokes.clone();
            let scale_x = 1200.0 / width;
            let scale_y = 800.0 / height;
            for stroke in &mut png_ready {
                for (x, y) in &mut stroke.points {
                    *x *= scale_x;
                    *y *= scale_y;
                }
            }
            let bytes = match doodle_png(&png_ready) {
                Ok(bytes) => bytes,
                Err(e) => { toast(&ui2, &format!("Could not save doodle: {e}")); return; }
            };
            let context = st.borrow().doodle_context;
            if context == 1 {
                let image = NoteImage {
                    meta: NoteImageMeta {
                        id: Uuid::new_v4(),
                        note_id: Uuid::nil(),
                        name: "doodle.png".into(),
                        mime: "image/png".into(),
                        size: bytes.len() as i64,
                        created_at: Utc::now(),
                    },
                    bytes,
                };
                st.borrow_mut().pending_note_images.push(image);
                set_pending_images_ui(&st.borrow(), &ui2);
                ui2.set_composer_open(true);
            } else if let Some(note_id) = st.borrow().current_id {
                let image = NoteImage {
                    meta: NoteImageMeta {
                        id: Uuid::new_v4(),
                        note_id,
                        name: "doodle.png".into(),
                        mime: "image/png".into(),
                        size: bytes.len() as i64,
                        created_at: Utc::now(),
                    },
                    bytes,
                };
                let key = st.borrow().vault_key.clone();
                let _ = notes_store::create_image(&st.borrow().db, &image, key.as_ref());
                load_note_images(&st, &ui2, note_id);
                refresh_list(&st, &ui2);
            }
            st.borrow_mut().doodle_strokes.clear();
            ui2.set_doodle_open(false);
        });
    }

    // ---------- misc settings actions ----------
    {
        let st = state.clone();
        let ui2 = ui.clone_strong();
        ui.on_export_editor(move || {
            let result = export_note_markdown(&st.borrow());
            match result { Ok(()) => toast(&ui2, "Markdown exported"), Err(e) => toast(&ui2, &e.to_string()) }
        });
    }
    {
        let ui2 = ui.clone_strong();
        let st = state.clone();
        ui.on_show_timestamps(move || {
            let Some(id) = st.borrow().current_id else { return };
            let s = st.borrow();
            if let Some(note) = get_from_state(&s, id) {
                ui2.set_timestamp_created(shared(note.created_at.with_timezone(&Local).format("%b %d, %Y · %I:%M %p").to_string()));
                ui2.set_timestamp_edited(shared(note.updated_at.with_timezone(&Local).format("%b %d, %Y · %I:%M %p").to_string()));
                let rows: Vec<SharedString> = activity_strings(&s, id).into_iter().map(shared).collect();
                ui2.set_timestamp_activity(ModelRc::new(VecModel::from(rows)));
                ui2.set_timestamps_open(true);
            }
        });
    }
    {
        let ui2 = ui.clone_strong();
        ui.on_markdown_guide(move || ui2.set_show_markdown_guide(true));
    }
    {
        let ui2 = ui.clone_strong();
        let st = state.clone();
        ui.on_import_md(move || {
            match import_markdown_files(&mut st.borrow_mut()) {
                Ok(count) => {
                    toast(&ui2, &format!("Imported {count} note{}", if count == 1 { "" } else { "s" }));
                    refresh_list(&st, &ui2);
                    refresh_graph(&st, &ui2);
                    if ui2.get_layout_style() == 2 { refresh_vault(&st, &ui2); }
                    set_settings_stats(&st, &ui2);
                }
                Err(e) => toast(&ui2, &e.to_string()),
            }
        });
    }
    {
        let ui2 = ui.clone_strong();
        let st = state.clone();
        ui.on_export_all_md(move || {
            match export_all_markdown(&st.borrow()) {
                Ok(()) => toast(&ui2, "notes-vault.md exported"),
                Err(e) => toast(&ui2, &e.to_string()),
            }
        });
    }
    {
        let ui2 = ui.clone_strong();
        let st = state.clone();
        ui.on_settings_density_selected(move |idx| {
            let density = match idx { 0 => "compact", 2 => "roomy", _ => "cozy" };
            {
                let mut s = st.borrow_mut();
                s.prefs.density = density.to_string();
            }
            ui2.set_density_setting(idx);
            save_config(&st.borrow());
            refresh_list(&st, &ui2);
        });
    }
    {
        let ui2 = ui.clone_strong();
        let st = state.clone();
        ui.on_settings_sort_selected(move |idx| {
            let sort = match idx { 1 => NotesSort::Created, 2 => NotesSort::Title, 3 => NotesSort::Manual, _ => NotesSort::Updated };
            {
                let mut s = st.borrow_mut();
                s.prefs.sort = sort;
            }
            ui2.set_sort_setting(idx);
            save_config(&st.borrow());
            refresh_list(&st, &ui2);
            toast(&ui2, if idx == 3 { "Manual order is available from list rows" } else { "Sort updated" });
        });
    }
    {
        let ui2 = ui.clone_strong();
        let st = state.clone();
        ui.on_settings_toggle_mobile_two(move || {
            {
                let mut s = st.borrow_mut();
                s.prefs.mobile_two = !s.prefs.mobile_two;
            }
            ui2.set_mobile_two(st.borrow().prefs.mobile_two);
            save_config(&st.borrow());
            refresh_list(&st, &ui2);
        });
    }
    {
        let ui2 = ui.clone_strong();
        let st = state.clone();
        ui.on_settings_toggle_pinned_first(move || {
            {
                let mut s = st.borrow_mut();
                s.prefs.pinned_first = !s.prefs.pinned_first;
            }
            ui2.set_pinned_first(st.borrow().prefs.pinned_first);
            save_config(&st.borrow());
            refresh_list(&st, &ui2);
        });
    }
    {
        let ui2 = ui.clone_strong();
        let st = state.clone();
        ui.on_settings_toggle_md_cards(move || {
            {
                let mut s = st.borrow_mut();
                s.prefs.md_cards = !s.prefs.md_cards;
            }
            ui2.set_md_cards(st.borrow().prefs.md_cards);
            save_config(&st.borrow());
            refresh_list(&st, &ui2);
        });
    }
    {
        let ui2 = ui.clone_strong();
        let st = state.clone();
        ui.on_settings_toggle_show_dates(move || {
            {
                let mut s = st.borrow_mut();
                s.prefs.show_dates = !s.prefs.show_dates;
            }
            ui2.set_show_dates(st.borrow().prefs.show_dates);
            save_config(&st.borrow());
            refresh_list(&st, &ui2);
        });
    }
    {
        let ui2 = ui.clone_strong();
        let st = state.clone();
        ui.on_settings_clamp_selected(move |idx| {
            {
                let mut s = st.borrow_mut();
                s.prefs.clamp = idx;
            }
            ui2.set_card_clamp(idx);
            save_config(&st.borrow());
            refresh_list(&st, &ui2);
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
