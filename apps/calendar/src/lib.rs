//! yellod Calendar — application logic.
//! UI lives in ui/calendar.slint; this file computes layout models and wires callbacks.

#![windows_subsystem = "windows"]

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::rc::Rc;

use chrono::{DateTime, Datelike, Duration, Local, NaiveDate, TimeZone, Timelike, Utc};
use slint::{Model, ModelRc, SharedString, VecModel};
use uuid::Uuid;

use ycore::model::{Calendar, Event, Recurrence, ReminderKind};
use ycore::storage::{calendar as cal_store, open_calendar_db};

slint::include_modules!();

const EVENT_COLORS: [&str; 7] = [
    "#aecbfa", "#f28b82", "#81c995", "#fdd663", "#d7aefb", "#78d9ec", "#fcad70",
];

const REMINDER_OPTIONS: [(&str, ReminderKind); 7] = [
    ("At time", ReminderKind::AtTime),
    ("5 min", ReminderKind::FiveMinutes),
    ("15 min", ReminderKind::FifteenMinutes),
    ("30 min", ReminderKind::ThirtyMinutes),
    ("1 hour", ReminderKind::OneHour),
    ("1 day", ReminderKind::OneDay),
    ("1 week", ReminderKind::OneWeek),
];

struct AppState {
    db: rusqlite::Connection,
    data_dir: PathBuf,
    config_path: PathBuf,
    dark: bool,
    view: i32,
    cursor: NaiveDate,
    seeded: bool,
    calendars: Vec<Calendar>,
    /// Event currently open in the dialog (None = creating a new one).
    editing_id: Option<Uuid>,
    editing_start: Option<DateTime<Utc>>,
    editing_end: Option<DateTime<Utc>>,
    fired_reminders: std::collections::HashSet<(Uuid, i64, usize)>,
}

// ---------- generic helpers ----------

fn data_dir() -> PathBuf {
    let base = dirs::data_dir().unwrap_or_else(|| PathBuf::from("."));
    base.join("yellod").join("Calendar")
}

fn parse_hex(s: &str) -> slint::Color {
    let h = s.trim().trim_start_matches('#');
    let v = u32::from_str_radix(h, 16).unwrap_or(0xaecbfa);
    slint::Color::from_rgb_u8(((v >> 16) & 0xff) as u8, ((v >> 8) & 0xff) as u8, (v & 0xff) as u8)
}

fn to_shared(s: &str) -> SharedString {
    SharedString::from(s)
}

fn load_config(path: &PathBuf) -> serde_json::Value {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_else(|| serde_json::json!({ "dark": true, "seeded": false }))
}

fn save_config(state: &AppState) {
    let cfg = serde_json::json!({ "dark": state.dark, "seeded": state.seeded });
    let _ = std::fs::write(&state.config_path, serde_json::to_string_pretty(&cfg).unwrap_or_default());
}

fn toast(ui: &MainWindow, msg: &str) {
    ui.set_toast_message(SharedString::from(msg));
    ui.set_toast_visible(true);
}

fn week_start_of(d: NaiveDate) -> NaiveDate {
    let wd = d.weekday().num_days_from_monday() as i64;
    d - Duration::days(wd)
}

fn local_midnight(d: NaiveDate) -> DateTime<Utc> {
    Local
        .from_local_datetime(&d.and_hms_opt(0, 0, 0).unwrap())
        .single()
        .map(|t| t.with_timezone(&Utc))
        .unwrap_or_else(Utc::now)
}

fn local_date(dt: DateTime<Utc>) -> NaiveDate {
    dt.with_timezone(&Local).date_naive()
}

fn time_label(dt: DateTime<Utc>) -> String {
    let l = dt.with_timezone(&Local);
    let t = l.format("%I:%M %p").to_string();
    let t = t.trim_start_matches('0').to_string();
    let today = Local::now().date_naive();
    let d = l.date_naive();
    if d == today {
        format!("Today, {t}")
    } else if d == today + Duration::days(1) {
        format!("Tomorrow, {t}")
    } else {
        l.format("%b %d, %I:%M %p").to_string().replace(" 0", " ")
    }
}

fn time_label_short(dt: DateTime<Utc>) -> String {
    let l = dt.with_timezone(&Local);
    l.format("%I:%M %p").to_string().trim_start_matches('0').to_string()
}

fn time_range_label(start: DateTime<Utc>, end: DateTime<Utc>, all_day: bool) -> String {
    if all_day {
        return "All day".to_string();
    }
    format!("{} – {}", time_label_short(start), time_label_short(end))
}

fn minutes_since_midnight(dt: DateTime<Utc>) -> i64 {
    let l = dt.with_timezone(&Local);
    (l.hour() * 60 + l.minute()) as i64
}

// ---------- recurrence expansion ----------

