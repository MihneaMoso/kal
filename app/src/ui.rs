//! UI components for the Chrono desktop shell (phase 2).

use chrono::{Datelike, Local, NaiveDate, NaiveTime, Weekday};
use chrono_core::models::{CalendarItem, ItemKind};
use chrono_core::viewmodel;
use dioxus::prelude::*;
use ulid::Ulid;

use crate::DbHandle;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewMode {
    Month,
    Week,
    Day,
    Agenda,
}

impl ViewMode {
    pub const ALL: [ViewMode; 4] =
        [ViewMode::Month, ViewMode::Week, ViewMode::Day, ViewMode::Agenda];

    pub fn label(&self) -> &'static str {
        match self {
            ViewMode::Month => "Month",
            ViewMode::Week => "Week",
            ViewMode::Day => "Day",
            ViewMode::Agenda => "Agenda",
        }
    }
}

pub fn today_line() -> String {
    Local::now().format("%A, %B %d %Y").to_string()
}

/// Items filtered down to visible calendars — shared by all views.
pub fn use_visible_items() -> Vec<CalendarItem> {
    let items_res = use_context::<Resource<Vec<CalendarItem>>>();
    let cals_res = use_context::<Resource<Vec<chrono_core::models::Calendar>>>();
    let items = items_res.value().read().clone().unwrap_or_default();
    let cals = cals_res.value().read().clone().unwrap_or_default();
    let hidden: std::collections::HashSet<Ulid> =
        cals.iter().filter(|c| !c.visible).map(|c| c.id).collect();
    items.into_iter().filter(|i| !hidden.contains(&i.calendar_id)).collect()
}

fn local_offset() -> chrono::FixedOffset {
    *Local::now().offset()
}

fn parse_when(date: &str, time: &str) -> Option<chrono_core::models::DateTimeTz> {
    let d = NaiveDate::parse_from_str(date, "%Y-%m-%d").ok()?;
    let t = if time.is_empty() {
        NaiveTime::from_hms_opt(0, 0, 0)?
    } else {
        NaiveTime::parse_from_str(time, "%H:%M").ok()?
    };
    d.and_time(t)
        .and_local_timezone(local_offset())
        .single()
        .map(|dt| dt.fixed_offset())
}

// ---------------------------------------------------------------------------
// Views
// ---------------------------------------------------------------------------

#[component]
pub fn MonthView() -> Element {
    let cursor = use_context::<Signal<NaiveDate>>();
    let items = use_visible_items();

    let c = *cursor.read();
    let grid = viewmodel::month_grid(c.year(), c.month(), Weekday::Mon);
    let today = Local::now().date_naive();

    let rows: Vec<(String, Vec<NaiveDate>)> = grid
        .into_iter()
        .enumerate()
        .map(|(w, row)| (format!("{}-{}-{w}", c.year(), c.month()), row))
        .collect();

    rsx! {
        div { class: "month-grid",
            div { class: "month-head",
                for wd in ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"] {
                    div { key: "{wd}", "{wd}" }
                }
            }
            for (row_key, row) in rows {
                div { class: "month-row", key: "{row_key}",
                    for date in row {
                        MonthCell {
                            key: "{date}",
                            date,
                            today,
                            in_month: date.month() == c.month(),
                            items: items.clone(),
                        }
                    }
                }
            }
        }
    }
}

/// A single day cell in the month grid. Owns its context handles so the grid
/// stays a dumb loop over dates.
#[component]
fn MonthCell(
    date: NaiveDate,
    today: NaiveDate,
    in_month: bool,
    items: Vec<CalendarItem>,
) -> Element {
    let mut editor = use_context::<Signal<Option<EditorState>>>();
    let db = use_context::<DbHandle>();
    let db_new = db.clone();

    let class = if date == today {
        "month-cell today"
    } else if !in_month {
        "month-cell other"
    } else {
        "month-cell"
    };
    let occs = viewmodel::items_on_date(&items, date);

    rsx! {
        div {
            class: "{class}",
            onclick: move |_| editor.set(Some(EditorState::new_on(&db_new, date))),
            span { class: "day-num", "{date.day()}" }
            for occ in occs {
                {
                    let item = items.iter().find(|i| i.id == occ.item_id).cloned();
                    match item {
                        Some(it) => rsx! { EventChip { item: it } },
                        None => rsx! {},
                    }
                }
            }
        }
    }
}

