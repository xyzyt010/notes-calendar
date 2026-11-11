
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