/// Expand an event's occurrences into [from, to) (UTC instants).
fn occurrences(ev: &Event, from: DateTime<Utc>, to: DateTime<Utc>) -> Vec<DateTime<Utc>> {
    let mut out = Vec::new();
    let start = ev.start;
    // Non-recurring: include if it overlaps the window.
    let duration = ev.end - ev.start;
    match ev.recurrence {
        None => {
            if start < to && ev.end > from {
                out.push(start);
            }
        }
        Some(Recurrence::Daily) => {
            let mut t = start;
            while t < to && out.len() < 400 {
                if t + duration > from {
                    out.push(t);
                }
                t = t + Duration::days(1);
            }
        }
        Some(Recurrence::Weekdays) => {
            let mut t = start;
            while t < to && out.len() < 400 {
                let wd = t.with_timezone(&Local).weekday().num_days_from_monday();
                if wd < 5 && t + duration > from {
                    out.push(t);
                }
                t = t + Duration::days(1);
            }
        }
        Some(Recurrence::Weekly) | Some(Recurrence::EveryNWeeks(_)) | Some(Recurrence::Custom) => {
            let step = match ev.recurrence {
                Some(Recurrence::EveryNWeeks(n)) if n > 1 => n as i64,
                _ => 1,
            };
            let mut t = start;
            while t < to && out.len() < 400 {
                if t + duration > from {
                    out.push(t);
                }
                t = t + Duration::weeks(step);
            }
        }
        Some(Recurrence::Monthly) => {
            let start_l = start.with_timezone(&Local);
            let dom = start_l.day();
            let (hh, mm) = (start_l.hour(), start_l.minute());
            let mut y = start_l.year();
            let mut m = start_l.month();
            loop {
                let dim = days_in_month(y, m);
                let d = dom.min(dim);
                let naive = NaiveDate::from_ymd_opt(y, m, d).unwrap();
                if let Some(t_local) = Local.from_local_datetime(&naive.and_hms_opt(hh, mm, 0).unwrap()).single() {
                    let t = t_local.with_timezone(&Utc);
                    if t + duration > from {
                        out.push(t);
                    }
                    if t >= to || out.len() > 400 {
                        break;
                    }
                }
                if m == 12 {
                    y += 1;
                    m = 1;
                } else {
                    m += 1;
                }
            }
        }
        Some(Recurrence::Yearly) => {
            let start_l = start.with_timezone(&Local);
            let (mon, dom, hh, mm) = (start_l.month(), start_l.day(), start_l.hour(), start_l.minute());
            let mut y = start_l.year();
            loop {
                let dim = days_in_month(y, mon);
                let naive = NaiveDate::from_ymd_opt(y, mon, dom.min(dim)).unwrap();
                if let Some(t_local) = Local.from_local_datetime(&naive.and_hms_opt(hh, mm, 0).unwrap()).single() {
                    let t = t_local.with_timezone(&Utc);
                    if t + duration > from {
                        out.push(t);
                    }
                    if t >= to || out.len() > 400 {
                        break;
                    }
                }
                y += 1;
            }
        }
    }
    out
}

fn days_in_month(y: i32, m: u32) -> u32 {
    NaiveDate::from_ymd_opt(if m == 12 { y + 1 } else { y }, if m == 12 { 1 } else { m + 1 }, 1)
        .unwrap()
        .pred_opt()
        .unwrap()
        .day()
}

/// All occurrences of all visible events overlapping [from, to), as (event, occurrence_start).
fn occurrences_in_range(
    events: &[Event],
    visible: &std::collections::HashSet<Uuid>,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
) -> Vec<(Event, DateTime<Utc>)> {
    let mut out = Vec::new();
    for ev in events {
        if let Some(cid) = ev.calendar_id {
            if !visible.contains(&cid) {
                continue;
            }
        }
        for occ in occurrences(ev, from, to) {
            out.push((ev.clone(), occ));
        }
    }
    out
}

// ---------- timeline overlap layout ----------

fn layout_day(events: Vec<(Uuid, DateTime<Utc>, DateTime<Utc>)>) -> Vec<(Uuid, f32, f32, f32, f32)> {
    // Returns: id, top_min, height_min, col, cols — greedy cluster layout.
    let mut sorted = events;
    sorted.sort_by(|a, b| a.1.cmp(&b.1).then((b.2 - b.1).cmp(&(a.2 - a.1))));

    let mut result = Vec::new();
    let mut cluster: Vec<usize> = Vec::new();
    let mut cluster_end = Utc::now();

    for (i, ev) in sorted.iter().enumerate() {
        if cluster.is_empty() || ev.1 < cluster_end {
            cluster.push(i);
            cluster_end = cluster_end.max(ev.2);
        } else {
            emit_cluster(&sorted, &cluster, &mut result);
            cluster.clear();
            cluster.push(i);
            cluster_end = ev.2;
        }
    }
    if !cluster.is_empty() {
        emit_cluster(&sorted, &cluster, &mut result);
    }
    result
}

fn emit_cluster(
    sorted: &Vec<(Uuid, DateTime<Utc>, DateTime<Utc>)>,
    cluster: &[usize],
    out: &mut Vec<(Uuid, f32, f32, f32, f32)>,
) {
    // Column assignment: greedy first-fit by time.
    let mut col_end: Vec<DateTime<Utc>> = Vec::new();
    let mut assignment: Vec<(usize, usize)> = Vec::new(); // event idx, col
    for &i in cluster {
        let ev = &sorted[i];
        let mut placed = None;
        for (c, end) in col_end.iter().enumerate() {
            if *end <= ev.1 {
                placed = Some(c);
                break;
            }
        }
        match placed {
            Some(c) => {
                col_end[c] = ev.2;
                assignment.push((i, c));
            }
            None => {
                col_end.push(ev.2);
                assignment.push((i, col_end.len() - 1));
            }
        }
    }
    let cols = col_end.len() as f32;
    for (i, col) in assignment {
        let ev = &sorted[i];
        let start_min = minutes_since_midnight(ev.1) as f32;
        let end_min = minutes_since_midnight(ev.2) as f32;
        let dur = (end_min - start_min).max(15.0);
        out.push((ev.0, start_min, dur, col as f32, cols));
    }
}

// ---------- view data builders ----------

fn visible_calendar_ids(state: &AppState) -> std::collections::HashSet<Uuid> {
    state.calendars.iter().filter(|c| c.visible).map(|c| c.id).collect()
}

fn weekday_label(d: NaiveDate) -> &'static str {
    match d.weekday().num_days_from_monday() {
        0 => "Mon", 1 => "Tue", 2 => "Wed", 3 => "Thu", 4 => "Fri", 5 => "Sat", _ => "Sun",
    }
}

fn date_title(state: &AppState) -> String {
    let c = state.cursor;
    match state.view {
        0 => c.format("%A, %B %d, %Y").to_string(),
        1 => {
            let s = week_start_of(c);
            let e = s + Duration::days(6);
            if s.month() == e.month() {
                format!("{} {} – {}, {}", s.format("%B"), s.day(), e.day(), e.year())
            } else if s.year() == e.year() {
                format!("{} {} – {} {}, {}", s.format("%b"), s.day(), e.format("%b"), e.day(), e.year())
            } else {
                format!("{} {}, {} – {} {}, {}", s.format("%b"), s.day(), s.year(), e.format("%b"), e.day(), e.year())
            }
        }
        2 => c.format("%B %Y").to_string(),
        _ => c.format("%Y").to_string(),
    }
}

struct DayContent {
    blocks_per_day: Vec<Vec<HourEventRect>>,
    all_day: Vec<MonthChip>,
}