#[component]
fn EventChip(item: CalendarItem) -> Element {
    let mut editor = use_context::<Signal<Option<EditorState>>>();
    let db = use_context::<DbHandle>();

    let label = if item.all_day || item.kind == ItemKind::Birthday {
        item.title.clone()
    } else {
        format!("{} {}", item.start.format("%H:%M"), item.title)
    };
    let bg = item
        .color_override
        .as_ref()
        .map(|c| c.to_string())
        .unwrap_or_else(|| "var(--accent)".into());
    let item_id = item.id;

    rsx! {
        div {
            class: "event-chip",
            style: "background:{bg}",
            title: "{label}",
            onclick: move |e| {
                e.stop_propagation();
                editor.set(Some(EditorState::edit_existing(&db, item_id)));
            },
            "{label}"
        }
    }
}

/// One row in day/week/agenda lists.
#[component]
fn ItemRow(date: Option<NaiveDate>, item: CalendarItem) -> Element {
    let mut editor = use_context::<Signal<Option<EditorState>>>();
    let mut items_res = use_context::<Resource<Vec<CalendarItem>>>();
    let db = use_context::<DbHandle>();
    let db_edit = db.clone();
    let toggle_item = item.clone();
    let item_id = item.id;
    let done = item.completed.is_some();
    let is_task = item.kind == ItemKind::Task;

    let when = if item.all_day {
        "all-day".to_string()
    } else {
        format!(
            "{} – {}",
            item.start.format("%H:%M"),
            item.end.map(|e| e.format("%H:%M").to_string()).unwrap_or_default()
        )
    };
    let date_str = date.map(|d| d.format("%a %b %d").to_string()).unwrap_or_default();

    rsx! {
        li {
            key: "{item_id}",
            onclick: move |_| editor.set(Some(EditorState::edit_existing(&db_edit, item_id))),
            if !date_str.is_empty() {
                span { class: "agenda-date", "{date_str}" }
            }
            if is_task {
                input {
                    r#type: "checkbox",
                    class: "task-check",
                    checked: done,
                    onclick: move |e| e.stop_propagation(),
                    onchange: move |_| {
                        let mut it = toggle_item.clone();
                        it.completed = if it.completed.is_some() {
                            None
                        } else {
                            Some(Local::now().fixed_offset())
                        };
                        it.updated_at = Local::now().fixed_offset();
                        let _ = db.upsert_item(&it);
                        items_res.restart();
                    },
                }
            }
            strong { style: if done { "text-decoration:line-through;color:var(--fg-muted)" }, "{item.title}" }
            span { class: "when", "{when}" }
        }
    }
}

#[component]
pub fn DayView() -> Element {
    let cursor = use_context::<Signal<NaiveDate>>();
    let items = use_visible_items();
    let d = *cursor.read();
    let occs = viewmodel::items_on_date(&items, d);

    let rows: Vec<CalendarItem> = occs
        .iter()
        .filter_map(|o| items.iter().find(|i| i.id == o.item_id).cloned())
        .collect();

    rsx! {
        ul { class: "day-list",
            if rows.is_empty() {
                li { class: "empty", "Nothing scheduled." }
            }
            for it in rows {
                ItemRow { key: "{it.id}", date: None, item: it }
            }
        }
    }
}

#[component]
pub fn WeekView() -> Element {
    let cursor = use_context::<Signal<NaiveDate>>();
    let items = use_visible_items();
    let c = *cursor.read();
    let week_start = c - chrono::Duration::days(c.weekday().num_days_from_monday() as i64);

    let days: Vec<(String, Vec<CalendarItem>)> = (0..7)
        .map(|i| {
            let d = week_start + chrono::Duration::days(i);
            let day_items: Vec<CalendarItem> = viewmodel::items_on_date(&items, d)
                .iter()
                .filter_map(|o| items.iter().find(|it| it.id == o.item_id).cloned())
                .collect();
            (d.format("%a %b %d").to_string(), day_items)
        })
        .collect();

    rsx! {
        ul { class: "day-list",
            for (label, day_items) in days {
                li { key: "{label}", style: "flex-direction:column;align-items:stretch;",
                    h3 { style: "font-size:13px;margin:4px 0;", "{label}" }
                    if day_items.is_empty() {
                        span { class: "when", "—" }
                    }
                    for it in day_items {
                        ItemRow { key: "{it.id}", date: None, item: it }
                    }
                }
            }
        }
    }
}

