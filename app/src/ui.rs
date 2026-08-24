//! UI components for the Kal desktop shell (phase 2).

use chrono::{Datelike, Local, NaiveDate, NaiveTime, Weekday};
use kal_core::models::{CalendarItem, DateTimeTz, ItemKind, Occurrence};
use kal_core::viewmodel;
use dioxus::prelude::*;
use std::collections::BTreeMap;
use ulid::Ulid;

use crate::DbHandle;

/// Device-local preferences (§5.7), persisted in the `settings` table.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Settings {
    pub theme: String,            // "light" | "dark"
    pub time_24h: bool,
    pub first_day_monday: bool,
    pub default_view: ViewMode,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: "light".into(),
            time_24h: true,
            first_day_monday: true,
            default_view: ViewMode::Month,
        }
    }
}

impl Settings {
    pub fn load(db: &crate::DbHandle) -> Self {
        db.get_setting("preferences")
            .ok()
            .flatten()
            .and_then(|json| serde_json::from_str(&json).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, db: &crate::DbHandle) {
        if let Ok(json) = serde_json::to_string(self) {
            let _ = db.set_setting("preferences", &json);
        }
    }

    pub fn fmt_time(&self, dt: kal_core::models::DateTimeTz) -> String {
        if self.time_24h {
            dt.format("%H:%M").to_string()
        } else {
            dt.format("%l:%M %p").to_string().trim().to_string()
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ViewMode {
    Month,
    Week,
    Day,
    Agenda,
}

impl ViewMode {
    pub const ALL: [ViewMode; 4] =
        [ViewMode::Month, ViewMode::Week, ViewMode::Day, ViewMode::Agenda];

    pub fn label(&self) -> String {
        match self {
            ViewMode::Month => crate::i18n::tr("view-month"),
            ViewMode::Week => crate::i18n::tr("view-week"),
            ViewMode::Day => crate::i18n::tr("view-day"),
            ViewMode::Agenda => crate::i18n::tr("view-agenda"),
        }
    }
}

pub fn today_line() -> String {
    Local::now().format("%A, %B %d %Y").to_string()
}

/// Items filtered down to visible calendars — shared by all views.
pub fn use_visible_items() -> Vec<CalendarItem> {
    let items_res = use_context::<Resource<Vec<CalendarItem>>>();
    let cals_res = use_context::<Resource<Vec<kal_core::models::Calendar>>>();
    let items = items_res.value().read().clone().unwrap_or_default();
    let cals = cals_res.value().read().clone().unwrap_or_default();
    let hidden: std::collections::HashSet<Ulid> =
        cals.iter().filter(|c| !c.visible).map(|c| c.id).collect();
    items.into_iter().filter(|i| !hidden.contains(&i.calendar_id)).collect()
}

fn local_offset() -> chrono::FixedOffset {
    *Local::now().offset()
}

fn parse_when(date: &str, time: &str) -> Option<kal_core::models::DateTimeTz> {
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
    let settings = use_context::<Signal<Settings>>();
    let items = use_visible_items();

    let st = settings.read();
    let c = *cursor.read();
    let first_day = if st.first_day_monday { Weekday::Mon } else { Weekday::Sun };
    let grid = viewmodel::month_grid(c.year(), c.month(), first_day);
    let today = Local::now().date_naive();

    let first = grid[0][0];
    let last = grid[viewmodel::MONTH_GRID_WEEKS - 1][6];
    let occ_map = viewmodel::occurrences_by_date(&items, first, last);

    let rows: Vec<(String, Vec<(NaiveDate, Vec<Occurrence>)>)> = grid
        .into_iter()
        .enumerate()
        .map(|(w, row)| {
            (
                format!("{}-{}-{w}", c.year(), c.month()),
                row.into_iter().map(|d| {
                    let occs = occ_map.get(&d).cloned().unwrap_or_default();
                    (d, occs)
                }).collect(),
            )
        })
        .collect();

    rsx! {
        div { class: "month-grid",
            div { class: "month-head",
                for wd in weekday_headers(st.first_day_monday) {
                    div { key: "{wd}", "{wd}" }
                }
            }
            for (row_key, row) in rows {
                div { class: "month-row", key: "{row_key}",
                    for (date, cell_occs) in row {
                        MonthCell {
                            key: "{date}",
                            date,
                            today,
                            in_month: date.month() == c.month(),
                            occurrences: cell_occs,
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
    occurrences: Vec<Occurrence>,
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
    rsx! {
        div {
            class: "{class}",
            onclick: move |_| editor.set(Some(EditorState::new_on(&db_new, date))),
            span { class: "day-num", "{date.day()}" }
            for occ in occurrences.iter().cloned() {
                {
                    let item = items.iter().find(|i| i.id == occ.item_id).cloned();
                    match item {
                        Some(it) => rsx! { EventChip { item: it, occ_start: occ.start } },
                        None => rsx! {},
                    }
                }
            }
        }
    }
}

#[component]
fn EventChip(item: CalendarItem, occ_start: DateTimeTz) -> Element {
    let mut editor = use_context::<Signal<Option<EditorState>>>();
    let db = use_context::<DbHandle>();

    let settings = use_context::<Signal<Settings>>();
    let time_str = settings.read().fmt_time(item.start);
    let label = if item.all_day || item.kind == ItemKind::Birthday {
        item.title.clone()
    } else {
        format!("{time_str} {}", item.title)
    };
    let bg = item
        .color_override
        .as_ref()
        .map(|c| c.to_string())
        .unwrap_or_else(|| "var(--accent)".into());
    let item_id = item.id;
    // Birthday age badge (§5.1): computed against this occurrence's date.
    let chip_label = if item.kind == ItemKind::Birthday {
        match item.birthday_age_at(occ_start) {
            Some(age) => format!("{label} · {age}"),
            None => label,
        }
    } else {
        label
    };

    rsx! {
        div {
            class: "event-chip",
            style: "background:{bg}",
            title: "{chip_label}",
            onclick: move |e| {
                e.stop_propagation();
                editor.set(Some(EditorState::edit_existing(&db, item_id, Some(occ_start))));
            },
            "{chip_label}"
        }
    }
}

/// One row in day/week/agenda lists.
#[component]
fn ItemRow(date: Option<NaiveDate>, item: CalendarItem, occ_start: DateTimeTz) -> Element {
    let settings = use_context::<Signal<Settings>>();
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
            settings.read().fmt_time(item.start),
            item.end.map(|e| settings.read().fmt_time(e)).unwrap_or_default()
        )
    };
    let date_str = date.map(|d| d.format("%a %b %d").to_string()).unwrap_or_default();
    // Age shown for birthdays next to the title.
    let title_suffix = if item.kind == ItemKind::Birthday {
        match item.birthday_age_at(occ_start) {
            Some(age) => format!(" · {age}"),
            None => String::new(),
        }
    } else {
        String::new()
    };

    rsx! {
        li {
            key: "{item_id}",
            onclick: move |_| editor.set(Some(EditorState::edit_existing(&db_edit, item_id, Some(occ_start)))),
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
            strong { style: if done { "text-decoration:line-through;color:var(--fg-muted)" }, "{item.title}{title_suffix}" }
            span { class: "when", "{when}" }
        }
    }
}

#[component]
pub fn DayView() -> Element {
    let cursor = use_context::<Signal<NaiveDate>>();
    let items = use_visible_items();
    let d = *cursor.read();
    let empty = BTreeMap::new();
    let map = if items.is_empty() {
        &empty
    } else {
        &viewmodel::occurrences_by_date(&items, d, d)
    };
    let rows: Vec<(Occurrence, CalendarItem)> = map
        .get(&d)
        .map(|occs| {
            occs.iter()
                .filter_map(|o| items.iter().find(|i| i.id == o.item_id).cloned().map(|i| (o.clone(), i)))
                .collect()
        })
        .unwrap_or_default();

    rsx! {
        ul { class: "day-list",
            if rows.is_empty() {
                li { class: "empty", "Nothing scheduled." }
            }
            for (occ, it) in rows {
                ItemRow { key: "{it.id}-{occ.start}", date: None, item: it, occ_start: occ.start }
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
    let map = viewmodel::occurrences_by_date(&items, week_start, week_start + chrono::Duration::days(6));

    let days: Vec<(String, Vec<(Occurrence, CalendarItem)>)> = (0..7)
        .map(|i| {
            let d = week_start + chrono::Duration::days(i);
            let day_items: Vec<(Occurrence, CalendarItem)> = map
                .get(&d)
                .map(|occs| {
                    occs.iter()
                        .filter_map(|o| items.iter().find(|it| it.id == o.item_id).cloned().map(|it| (o.clone(), it)))
                        .collect()
                })
                .unwrap_or_default();
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
                    for (occ, it) in day_items {
                        ItemRow { key: "{it.id}-{occ.start}", date: None, item: it, occ_start: occ.start }
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
    let map = viewmodel::occurrences_by_date(&items, today, today + chrono::Duration::days(30));
    let mut rows: Vec<(NaiveDate, Occurrence, CalendarItem)> = Vec::new();
    for (d, occs) in &map {
        for o in occs {
            if let Some(it) = items.iter().find(|i| i.id == o.item_id).cloned() {
                rows.push((*d, o.clone(), it));
            }
        }
    }
    rows.truncate(100);

    rsx! {
        ul { class: "agenda-list",
            if rows.is_empty() {
                li { class: "empty", "Nothing in the next 30 days." }
            }
            for (d, occ, it) in rows {
                ItemRow { key: "{it.id}-{occ.start}-{d}", date: Some(d), item: it, occ_start: occ.start }
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
    /// Start instant of the clicked occurrence (differs from item.start for
    /// recurring series) — drives per-instance edit scoping.
    pub occurrence_start: Option<DateTimeTz>,
    pub kind: ItemKind,
    pub title: String,
    pub date: String,
    pub start_time: String,
    pub end_time: String,
    pub all_day: bool,
    pub calendar_id: Ulid,
    pub location: String,
    /// Editor-side recurrence choice; expanded into an RRULE string on save.
    pub rrule_preset: RrulePreset,
    /// Selected reminder offsets in minutes before start.
    pub reminder_minutes: Vec<i64>,
}

/// Default reminder presets offered in the editor (§5.3).
pub const REMINDER_PRESETS: [i64; 5] = [10, 30, 60, 1440, 10080];

pub fn preset_label(minutes: i64) -> &'static str {
    match minutes {
        10 => "10 min",
        30 => "30 min",
        60 => "1 hour",
        1440 => "1 day",
        10080 => "1 week",
        _ => "custom",
    }
}

/// Simplified recurrence picker (full custom RRULE editing comes later).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RrulePreset {
    None,
    Daily,
    Weekly,
    Monthly,
    Yearly,
}

impl RrulePreset {
    fn to_rrule(self) -> Option<&'static str> {
        match self {
            RrulePreset::None => None,
            RrulePreset::Daily => Some("FREQ=DAILY"),
            RrulePreset::Weekly => Some("FREQ=WEEKLY"),
            RrulePreset::Monthly => Some("FREQ=MONTHLY"),
            RrulePreset::Yearly => Some("FREQ=YEARLY"),
        }
    }

    const ALL: [RrulePreset; 5] = [
        RrulePreset::None,
        RrulePreset::Daily,
        RrulePreset::Weekly,
        RrulePreset::Monthly,
        RrulePreset::Yearly,
    ];

    fn label(self) -> &'static str {
        match self {
            RrulePreset::None => "Doesn't repeat",
            RrulePreset::Daily => "Daily",
            RrulePreset::Weekly => "Weekly",
            RrulePreset::Monthly => "Monthly",
            RrulePreset::Yearly => "Annually",
        }
    }
}

fn rrule_to_preset(rule: Option<&str>) -> RrulePreset {
    match rule.unwrap_or("") {
        r if r.starts_with("FREQ=DAILY") => RrulePreset::Daily,
        r if r.starts_with("FREQ=WEEKLY") => RrulePreset::Weekly,
        r if r.starts_with("FREQ=MONTHLY") => RrulePreset::Monthly,
        r if r.starts_with("FREQ=YEARLY") => RrulePreset::Yearly,
        _ => RrulePreset::None,
    }
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
            occurrence_start: None,
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
            rrule_preset: if kind == ItemKind::Birthday { RrulePreset::Yearly } else { RrulePreset::None },
            reminder_minutes: Vec::new(),
        }
    }

    pub fn edit_existing(db: &DbHandle, id: Ulid, occ_start: Option<DateTimeTz>) -> Self {
        let start = Local::now().fixed_offset();
        let blank = || CalendarItem::new(ItemKind::Event, "", Ulid::nil(), start);
        let item = db.get_item(id).ok().flatten().unwrap_or_else(blank);
        let reminder_minutes = item
            .reminders
            .iter()
            .filter_map(|r| match r.offset {
                kal_core::models::ReminderOffset::MinutesBefore { minutes } => Some(minutes),
                _ => None,
            })
            .collect();
        Self {
            id: Some(item.id),
            occurrence_start: occ_start,
            rrule_preset: rrule_to_preset(item.rrule.as_deref()),
            reminder_minutes,
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

/// Edit scope for recurring-series changes (Google Calendar semantics).
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    All,
    ThisOnly,
    ThisAndFollowing,
}

/// Apply a scoped edit to a recurring series.
///
/// - `ThisOnly`: EXDATE the original occurrence out of the base series and
///   create a new single item with the edited values.
/// - `ThisAndFollowing`: truncate the base series with an UNTIL just before
///   `occ_start` and create a new series starting at the edited occurrence.
///
/// Returns true on success (all writes applied).
fn db_apply_scoped_edit(
    db: &DbHandle,
    base_id: Ulid,
    edited: &CalendarItem,
    occ_start: DateTimeTz,
    and_following: bool,
) -> bool {
    let now = Local::now().fixed_offset();
    let Some(mut base) = db.get_item(base_id).ok().flatten() else {
        return false;
    };

    if and_following {
        // Truncate base series right before the edited occurrence.
        if !base.rrule.as_deref().unwrap_or("").contains("COUNT=")
            && !base.rrule.as_deref().unwrap_or("").contains("UNTIL=")
        {
            let until_utc = (occ_start - chrono::Duration::minutes(1))
                .with_timezone(&chrono::Utc);
            let until = until_utc.format("%Y%m%dT%H%M%SZ").to_string();
            let rule = base.rrule.take().unwrap_or_default();
            base.rrule = Some(format!("{rule};UNTIL={until}"));
        }
    } else {
        // Exclude just this occurrence from the base series.
        base.exdates.push(occ_start);
    }
    base.updated_at = now;
    if db.upsert_item(&base).is_err() {
        return false;
    }

    // New item/series carries the edited values.
    let mut new_item = edited.clone();
    new_item.id = Ulid::new();
    new_item.created_at = now;
    new_item.updated_at = now;
    if !and_following {
        new_item.rrule = None;
        new_item.exdates.clear();
    }
    db.upsert_item(&new_item).is_ok()
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
    let mut rrule_choice = use_signal(move || state.rrule_preset);
    let init_reminders = state.reminder_minutes.clone();
    let mut reminder_minutes = use_signal(move || init_reminders);
    let mut custom_reminder = use_signal(String::new);

    // Editing a recurring series instance? Then saving needs a scope choice.
    let is_recurring_edit = state
        .id
        .and_then(|id| db.get_item(id).ok().flatten())
        .and_then(|i| i.rrule)
        .is_some();

    // Signal captures are Copy in Dioxus, so we build one handler per scope.
    // Signal captures are Copy in Dioxus; each scope gets its own handler
    // with its own DB handle clone.
    let mk_save = |scope: Scope, db_save: DbHandle| move |_| {
        let Some(start) = parse_when(&date.read(), &start_time.read()) else {
            return;
        };
        let end = parse_when(&date.read(), &end_time.read());

        // Load-or-create base item so editing preserves fields not shown here
        // (reminders, …) once later phases introduce them.
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
        item.rrule = rrule_choice.read().to_rrule().map(str::to_string);
        item.reminders = {
            let mut mins = reminder_minutes.read().clone();
            if let Ok(m) = custom_reminder.read().trim().parse::<i64>() {
                if m > 0 && !mins.contains(&m) {
                    mins.push(m);
                }
            }
            mins.sort();
            mins.dedup();
            mins.into_iter()
                .map(kal_core::models::Reminder::minutes_before)
                .collect()
        };
        item.updated_at = Local::now().fixed_offset();

        let and_following = scope == Scope::ThisAndFollowing;
        match (scope, state.id, state.occurrence_start) {
            (Scope::ThisOnly | Scope::ThisAndFollowing, Some(base_id), Some(occ_start)) => {
                if !db_apply_scoped_edit(&db_save, base_id, &item, occ_start, and_following) {
                    return;
                }
            }
            _ => {
                if db_save.upsert_item(&item).is_err() {
                    return;
                }
            }
        }

        items_res.restart();
        editor.set(None);
    };
    let save_all = mk_save(Scope::All, db_save.clone());
    let save_following = mk_save(Scope::ThisAndFollowing, db_save.clone());
    let save_this_only = mk_save(Scope::ThisOnly, db_save);

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
            div {
                class: "modal",
                role: "dialog",
                "aria-modal": true,
                "aria-label": "{heading}",
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
                label { "Remind me"
                    div { style: "display:flex;gap:6px;flex-wrap:wrap;",
                        for m in REMINDER_PRESETS {
                            {
                                let active = reminder_minutes.read().contains(&m);
                                rsx! {
                                    button {
                                        key: "{m}",
                                        class: if active { "primary" } else { "" },
                                        style: "font-size:12px;padding:2px 8px;",
                                        onclick: move |_| {
                                            let mut list = reminder_minutes.read().clone();
                                            if list.contains(&m) {
                                                list.retain(|x| *x != m);
                                            } else {
                                                list.push(m);
                                            }
                                            reminder_minutes.set(list);
                                        },
                                        "{preset_label(m)}"
                                    }
                                }
                            }
                        }
                    }
                }
                label { "Custom reminder (minutes before)"
                    input {
                        r#type: "text",
                        value: "{custom_reminder}",
                        placeholder: "e.g. 45",
                        oninput: move |e| custom_reminder.set(e.value()),
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
                label { "Repeat"
                    select {
                        onchange: move |e| {
                            match e.value().as_str() {
                                "daily" => rrule_choice.set(RrulePreset::Daily),
                                "weekly" => rrule_choice.set(RrulePreset::Weekly),
                                "monthly" => rrule_choice.set(RrulePreset::Monthly),
                                "yearly" => rrule_choice.set(RrulePreset::Yearly),
                                _ => rrule_choice.set(RrulePreset::None),
                            }
                        },
                        for p in RrulePreset::ALL {
                            option {
                                key: "{p:?}",
                                value: "{preset_value(p)}",
                                selected: *rrule_choice.read() == p,
                                "{p.label()}"
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
                    div { style: "display:flex;gap:8px;flex-wrap:wrap;",
                        button { onclick: move |_| editor.set(None), "Cancel" }
                        if is_recurring_edit {
                            button { class: "primary", onclick: save_all, "All events" }
                            button { onclick: save_following, "…and following" }
                            button { class: "primary", onclick: save_this_only, "This event" }
                        } else {
                            button { class: "primary", onclick: save_all, "Save" }
                        }
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

fn preset_value(p: RrulePreset) -> &'static str {
    match p {
        RrulePreset::None => "none",
        RrulePreset::Daily => "daily",
        RrulePreset::Weekly => "weekly",
        RrulePreset::Monthly => "monthly",
        RrulePreset::Yearly => "yearly",
    }
}

fn weekday_headers(monday_first: bool) -> [&'static str; 7] {
    if monday_first {
        ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"]
    } else {
        ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"]
    }
}
