
// ---------- seed data ----------

fn seed_if_empty(state: &Rc<RefCell<AppState>>) {
    let needs_seed = {
        let s = state.borrow();
        !s.seeded
    };
    if !needs_seed {
        return;
    }

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
