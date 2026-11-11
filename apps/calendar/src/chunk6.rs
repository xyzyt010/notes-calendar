
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