fn build_day_columns(
    events: &[Event],
    visible: &std::collections::HashSet<Uuid>,
    days: &[NaiveDate],
) -> DayContent {
    let range_start = local_midnight(days[0]);
    let range_end = local_midnight(*days.last().unwrap()) + Duration::days(1);
    let occs = occurrences_in_range(events, visible, range_start, range_end);

    let mut timed: BTreeMap<usize, Vec<(Uuid, DateTime<Utc>, DateTime<Utc>, Event)>> = BTreeMap::new();
    let mut all_day: Vec<MonthChip> = Vec::new();
    for (ev, occ) in occs {
        let occ_date = local_date(occ);
        let dur = ev.end - ev.start;
        let end = occ + dur;
        let mut covered = false;
        for (i, d) in days.iter().enumerate() {
            if *d == occ_date {
                if ev.all_day {
                    all_day.push(MonthChip {
                        id: ev.id.to_string().into(),
                        title: ev.title.clone().into(),
                        color: parse_hex(ev.color.as_deref().unwrap_or("#aecbfa")),
                    });
                } else {
                    timed.entry(i).or_default().push((ev.id, occ, end, ev.clone()));
                }
                covered = true;
                break;
            }
        }
        if !covered && !ev.all_day {
            for (i, d) in days.iter().enumerate() {
                let day_start = local_midnight(*d);
                let day_end = day_start + Duration::days(1);
                let clip_start = occ.max(day_start);
                let clip_end = end.min(day_end);
                if clip_start < clip_end {
                    timed.entry(i).or_default().push((ev.id, clip_start, clip_end, ev.clone()));
                }
            }
        }
    }

    let mut blocks_out: Vec<Vec<HourEventRect>> = Vec::new();
    for i in 0..days.len() {
        let list = timed.remove(&i).unwrap_or_default();
        let input: Vec<(Uuid, DateTime<Utc>, DateTime<Utc>)> = list.iter().map(|(id, s, e, _)| (*id, *s, *e)).collect();
        let laid = layout_day(input);
        let mut blocks = Vec::new();
        for (id, top, height, col, cols) in laid {
            let ev = list.iter().find(|(eid, _, _, _)| *eid == id).unwrap();
            let start_l = ev.1.with_timezone(&Local);
            let end_l = ev.2.with_timezone(&Local);
            let label = format!(
                "{} – {}",
                start_l.format("%I:%M %p").to_string().trim_start_matches('0'),
                end_l.format("%I:%M %p").to_string().trim_start_matches('0')
            );
            blocks.push(HourEventRect {
                id: id.to_string().into(),
                title: ev.3.title.clone().into(),
                color: parse_hex(ev.3.color.as_deref().unwrap_or("#aecbfa")),
                top_min: top,
                height_min: height,
                col,
                cols,
                time_label: label.into(),
            });
        }
        blocks_out.push(blocks);
    }
    DayContent { blocks_per_day: blocks_out, all_day }
}

// ---------- month / year / mini-month builders ----------

struct MonthGrid {
    cells: Vec<MonthCell>,   // 42
}

fn month_grid(
    events: &[Event],
    visible: &std::collections::HashSet<Uuid>,
    grid_start: NaiveDate,
    cursor: NaiveDate,
    today: NaiveDate,
) -> MonthGrid {
    let days: Vec<NaiveDate> = (0..42).map(|i| grid_start + Duration::days(i)).collect();
    let occs = occurrences_in_range(events, visible, local_midnight(grid_start), local_midnight(grid_start + Duration::days(42)) + Duration::days(1));

    // Count events per day
    let mut per_day: Vec<Vec<MonthChip>> = vec![Vec::new(); 42];
    for (ev, occ) in occs {
        let dur = ev.end - ev.start;
        for (i, d) in days.iter().enumerate() {
            let occ_end = occ + dur;
            if local_date(occ) <= *d && local_date(occ_end) >= *d {
                per_day[i].push(MonthChip {
                    id: ev.id.to_string().into(),
                    title: ev.title.clone().into(),
                    color: parse_hex(ev.color.as_deref().unwrap_or("#aecbfa")),
                });
            }
        }
    }

    let mut cells = Vec::with_capacity(42);
    let cursor_month = cursor.month();
    let cursor_year = cursor.year();
    for (i, d) in days.iter().enumerate() {
        let outside = d.month() != cursor_month || d.year() != cursor_year;
        let chip_count = per_day[i].len();
        let chips: Vec<MonthChip> = per_day[i].iter().take(3).cloned().collect();
        cells.push(MonthCell {
            day_num: d.day().to_string().into(),
            today: *d == today,
            outside,
            selected: *d == cursor,
            chips: ModelRc::new(VecModel::from(chips)),
            more: (chip_count.saturating_sub(3)) as i32,
        });
    }
    MonthGrid { cells }
}

struct YearGrid {
    months: Vec<YearMonth>,
}

fn year_grid(
    events: &[Event],
    visible: &std::collections::HashSet<Uuid>,
    year: i32,
    today: NaiveDate,
) -> YearGrid {
    let year_start = local_midnight(NaiveDate::from_ymd_opt(year, 1, 1).unwrap());
    let year_end = local_midnight(NaiveDate::from_ymd_opt(year + 1, 1, 1).unwrap());
    let occs = occurrences_in_range(events, visible, year_start, year_end);

    let mut busy: std::collections::HashSet<NaiveDate> = std::collections::HashSet::new();
    for (ev, occ) in occs {
        let dur = ev.end - ev.start;
        let mut d = local_date(occ);
        let end_d = local_date(occ + dur);
        while d <= end_d {
            busy.insert(d);
            d += Duration::days(1);
        }
    }

    let mut months = Vec::with_capacity(12);
    for m in 1..=12u32 {
        let dim = days_in_month(year, m);
        let first = NaiveDate::from_ymd_opt(year, m, 1).unwrap();
        let lead = first.weekday().num_days_from_monday() as i32;
        let days: Vec<YearDay> = (1..=dim)
            .map(|d| {
                let date = NaiveDate::from_ymd_opt(year, m, d).unwrap();
                YearDay {
                    day: d as i32,
                    busy: busy.contains(&date),
                    today: date == today,
                }
            })
            .collect();
        let name = NaiveDate::from_ymd_opt(year, m, 1).unwrap().format("%B").to_string();
        months.push(YearMonth {
            name: name.into(),
            lead,
            days: ModelRc::new(VecModel::from(days)),
        });
    }
    YearGrid { months }
}

