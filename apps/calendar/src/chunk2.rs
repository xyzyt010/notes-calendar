
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
                        title: ev.title.into(),
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
