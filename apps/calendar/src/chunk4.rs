
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