fn mini_month(
    events: &[Event],
    visible: &std::collections::HashSet<Uuid>,
    month_start: NaiveDate,
    cursor: NaiveDate,
    today: NaiveDate,
) -> (Vec<MiniDay>, i32) {
    let grid_start = week_start_of(month_start);
    let days: Vec<NaiveDate> = (0..42).map(|i| grid_start + Duration::days(i)).collect();
    let occs = occurrences_in_range(events, visible, local_midnight(grid_start), local_midnight(grid_start + Duration::days(42)) + Duration::days(1));
    let mut busy: std::collections::HashSet<NaiveDate> = std::collections::HashSet::new();
    for (ev, occ) in occs {
        let dur = ev.end - ev.start;
        let mut d = local_date(occ);
        let end_d = local_date(occ + dur);
        while d <= end_d {
            busy.insert(d);
            d += Duration::days(1);
        }
    }
    let cells = days
        .iter()
        .filter(|d| d.month() == month_start.month() && d.year() == month_start.year())
        .map(|d| MiniDay {
            day: d.day() as i32,
            today: *d == today,
            selected: *d == cursor,
            busy: busy.contains(d),
        })
        .collect();
    let lead = grid_start.weekday().num_days_from_monday() as i32;
    (cells, lead)
}

fn upcoming_events(
    events: &[Event],
    visible: &std::collections::HashSet<Uuid>,
) -> Vec<UpcomingEvent> {
    let now = Utc::now();
    let window_end = now + Duration::days(90);
    let mut list = occurrences_in_range(events, visible, now, window_end);
    list.sort_by_key(|(_, occ)| *occ);
    list.dedup_by(|a, b| a.0.id == b.0.id && a.1 == b.1);
    list.into_iter()
        .take(5)
        .map(|(ev, occ)| UpcomingEvent {
            id: ev.id.to_string().into(),
            title: ev.title.into(),
            time_label: time_label(occ).into(),
            color: parse_hex(ev.color.as_deref().unwrap_or("#aecbfa")),
        })
        .collect()
}

fn sidebar_calendars(state: &AppState, events: &[Event]) -> Vec<SidebarCalendar> {
    state
        .calendars
        .iter()
        .map(|c| {
            let count = events.iter().filter(|e| e.calendar_id == Some(c.id)).count();
            SidebarCalendar {
                id: c.id.to_string().into(),
                name: c.name.clone().into(),
                color: parse_hex(&c.color),
                visible: c.visible,
                count: count.to_string().into(),
            }
        })
        .collect()
}


// ---------- master refresh ----------

fn refresh_all(state: &Rc<RefCell<AppState>>, ui: &MainWindow) {
    let events: Vec<Event> = cal_store::list_events(&state.borrow().db).unwrap_or_default();
    let visible = visible_calendar_ids(&state.borrow());
    let today = Local::now().date_naive();

    ui.set_date_title(to_shared(&date_title(&state.borrow())));
    ui.set_view(state.borrow().view);

    // Sidebar: mini month for cursor month
    let month_start = state.borrow().cursor.with_day(1).unwrap();
    let (mini_days, mini_lead) = mini_month(&events, &visible, month_start, state.borrow().cursor, today);
    ui.set_mini_month_title(to_shared(&month_start.format("%B %Y").to_string()));
    ui.set_mini_days(ModelRc::new(VecModel::from(mini_days)));
    ui.set_mini_lead(mini_lead);

    // Sidebar: calendars with counts + upcoming
    {
        let s = state.borrow();
        ui.set_calendars(ModelRc::new(VecModel::from(sidebar_calendars(&s, &events))));
        ui.set_upcoming(ModelRc::new(VecModel::from(upcoming_events(&events, &visible))));
    }

    let view = state.borrow().view;
    match view {
        0 => {
            let cursor = state.borrow().cursor;
            let days = vec![cursor];
            let content = build_day_columns(&events, &visible, &days);
            let now_min = minutes_since_midnight(Utc::now()) as i32;
            ui.set_now_min(now_min);
            let columns: Vec<TimelineColumn> = days
                .iter()
                .zip(content.blocks_per_day)
                .map(|(d, blocks)| TimelineColumn {
                    day: WeekDay {
                        label: weekday_label(*d).into(),
                        day_num: d.day().to_string().into(),
                        full: d.format("%A, %B %d").to_string().into(),
                        today: *d == today,
                    },
                    blocks: ModelRc::new(VecModel::from(blocks)),
                })
                .collect();
            ui.set_day_columns(ModelRc::new(VecModel::from(columns)));
            ui.set_all_day(ModelRc::new(VecModel::from(content.all_day)));
            let count = cal_store::list_events(&state.borrow().db).unwrap_or_default().len();
            ui.set_event_count(count as i32);
        }
        1 => {
            let cursor = state.borrow().cursor;
            let start = week_start_of(cursor);
            let days: Vec<NaiveDate> = (0..7).map(|i| start + Duration::days(i)).collect();
            let content = build_day_columns(&events, &visible, &days);
            let now_min = minutes_since_midnight(Utc::now()) as i32;
            ui.set_now_min(now_min);
            let columns: Vec<TimelineColumn> = days
                .iter()
                .zip(content.blocks_per_day)
                .map(|(d, blocks)| TimelineColumn {
                    day: WeekDay {
                        label: weekday_label(*d).into(),
                        day_num: d.day().to_string().into(),
                        full: d.format("%A, %B %d").to_string().into(),
                        today: *d == today,
                    },
                    blocks: ModelRc::new(VecModel::from(blocks)),
                })
                .collect();
            ui.set_week_columns(ModelRc::new(VecModel::from(columns)));
            ui.set_all_day(ModelRc::new(VecModel::from(content.all_day)));
            let count = cal_store::list_events(&state.borrow().db).unwrap_or_default().len();
            ui.set_event_count(count as i32);
        }
        2 => {
            let cursor = state.borrow().cursor;
            let first = cursor.with_day(1).unwrap();
            let grid_start = week_start_of(first);
            let grid = month_grid(&events, &visible, grid_start, cursor, today);
            ui.set_month_cells(ModelRc::new(VecModel::from(grid.cells)));
            let count = events.len();
            ui.set_event_count(count as i32);
        }
        _ => {
            let cursor = state.borrow().cursor;
            let year = cursor.year();
            let grid = year_grid(&events, &visible, year, today);
            ui.set_year_months(ModelRc::new(VecModel::from(grid.months)));
            ui.set_current_month(if today.year() == year { today.month() as i32 - 1 } else { -1 });
            ui.set_event_count(events.len() as i32);
        }
    }
}

