mod sync_ui;
mod ui;

use chrono::{Datelike, Local, Months, NaiveDate};
use kal_notify::{DesktopNotifier, ReminderScheduler as _, ThreadScheduler};
use std::sync::Arc;
use kal_core::models::{Calendar, CalendarItem, Color};
use kal_storage::Database;
use dioxus::prelude::*;
/// App-wide shared handle to the reminder scheduler.
pub type SchedulerHandle = Arc<ThreadScheduler<DesktopNotifier>>;

/// App-wide shared handle to the SQLite database.
pub type DbHandle = Arc<Database>;

pub fn default_db_path() -> Option<std::path::PathBuf> {
    dirs_next::data_dir().map(|d| d.join("kal").join("calendar.db"))
}

fn open_db() -> DbHandle {
    match default_db_path() {
        Some(p) => {
            if let Some(parent) = p.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            Arc::new(Database::open(&p).expect("failed to open calendar database"))
        }
        None => Arc::new(Database::open_in_memory().expect("failed to open in-memory database")),
    }
}

/// Ensure the default "Personal" and auto-created "Birthdays" calendars exist.
fn ensure_default_calendars(db: &Database) -> Vec<Calendar> {
    if db.list_calendars().unwrap_or_default().is_empty() {
        db.upsert_calendar(&Calendar::local("Personal", Color("#3366cc".into())))
            .ok();
        db.upsert_calendar(&Calendar {
            id: ulid::Ulid::new(),
            name: "Birthdays".into(),
            color: Color("#e91e63".into()),
            source: kal_core::models::CalendarSource::Birthdays,
            visible: true,
            updated_at: Local::now().fixed_offset(),
        })
        .ok();
    }
    db.list_calendars().unwrap_or_default()
}

fn main() {
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    use_context_provider::<DbHandle>(open_db);

    let db = use_context::<DbHandle>();
    let db_seed = db.clone();
    let db_items = db.clone();
    let cal_res = use_resource(move || {
        let db = db_seed.clone();
        async move { ensure_default_calendars(&db) }
    });
    // Shared calendars list, refreshed by rows/sidebar after mutations.
    let db_cals = db.clone();
    let mut calendars_res = use_resource(move || {
        let db = db_cals.clone();
        async move { db.list_calendars().unwrap_or_default() }
    });
    use_context_provider(|| calendars_res);

    let today: NaiveDate = Local::now().date_naive();
    use_context_provider(|| Signal::new(today));
    use_context_provider(|| Signal::new(ui::ViewMode::Month));
    let editor: Signal<Option<ui::EditorState>> = Signal::new(None);
    use_context_provider(|| editor);

    // Single shared item list; views read it via context and restart it after
    // mutations.
    let mut items_res = use_resource(move || {
        let db = db_items.clone();
        async move { db.list_items(false).unwrap_or_default() }
    });
    use_context_provider(|| items_res);

    // Reminder scheduler shared app-wide; reconciled in the effect below.
    let scheduler: SchedulerHandle = Arc::new(ThreadScheduler::new(DesktopNotifier));
    use_context_provider(|| scheduler);

    // Reload once default calendars are ensured.
    use_effect(move || {
        if cal_res.value().read().is_some() {
            calendars_res.restart();
            items_res.restart();
        }
    });

    // After a sync merge (sync_ui bumps this), reload everything.
    let mut cal_res_sync = calendars_res;
    let db_dirty = db.clone();
    use_effect(move || {
        if *sync_ui::RESOURCES_DIRTY.read() > 0 {
            cal_res_sync.restart();
            let _ = &db_dirty;
            items_res.restart();
        }
    });

    // Reconcile scheduled reminders whenever items change (§5.3: reschedule
    // on foreground / after mutations / after sync merges).
    let db_sched = db.clone();
    let sched = use_context::<SchedulerHandle>();
    let mut last_items_len = use_signal(|| 0usize);
    use_effect(move || {
        let version = items_res.value().read().as_ref().map(|v| v.len()).unwrap_or(0);
        if version != *last_items_len.read() {
            last_items_len.set(version);
            if let Some(items) = items_res.value().read().as_ref() {
                let from = Local::now().fixed_offset();
                let firings =
                    kal_core::reminders::compute_firings(items, from, 14);
                sched.reschedule(&firings);
                let _ = &db_sched; // reserved: per-item deep-link payload later
            }
        }
    });

    let theme = use_signal(|| String::from("light"));
    let css = include_str!("../assets/main.css");

    rsx! {
        div { class: "app", "data-theme": "{theme}",
            style { dangerous_inner_html: "{css}" }

            TopBar { theme }
            div { class: "body",
                Sidebar {}
                Content {}
            }

            match editor.read().clone() {
                Some(state) => rsx! { ui::EditorModal { state } },
                None => rsx! {},
            }
        }
    }
}

