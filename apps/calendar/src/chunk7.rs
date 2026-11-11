// ---------- entry point ----------

pub fn run() -> anyhow::Result<()> {
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
    drop(state);

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
                st.borrow_mut().calendars = cal_store::list_calendars(&st.borrow().db).unwrap_or_default();
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

    ui.run()?;
    Ok(())
}

use chrono::SubsecRound;