// ---------- seed data ----------

fn seed_if_empty(state: &Rc<RefCell<AppState>>) {
    let needs_seed = {
        let s = state.borrow();
        !s.seeded
    };
    if !needs_seed {
        return;
    }

    {
        let s = state.borrow();
        let db = &s.db;
        let count: i64 = db.query_row("SELECT COUNT(*) FROM events", [], |r| r.get(0)).unwrap_or(0);
        if count == 0 {
            let personal = Calendar::new("Personal", "#aecbfa");
            let work = Calendar::new("Work", "#d7aefb");
            let _ = cal_store::create_calendar(db, &personal);
            let _ = cal_store::create_calendar(db, &work);

            let today = Local::now().date_naive();
            let mk = |title: &str, date: NaiveDate, h: u32, m: u32, dur_min: i64, rec: Option<Recurrence>, color: &str, cal: &Calendar, reminder: ReminderKind, all_day: bool| {
                let start = local_midnight(date) + Duration::minutes((h as i64) * 60 + m as i64);
                let mut ev = Event::new(title, start, start + Duration::minutes(dur_min));
                ev.recurrence = rec;
                ev.color = Some(color.to_string());
                ev.calendar_id = Some(cal.id);
                ev.reminders = vec![reminder];
                ev.all_day = all_day;
                ev
            };
            let tomorrow = today + Duration::days(1);
            let days_until_friday = (4 - today.weekday().num_days_from_monday() as i64 + 7) % 7;
            let friday = today + Duration::days(if days_until_friday == 0 { 7 } else { days_until_friday });
            let days_until_saturday = (5 - today.weekday().num_days_from_monday() as i64 + 7) % 7;
            let saturday = today + Duration::days(if days_until_saturday == 0 { 7 } else { days_until_saturday });
            let sunday = saturday + Duration::days(1);

            let samples = vec![
                mk("Team standup", tomorrow, 9, 30, 15, Some(Recurrence::Weekdays), "#d7aefb", &work, ReminderKind::FifteenMinutes, false),
                mk("Design review", friday, 14, 0, 60, None, "#aecbfa", &personal, ReminderKind::ThirtyMinutes, false),
                mk("Weekend hike at Rattler Ridge", saturday, 8, 0, 240, None, "#81c995", &personal, ReminderKind::OneHour, false),
                mk("Farmers market", sunday, 10, 0, 120, None, "#fdd663", &personal, ReminderKind::AtTime, false),
            ];
            for ev in samples {
                let _ = cal_store::create_event(db, &ev);
            }
        }
    }

    // Mark seeded + load calendars
    {
        let mut s = state.borrow_mut();
        s.seeded = true;
        s.calendars = cal_store::list_calendars(&s.db).unwrap_or_default();
        save_config(&s);
    }
}

// ---------- reminder firing ----------

fn due_reminders(state: &Rc<RefCell<AppState>>) -> Vec<(Uuid, DateTime<Utc>, ReminderKind, String)> {
    let s = state.borrow();
    let events = cal_store::list_events(&s.db).unwrap_or_default();
    let visible = visible_calendar_ids(&s);
    let now = Utc::now();
    let window_start = now - Duration::seconds(75);
    let window_end = now + Duration::seconds(5);
    let occs = occurrences_in_range(&events, &visible, window_start, window_end + Duration::days(1));
    let mut out = Vec::new();
    for (ev, occ) in occs {
        if ev.reminders.is_empty() {
            continue;
        }
        for kind in &ev.reminders {
            let fire_at = occ - Duration::minutes(kind.minutes_before());
            if fire_at >= window_start && fire_at <= window_end {
                out.push((ev.id, occ, *kind, ev.title.clone()));
            }
        }
    }
    out
}

// ---------- open/close the event dialog ----------

fn format_full(dt: DateTime<Utc>) -> String {
    let l = dt.with_timezone(&Local);
    let s = l.format("%a, %b %d · %I:%M %p").to_string();
    s.replace(" 0", " ")
}