#[component]
fn TopBar(mut theme: Signal<String>) -> Element {
    let dark = theme() == "dark";
    rsx! {
        header { class: "topbar",
            h1 { "Kal" }
            span { class: "subtitle", "{ui::today_line()}" }
            div { class: "spacer" }
            button {
                onclick: move |_| theme.set(if dark { "light".into() } else { "dark".into() }),
                {if dark { "Light mode" } else { "Dark mode" }}
            }
        }
    }
}

fn source_label(cal: &Calendar) -> &'static str {
    use kal_core::models::CalendarSource::*;
    match cal.source {
        Local => "",
        GoogleImport => "google",
        IcsImport => "ics",
        Birthdays => "auto",
    }
}

#[component]
fn Sidebar() -> Element {
    let db = use_context::<DbHandle>();
    let mut editor = use_context::<Signal<Option<ui::EditorState>>>();
    let mut items_res = use_context::<Resource<Vec<CalendarItem>>>();

    let mut cal_res = use_context::<Resource<Vec<Calendar>>>();
    let calendars = cal_res.value().read().clone().unwrap_or_default();

    let db_import = db.clone();
    let db_export = db.clone();
    let db_event = db.clone();
    let db_task = db.clone();
    let db_bday = db.clone();

    rsx! {
        aside { class: "sidebar",
            h2 { "Calendars" }
            ul {
                for cal in calendars.iter().cloned() {
                    CalendarRow { key: "{cal.id}", calendar: cal }
                }
            }
            sync_ui::SyncPanel {}
            h2 { "Import / Export" }
            div { style: "display:flex; flex-direction:column; gap:6px;",
                button {
                    onclick: move |_| {
                        let db2 = db_import.clone();
                        if let Some(path) = rfd::FileDialog::new()
                            .add_filter("iCalendar", &["ics"])
                            .pick_file()
                        {
                            match std::fs::read_to_string(path.as_path()) {
                                Ok(text) => import_ics_file(&db2, &text),
                                Err(e) => eprintln!("read failed: {e}"),
                            }
                            cal_res.restart();
                            items_res.restart();
                        }
                    },
                    "Import .ics…"
                }
                button {
                    onclick: move |_| {
                        let db2 = db_export.clone();
                        if let Some(path) = rfd::FileDialog::new()
                            .set_file_name("Kal-export.ics")
                            .save_file()
                        {
                            let cals = db2.list_calendars().unwrap_or_default();
                            let items = db2.list_items(false).unwrap_or_default();
                            let ics = kal_import::export_all(&cals, &items);
                            if let Err(e) = std::fs::write(path.as_path(), ics) {
                                eprintln!("write failed: {e}");
                            }
                        }
                    },
                    "Export all (.ics)"
                }
            }
            h2 { "New item" }
            div { style: "display:flex; flex-direction:column; gap:6px;",
                button {
                    class: "primary",
                    onclick: move |_| editor.set(Some(ui::EditorState::new_kind(&db_event, kal_core::models::ItemKind::Event))),
                    "+ Event"
                }
                button {
                    onclick: move |_| editor.set(Some(ui::EditorState::new_kind(&db_task, kal_core::models::ItemKind::Task))),
                    "+ Task"
                }
                button {
                    onclick: move |_| editor.set(Some(ui::EditorState::new_kind(&db_bday, kal_core::models::ItemKind::Birthday))),
                    "+ Birthday"
                }
            }
        }
    }
}