#[component]
pub fn AgendaView() -> Element {
    let items = use_visible_items();
    let today = Local::now().date_naive();
    let entries = viewmodel::agenda_range(&items, today, today + chrono::Duration::days(30));

    let rows: Vec<(NaiveDate, CalendarItem)> = entries
        .iter()
        .take(100)
        .filter_map(|(d, o)| items.iter().find(|i| i.id == o.item_id).cloned().map(|i| (*d, i)))
        .collect();

    rsx! {
        ul { class: "agenda-list",
            if rows.is_empty() {
                li { class: "empty", "Nothing in the next 30 days." }
            }
            for (d, it) in rows {
                ItemRow { key: "{it.id}-{d}", date: Some(d), item: it }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Editor
// ---------------------------------------------------------------------------

/// State driving the create/edit modal.
#[derive(Clone, PartialEq)]
pub struct EditorState {
    pub id: Option<Ulid>,
    pub kind: ItemKind,
    pub title: String,
    pub date: String,
    pub start_time: String,
    pub end_time: String,
    pub all_day: bool,
    pub calendar_id: Ulid,
    pub location: String,
}

impl EditorState {
    /// Blank editor pre-filled for an event on `date`, first visible calendar.
    pub fn new_on(db: &DbHandle, date: NaiveDate) -> Self {
        Self::build(db, ItemKind::Event, date)
    }

    /// Blank editor for `kind` starting today.
    pub fn new_kind(db: &DbHandle, kind: ItemKind) -> Self {
        Self::build(db, kind, Local::now().date_naive())
    }

    fn build(db: &DbHandle, kind: ItemKind, date: NaiveDate) -> Self {
        let calendar_id = db
            .list_calendars()
            .ok()
            .and_then(|cals| cals.into_iter().find(|c| c.visible).map(|c| c.id))
            .unwrap_or_else(Ulid::nil);
        let birthday = kind == ItemKind::Birthday;
        Self {
            id: None,
            kind,
            title: String::new(),
            date: date.format("%Y-%m-%d").to_string(),
            start_time: if birthday { "00:00".into() } else { "09:00".into() },
            end_time: if birthday || kind == ItemKind::Task {
                String::new()
            } else {
                "10:00".into()
            },
            all_day: birthday,
            calendar_id,
            location: String::new(),
        }
    }

    pub fn edit_existing(db: &DbHandle, id: Ulid) -> Self {
        let start = Local::now().fixed_offset();
        let blank = || CalendarItem::new(ItemKind::Event, "", Ulid::nil(), start);
        let item = db.get_item(id).ok().flatten().unwrap_or_else(blank);
        Self {
            id: Some(item.id),
            kind: item.kind,
            title: item.title,
            date: item.start.date_naive().format("%Y-%m-%d").to_string(),
            start_time: item.start.format("%H:%M").to_string(),
            end_time: item
                .end
                .map(|e| e.format("%H:%M").to_string())
                .unwrap_or_default(),
            all_day: item.all_day,
            calendar_id: item.calendar_id,
            location: item.location.unwrap_or_default(),
        }
    }
}

#[component]
pub fn EditorModal(state: EditorState) -> Element {
    let db = use_context::<DbHandle>();
    let mut editor = use_context::<Signal<Option<EditorState>>>();
    let mut items_res = use_context::<Resource<Vec<CalendarItem>>>();

    let calendars = db.list_calendars().unwrap_or_default();
    let db_save = db.clone();
    let db_delete = db.clone();

    let init_title = state.title.clone();
    let init_date = state.date.clone();
    let init_start = state.start_time.clone();
    let init_end = state.end_time.clone();
    let init_location = state.location.clone();
    let init_person = state
        .id
        .and_then(|id| db.get_item(id).ok().flatten())
        .and_then(|i| i.metadata.birthday_of)
        .unwrap_or_default();

    let mut title = use_signal(move || init_title);
    let mut date = use_signal(move || init_date);
    let mut start_time = use_signal(move || init_start);
    let mut end_time = use_signal(move || init_end);
    let mut all_day = use_signal(move || state.all_day);
    let mut calendar_id = use_signal(move || state.calendar_id);
    let mut location = use_signal(move || init_location);
    let mut person = use_signal(move || init_person);

    let save = move |_| {
        let Some(start) = parse_when(&date.read(), &start_time.read()) else {
            return;
        };
        let end = parse_when(&date.read(), &end_time.read());

        // Load-or-create base item so editing preserves fields not shown here
        // (reminders, rrule, …) once later phases introduce them.
        let mut item = match state.id {
            Some(id) => db_save.get_item(id).ok().flatten().unwrap_or_else(|| {
                CalendarItem::new(state.kind, "", *calendar_id.read(), start)
            }),
            None => CalendarItem::new(state.kind, "", *calendar_id.read(), start),
        };

        item.kind = state.kind;
        item.title = title.read().clone();
        item.location = {
            let l = location.read().clone();
            if l.is_empty() { None } else { Some(l) }
        };
        item.calendar_id = *calendar_id.read();
        item.start = start;
        item.end = end;
        item.all_day = *all_day.read();
        if state.kind == ItemKind::Birthday {
            item.metadata.birthday_of = Some(person.read().clone());
        }
        item.updated_at = Local::now().fixed_offset();

        if db_save.upsert_item(&item).is_ok() {
            items_res.restart();
            editor.set(None);
        }
    };

    let delete = move |_| {
        if let Some(id) = state.id {
            let _ = db_delete.soft_delete_item(id);
            items_res.restart();
            editor.set(None);
        }
    };

    let kind_is_birthday = state.kind == ItemKind::Birthday;
    let is_edit = state.id.is_some();
    let heading = modal_title(&state);

    rsx! {
        div { class: "modal-backdrop",
            onclick: move |_| editor.set(None),
            div { class: "modal",
                onclick: move |e| e.stop_propagation(),
                h2 { "{heading}" }
                label { "Title"
                    input {
                        r#type: "text",
                        value: "{title}",
                        autofocus: true,
                        oninput: move |e| title.set(e.value()),
                    }
                }
                if kind_is_birthday {
                    label { "Person"
                        input {
                            r#type: "text",
                            value: "{person}",
                            placeholder: "Who?",
                            oninput: move |e| person.set(e.value()),
                        }
                    }
                }
                div { class: "row",
                    label { "Date"
                        input { r#type: "date", value: "{date}",
                            oninput: move |e| date.set(e.value()), }
                    }
                    label { style: "display:flex;align-items:center;gap:4px;margin-top:18px;",
                        input {
                            r#type: "checkbox",
                            checked: *all_day.read(),
                            onchange: move |e| all_day.set(e.checked()),
                        }
                        "All day"
                    }
                }
                if !*all_day.read() {
                    div { class: "row",
                        label { "Start"
                            input { r#type: "time", value: "{start_time}",
                                oninput: move |e| start_time.set(e.value()), }
                        }
                        label { "End"
                            input { r#type: "time", value: "{end_time}",
                                oninput: move |e| end_time.set(e.value()), }
                        }
                    }
                }
                label { "Location (optional)"
                    input {
                        r#type: "text",
                        value: "{location}",
                        oninput: move |e| location.set(e.value()),
                    }
                }
                label { "Calendar"
                    select {
                        onchange: move |e| {
                            if let Ok(id) = Ulid::from_string(&e.value()) {
                                calendar_id.set(id);
                            }
                        },
                        for cal in calendars.iter() {
                            option {
                                key: "{cal.id}",
                                value: "{cal.id}",
                                selected: *calendar_id.read() == cal.id,
                                "{cal.name}"
                            }
                        }
                    }
                }
                div { class: "modal-actions",
                    if is_edit {
                        button { class: "danger", onclick: delete, "Delete" }
                    } else {
                        span {}
                    }
                    div { style: "display:flex;gap:8px",
                        button { onclick: move |_| editor.set(None), "Cancel" }
                        button { class: "primary", onclick: save, "Save" }
                    }
                }
            }
        }
    }
}

fn modal_title(state: &EditorState) -> String {
    match (state.id.is_some(), state.kind) {
        (true, ItemKind::Event) => "Edit event".into(),
        (true, ItemKind::Task) => "Edit task".into(),
        (true, ItemKind::Birthday) => "Edit birthday".into(),
        (false, k) => format!(
            "New {}",
            match k {
                ItemKind::Event => "event",
                ItemKind::Task => "task",
                ItemKind::Birthday => "birthday",
            }
        ),
    }
}