fn open_event_dialog(
    state: &Rc<RefCell<AppState>>,
    ui: &MainWindow,
    event: Option<Event>,
    slot: Option<(NaiveDate, i64)>,
) {
    let now = Local::now();
    let (start, end) = match event.as_ref() {
        Some(ev) => (ev.start, ev.end),
        None => {
            let (date, minutes) = slot.unwrap_or_else(|| {
                let n = Local::now();
                let d = n.date_naive();
                let minutes = if n.hour() < 20 {
                    ((n.hour() * 60 + n.minute() + 29) / 30) as i64 * 30
                } else {
                    9 * 60
                };
                (d, minutes)
            });
            let start = local_midnight(date) + Duration::minutes(minutes);
            (start, start + Duration::hours(1))
        }
    };

    // Snapshot state
    let calendars = state.borrow().calendars.clone();

    {
        let mut s = state.borrow_mut();
        s.editing_id = event.as_ref().map(|e| e.id);
        s.editing_start = Some(start);
        s.editing_end = Some(end);
    }

    ui.set_dialog_editing(event.is_some());
    match event.as_ref() {
        Some(ev) => {
            ui.set_dialog_title(ev.title.clone().into());
            ui.set_dialog_location(ev.location.clone().unwrap_or_default().into());
            ui.set_dialog_notes(ev.notes.clone().into());
            ui.set_dialog_all_day(ev.all_day);
            ui.set_dialog_recurrence(match ev.recurrence {
                None => 0,
                Some(Recurrence::Daily) => 1,
                Some(Recurrence::Weekly) | Some(Recurrence::EveryNWeeks(_)) => 2,
                Some(Recurrence::Monthly) => 3,
                Some(Recurrence::Yearly) => 4,
                Some(Recurrence::Weekdays) => 5,
                Some(Recurrence::Custom) => 0,
            });
            let reminders: Vec<ReminderOption> = REMINDER_OPTIONS
                .iter()
                .map(|(label, kind)| ReminderOption {
                    label: to_shared(label),
                    minutes: kind.minutes_before() as i32,
                    on: ev.reminders.contains(kind),
                })
                .collect();
            ui.set_dialog_reminders(ModelRc::new(VecModel::from(reminders)));
            let colors: Vec<EventColor> = EVENT_COLORS
                .iter()
                .map(|hex| EventColor {
                    color: parse_hex(hex),
                    on: ev.color.as_deref().map(|c| c.eq_ignore_ascii_case(hex)).unwrap_or(false),
                })
                .collect();
            ui.set_dialog_colors(ModelRc::new(VecModel::from(colors)));
            let cal_idx = ev
                .calendar_id
                .and_then(|cid| calendars.iter().position(|c| c.id == cid))
                .unwrap_or(0) as i32;
            ui.set_dialog_calendar(cal_idx);
        }
        None => {
            ui.set_dialog_title(SharedString::new());
            ui.set_dialog_location(SharedString::new());
            ui.set_dialog_notes(SharedString::new());
            ui.set_dialog_all_day(false);
            ui.set_dialog_recurrence(0);
            let reminders: Vec<ReminderOption> = REMINDER_OPTIONS
                .iter()
                .map(|(label, kind)| ReminderOption {
                    label: to_shared(label),
                    minutes: kind.minutes_before() as i32,
                    on: *kind == ReminderKind::FifteenMinutes,
                })
                .collect();
            ui.set_dialog_reminders(ModelRc::new(VecModel::from(reminders)));
            let colors: Vec<EventColor> = EVENT_COLORS
                .iter()
                .enumerate()
                .map(|(i, hex)| EventColor {
                    color: parse_hex(hex),
                    on: i == 0,
                })
                .collect();
            ui.set_dialog_colors(ModelRc::new(VecModel::from(colors)));
            ui.set_dialog_calendar(0);
        }
    }
    let cal_chips: Vec<CalendarChip> = calendars
        .iter()
        .map(|c| CalendarChip {
            name: c.name.clone().into(),
            color: parse_hex(&c.color),
        })
        .collect();
    ui.set_dialog_calendars(ModelRc::new(VecModel::from(cal_chips)));
    ui.set_dialog_start(to_shared(&format_full(start)));
    ui.set_dialog_end(to_shared(&format_full(end)));
    ui.set_show_dialog(true);
}

fn current_grid_start(state: &Rc<RefCell<AppState>>) -> NaiveDate {
    let c = state.borrow().cursor;
    week_start_of(c.with_day(1).unwrap())
}

fn shift_months(d: NaiveDate, delta: i32) -> NaiveDate {
    let total = d.year() * 12 + d.month() as i32 - 1 + delta;
    let y = total.div_euclid(12);
    let m = (total.rem_euclid(12) + 1) as u32;
    let dim = days_in_month(y, m);
    NaiveDate::from_ymd_opt(y, m, d.day().min(dim)).unwrap()
}

fn current_days(state: &Rc<RefCell<AppState>>) -> Vec<NaiveDate> {
    let s = state.borrow();
    if s.view == 0 {
        vec![s.cursor]
    } else {
        let start = week_start_of(s.cursor);
        (0..7).map(|i| start + Duration::days(i)).collect()
    }
}
// ---------- entry point ----------

pub fn run() -> anyhow::Result<()> {
    #[cfg(target_os = "android")]
    compile_error!("Android builds must call android_main instead of run()");
    run_ui()
}