#[component]
fn Content() -> Element {
    let view = use_context::<Signal<ui::ViewMode>>();
    rsx! {
        main { class: "content",
            ViewNav {}
            match *view.read() {
                ui::ViewMode::Month => rsx! { ui::MonthView {} },
                ui::ViewMode::Week => rsx! { ui::WeekView {} },
                ui::ViewMode::Day => rsx! { ui::DayView {} },
                ui::ViewMode::Agenda => rsx! { ui::AgendaView {} },
            }
        }
    }
}

#[component]
fn ViewNav() -> Element {
    let mut cursor = use_context::<Signal<NaiveDate>>();
    let mut view = use_context::<Signal<ui::ViewMode>>();

    let c = *cursor.read();
    let label = match *view.read() {
        ui::ViewMode::Month => c.format("%B %Y").to_string(),
        ui::ViewMode::Week => {
            let ws = c - chrono::Duration::days(c.weekday().num_days_from_monday() as i64);
            format!("{} – {}", ws.format("%b %d"), (ws + chrono::Duration::days(6)).format("%b %d, %Y"))
        }
        ui::ViewMode::Day => c.format("%A, %B %d %Y").to_string(),
        ui::ViewMode::Agenda => "Agenda — next 30 days".to_string(),
    };

    let step_back = move |_| {
        let v = view.read();
        cursor.set(match *v {
            ui::ViewMode::Day => *cursor.read() - chrono::Duration::days(1),
            ui::ViewMode::Week => *cursor.read() - chrono::Duration::days(7),
            _ => cursor.read().checked_sub_months(Months::new(1)).unwrap_or(*cursor.read()),
        });
    };
    let step_fwd = move |_| {
        let v = view.read();
        cursor.set(match *v {
            ui::ViewMode::Day => *cursor.read() + chrono::Duration::days(1),
            ui::ViewMode::Week => *cursor.read() + chrono::Duration::days(7),
            _ => cursor.read().checked_add_months(Months::new(1)).unwrap_or(*cursor.read()),
        });
    };

    rsx! {
        div { class: "month-nav",
            button { onclick: step_back, "‹" }
            button { onclick: move |_| cursor.set(Local::now().date_naive()), "Today" }
            button { onclick: step_fwd, "›" }
            h2 { "{label}" }
            div { class: "spacer" }
            for m in ui::ViewMode::ALL {
                button {
                    key: "{m:?}",
                    class: if *view.read() == m { "primary" } else { "" },
                    onclick: move |_| view.set(m),
                    "{m.label()}"
                }
            }
        }
    }
}

fn import_ics_file(db: &DbHandle, text: &str) {
    match kal_import::import_ics(text, "Imported") {
        Ok(result) => {
            db.upsert_calendar(&result.calendar).ok();
            for item in &result.items {
                if let Err(e) = db.upsert_item(item) {
                    eprintln!("skipped item {:?}: {e}", item.title);
                }
            }
        }
        Err(e) => eprintln!("import failed: {e}"),
    }
}

#[component]
fn CalendarRow(calendar: Calendar) -> Element {
    let db = use_context::<DbHandle>();
    let mut cal_res = use_context::<Resource<Vec<Calendar>>>();
    let mut items_res = use_context::<Resource<Vec<CalendarItem>>>();

    let key = calendar.id.to_string();
    let color = calendar.color.to_string();
    let name = calendar.name.clone();
    let src = source_label(&calendar);
    let visible = calendar.visible;

    rsx! {
        li {
            key: "{key}",
            input {
                r#type: "checkbox",
                checked: visible,
                title: "Toggle visibility",
                onchange: move |_| {
                    let mut c = calendar.clone();
                    c.visible = !c.visible;
                    let _ = db.upsert_calendar(&c);
                    cal_res.restart();
                    items_res.restart();
                },
            }
            span { class: "dot", style: "background:{color}", "" }
            span { "{name}" }
            small { class: "when", "{src}" }
        }
    }
}