pub fn run_ui() -> anyhow::Result<()> {
    let ui = MainWindow::new()?;

    let dir = data_dir();
    std::fs::create_dir_all(&dir)?;
    let config_path = dir.join("config.json");
    let cfg = load_config(&config_path);
    let mut state = AppState {
        db: open_calendar_db(&dir.join("calendar.db"))?,
        data_dir: dir,
        config_path,
        dark: cfg.get("dark").and_then(|v| v.as_bool()).unwrap_or(true),
        view: 2,
        cursor: Local::now().date_naive(),
        seeded: cfg.get("seeded").and_then(|v| v.as_bool()).unwrap_or(false),
        calendars: Vec::new(),
        editing_id: None,
        editing_start: None,
        editing_end: None,
        fired_reminders: std::collections::HashSet::new(),
    };
    ui.set_dark(state.dark);
    state.calendars = cal_store::list_calendars(&state.db).unwrap_or_default();
    let state = Rc::new(RefCell::new(state));
    seed_if_empty(&state);
    refresh_all(&state, &ui);

    // ---------- chrome callbacks ----------
    {
        let ui2 = ui.clone_strong();
        let st = state.clone();
        ui.on_nav_previous(move || {
            {
                let mut s = st.borrow_mut();
                s.cursor = match s.view {
                    0 => s.cursor - Duration::days(1),
                    1 => s.cursor - Duration::weeks(1),
                    2 => shift_months(s.cursor, -1),
                    _ => s.cursor.with_year(s.cursor.year() - 1).unwrap_or(s.cursor),
                };
            }
            refresh_all(&st, &ui2);
        });
    }
    {
        let ui2 = ui.clone_strong();
        let st = state.clone();
        ui.on_nav_next(move || {
            {
                let mut s = st.borrow_mut();
                s.cursor = match s.view {
                    0 => s.cursor + Duration::days(1),
                    1 => s.cursor + Duration::weeks(1),
                    2 => shift_months(s.cursor, 1),
                    _ => s.cursor.with_year(s.cursor.year() + 1).unwrap_or(s.cursor),
                };
            }
            refresh_all(&st, &ui2);
        });
    }
    {
        let ui2 = ui.clone_strong();
        let st = state.clone();
        ui.on_today(move || {
            st.borrow_mut().cursor = Local::now().date_naive();
            refresh_all(&st, &ui2);
        });
    }
    {
        let ui2 = ui.clone_strong();
        let st = state.clone();
        ui.on_view_changed(move |view| {
            st.borrow_mut().view = view;
            ui2.set_view(view);
            refresh_all(&st, &ui2);
        });
    }
    {
        let ui2 = ui.clone_strong();
        ui.on_toggle_sidebar(move || {
            ui2.set_sidebar_open(!ui2.get_sidebar_open());
        });
    }
    {
        let ui2 = ui.clone_strong();
        let st = state.clone();
        ui.on_toggle_theme(move || {
            let now_dark = !ui2.get_dark();
            ui2.set_dark(now_dark);
            st.borrow_mut().dark = now_dark;
            save_config(&st.borrow());
        });
    }
    {
        let ui2 = ui.clone_strong();
        let st = state.clone();
        ui.on_mini_month_nav(move |delta| {
            st.borrow_mut().cursor = shift_months(st.borrow().cursor, delta);
            refresh_all(&st, &ui2);
        });
    }
    {
        let ui2 = ui.clone_strong();
        let st = state.clone();
        ui.on_day_selected(move |day| {
            {
                let mut s = st.borrow_mut();
                let dim = days_in_month(s.cursor.year(), s.cursor.month());
                s.cursor = NaiveDate::from_ymd_opt(s.cursor.year(), s.cursor.month(), (day as u32).min(dim)).unwrap_or(s.cursor);
                s.view = 0;
            }
            ui2.set_view(0);
            refresh_all(&st, &ui2);
        });
    }
    {
        let ui2 = ui.clone_strong();
        let st = state.clone();
        ui.on_calendar_toggled(move |id| {
            if let Ok(cid) = Uuid::parse_str(&id) {
                {
                    let s = st.borrow();
                    if let Some(cal) = s.calendars.iter().find(|c| c.id == cid) {
                        let _ = cal_store::set_calendar_visible(&s.db, cid, !cal.visible);
                    }
                }
        
        let new_cals = cal_store::list_calendars(&st.borrow().db).unwrap_or_default();
        st.borrow_mut().calendars = new_cals;
                refresh_all(&st, &ui2);
            }
        });
    }

    // ---------- event callbacks ----------
    {
        let ui2 = ui.clone_strong();
        let st = state.clone();
        ui.on_new_event(move || {
            open_event_dialog(&st, &ui2, None, None);
        });
    }
    {
        let ui2 = ui.clone_strong();
        let st = state.clone();
        ui.on_event_clicked(move |id| {
            if let Ok(eid) = Uuid::parse_str(&id) {
                let s = st.borrow();
                if let Ok(Some(ev)) = cal_store::get_event(&s.db, eid) {
                    open_event_dialog(&st, &ui2, Some(ev), None);
                }
            }
        });
    }
    {
        let ui2 = ui.clone_strong();
        let st = state.clone();
        ui.on_slot_clicked(move |_day_index, minutes| {
            let days = current_days(&st);
            let idx = (_day_index as usize).min(days.len().saturating_sub(1));
            let date = days.get(idx).copied().unwrap_or_else(|| Local::now().date_naive());
            open_event_dialog(&st, &ui2, None, Some((date, minutes as i64)));
        });
    }
    {
        let ui2 = ui.clone_strong();
        let st = state.clone();
        ui.on_month_cell_clicked(move |cell_index| {
            let grid_start = current_grid_start(&st);
            let date = grid_start + Duration::days(cell_index as i64);
            {
                let mut s = st.borrow_mut();
                s.cursor = date;
                s.view = 0;
            }
            ui2.set_view(0);
            refresh_all(&st, &ui2);
        });
    }
    {
        let ui2 = ui.clone_strong();
        let st = state.clone();
        ui.on_chip_clicked(move |id| {
            if let Ok(eid) = Uuid::parse_str(&id) {
                let s = st.borrow();
                if let Ok(Some(ev)) = cal_store::get_event(&s.db, eid) {
                    open_event_dialog(&st, &ui2, Some(ev), None);
                }
            }
        });
    }
    {
        let ui2 = ui.clone_strong();
        let st = state.clone();
        ui.on_week_event_clicked(move |id, _day| {
            if let Ok(eid) = Uuid::parse_str(&id) {
                let s = st.borrow();
                if let Ok(Some(ev)) = cal_store::get_event(&s.db, eid) {
                    open_event_dialog(&st, &ui2, Some(ev), None);
                }
            }
        });
    }
    {
        let ui2 = ui.clone_strong();
        let st = state.clone();
        ui.on_month_clicked(move |month_index| {
            let y = st.borrow().cursor.year();
            let m = (month_index + 1) as u32;
            st.borrow_mut().cursor = NaiveDate::from_ymd_opt(y, m, 1).unwrap_or(Local::now().date_naive());
            st.borrow_mut().view = 2;
            ui2.set_view(2);
            refresh_all(&st, &ui2);
        });
    }
    {
        let ui2 = ui.clone_strong();
        let st = state.clone();
        ui.on_year_day_clicked(move |month_index, day| {
            let y = st.borrow().cursor.year();
            let m = (month_index + 1) as u32;
            let dim = days_in_month(y, m);
            st.borrow_mut().cursor = NaiveDate::from_ymd_opt(y, m, (day as u32).min(dim)).unwrap_or(Local::now().date_naive());
            st.borrow_mut().view = 0;
            ui2.set_view(0);
            refresh_all(&st, &ui2);
        });
    }

    // ---------- dialog callbacks ----------
    {
        let ui2 = ui.clone_strong();
        ui.on_cancel_event(move || {
            ui2.set_show_dialog(false);
        });
    }
    {
        let ui2 = ui.clone_strong();
        let st = state.clone();
        ui.on_save_event(move || {
            let title = ui2.get_dialog_title().to_string();
            let start = st.borrow().editing_start.unwrap_or_else(Utc::now);
            let mut end = st.borrow().editing_end.unwrap_or(start + Duration::hours(1));
            if end <= start {
                end = start + Duration::minutes(30);
            }
            let all_day = ui2.get_dialog_all_day();
            let recurrence = match ui2.get_dialog_recurrence() {
                1 => Some(Recurrence::Daily),
                2 => Some(Recurrence::Weekly),
                3 => Some(Recurrence::Monthly),
                4 => Some(Recurrence::Yearly),
                5 => Some(Recurrence::Weekdays),
                _ => None,
            };
            let reminders: Vec<ReminderKind> = ui2
                .get_dialog_reminders()
                .iter()
                .enumerate()
                .filter(|(_, r)| r.on)
                .map(|(i, _)| REMINDER_OPTIONS[i].1)
                .collect();
            let color = ui2
                .get_dialog_colors()
                .iter()
                .position(|c| c.on)
                .map(|i| EVENT_COLORS[i].to_string());
            let calendar_idx = ui2.get_dialog_calendar().max(0) as usize;
            let calendar_id = st.borrow().calendars.get(calendar_idx).map(|c| c.id);
            let location = ui2.get_dialog_location().to_string();
            let notes = ui2.get_dialog_notes().to_string();
            let location_opt = if location.trim().is_empty() { None } else { Some(location.trim().to_string()) };
            let final_title = if title.trim().is_empty() { "Untitled event".to_string() } else { title.trim().to_string() };

            {
                let s = st.borrow();
                match s.editing_id {
                    Some(id) => {
                        if let Ok(Some(mut ev)) = cal_store::get_event(&s.db, id) {
                            ev.title = final_title;
                            ev.start = start;
                            ev.end = end;
                            ev.all_day = all_day;
                            ev.recurrence = recurrence;
                            ev.reminders = reminders;
                            ev.color = color;
                            ev.calendar_id = calendar_id;
                            ev.location = location_opt;
                            ev.notes = notes;
                            let _ = cal_store::update_event(&s.db, &ev);
                        }
                    }
                    None => {
                        let mut ev = Event::new(final_title, start, end);
                        ev.all_day = all_day;
                        ev.recurrence = recurrence;
                        ev.reminders = reminders;
                        ev.color = color;
                        ev.calendar_id = calendar_id;
                        ev.location = location_opt;
                        ev.notes = notes;
                        let _ = cal_store::create_event(&s.db, &ev);
                    }
                }
            }
            ui2.set_show_dialog(false);
            refresh_all(&st, &ui2);
            toast(&ui2, "Event saved");
        });
    }
    {
        let ui2 = ui.clone_strong();
        ui.on_delete_event(move || {
            ui2.set_show_delete(true);
        });
    }
    {
        let ui2 = ui.clone_strong();
        let st = state.clone();
        ui.on_delete_confirmed(move || {
            let id = st.borrow().editing_id;
            if let Some(id) = id {
                let s = st.borrow();
                let _ = cal_store::delete_event(&s.db, id);
            }
            st.borrow_mut().editing_id = None;
            ui2.set_show_delete(false);
            ui2.set_show_dialog(false);
            refresh_all(&st, &ui2);
            toast(&ui2, "Event deleted");
        });
    }
    {
        let ui2 = ui.clone_strong();
        ui.on_reminder_toggled(move |index| {
            let mut list: Vec<ReminderOption> = ui2.get_dialog_reminders().iter().collect();
            if let Some(item) = list.get_mut(index as usize) {
                item.on = !item.on;
            }
            ui2.set_dialog_reminders(ModelRc::new(VecModel::from(list)));
        });
    }
    {
        let ui2 = ui.clone_strong();
        ui.on_color_picked(move |index| {
            let list: Vec<EventColor> = ui2
                .get_dialog_colors()
                .iter()
                .enumerate()
                .map(|(i, c)| EventColor { color: c.color, on: i == index as usize })
                .collect();
            ui2.set_dialog_colors(ModelRc::new(VecModel::from(list)));
        });
    }
    {
        let ui2 = ui.clone_strong();
        ui.on_calendar_picked(move |index| {
            ui2.set_dialog_calendar(index);
        });
    }
    {
        let ui2 = ui.clone_strong();
        ui.on_recurrence_picked(move |index| {
            ui2.set_dialog_recurrence(index);
        });
    }

    // ---------- reminders + clock ----------
    {
        let ui2 = ui.clone_strong();
        let st = state.clone();
        ui.on_reminder_tick(move || {
            // Keep now-line fresh in timeline views
            ui2.set_now_min(minutes_since_midnight(Utc::now()) as i32);
            for (id, occ, kind, title) in due_reminders(&st) {
                let key = (id, occ.timestamp(), kind as usize);
                if st.borrow_mut().fired_reminders.insert(key) {
                    let before = kind.minutes_before();
                    let msg = if before == 0 {
                        format!("Starting now — {title}")
                    } else {
                        format!("{title} starts in {before} minutes")
                    };
                    ui2.set_reminder_text(msg.into());
                    ui2.set_reminder_visible(true);
                }
            }
            if st.borrow().fired_reminders.len() > 200 {
                st.borrow_mut().fired_reminders.clear();
            }
        });
    }
    {
        let ui2 = ui.clone_strong();
        ui.on_dismiss_reminder(move || {
            ui2.set_reminder_visible(false);
        });
    }

    // Give the window a size that actually fits this screen (handles high-DPI
    // screens where slint's logical units map to fewer physical pixels).
    {
        let ui2 = ui.clone_strong();
        slint::Timer::single_shot(std::time::Duration::from_millis(150), move || {
            let sz = slint::LogicalSize::new(880.0, 600.0);
            ui2.window().set_size(sz);
            eprintln!("[calendar] logical size after set_size: {:?}", ui2.window().size());
        });
    }


    {
        let ui2 = ui.clone_strong();
        slint::Timer::single_shot(std::time::Duration::from_millis(500), move || {
            let sz = ui2.window().size();
            eprintln!(
                "[calendar] 500ms: window size={:?} scale={} root.width-slint-units={}",
                sz,
                ui2.window().scale_factor(),
                ui2.get_debug_width()
            );
        });
    }

    {
        let ui2 = ui.clone_strong();
        slint::Timer::single_shot(std::time::Duration::from_millis(600), move || {
            eprintln!(
                "[calendar] 600ms: size={:?} scale={} root.width={}",
                ui2.window().size(),
                ui2.window().scale_factor(),
                ui2.get_debug_width()
            );
        });
    }

    ui.run()?;
    Ok(())
}


// ---------- Android entry point ----------
#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub fn android_main(app: slint::android::AndroidApp) {
    // Initialize the Slint Android backend before creating any UI.
    slint::android::init(app).unwrap();
    // Android apps use the same UI entry point.
    if let Err(e) = run_ui() {
        eprintln!("[calendar] run_ui failed: {e:?}");
    }
}
