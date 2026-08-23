use std::sync::Arc;

use chrono_core::models::{Calendar, CalendarItem, Color};
use chrono_storage::Database;
use dioxus::prelude::*;

/// App-wide shared handle to the SQLite database.
pub type DbHandle = Arc<Database>;

fn default_db_path() -> Option<std::path::PathBuf> {
    dirs_next::data_dir().map(|d| d.join("chrono").join("calendar.db"))
}

fn open_db() -> DbHandle {
    let path = default_db_path();
    match path {
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
            source: chrono_core::models::CalendarSource::Birthdays,
            visible: true,
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
    let db_for_seed = db.clone();
    let calendars = use_resource(move || {
        let db = db_for_seed.clone();
        async move { ensure_default_calendars(&db) }
    });

    match calendars.value().read().clone() {
        Some(cals) => rsx! { Shell { calendars: cals } },
        None => rsx! { div { class: "loading", "Loading…" } },
    }
}

#[component]
fn Shell(calendars: Vec<Calendar>) -> Element {
    let db = use_context::<DbHandle>();
    let db_items = db.clone();
    let items = use_resource(move || {
        let db = db_items.clone();
        async move {
            let from = chrono::Utc::now().fixed_offset();
            let to = from + chrono::Duration::days(365);
            db.items_in_range(from, to).unwrap_or_default()
        }
    });
    let items = items.value().read().clone().unwrap_or_default();

    rsx! {
        div { class: "app",
            header { class: "topbar",
                h1 { "Chrono" }
                span { class: "subtitle", "local-first calendar" }
            }
            div { class: "body",
                aside { class: "sidebar",
                    h2 { "Calendars" }
                    ul {
                        for cal in calendars.iter() {
                            li { key: "{cal.id}",
                                span { class: "dot", style: "background:{cal.color}", "" }
                                "{cal.name}"
                            }
                        }
                    }
                }
                main { class: "content",
                    h2 { "Upcoming" }
                    if items.is_empty() {
                        p { class: "empty", "No events yet — CRUD lands in phase 2." }
                    } else {
                        ul { class: "agenda",
                            for item in items.iter() {
                                AgendaRow { item: item.clone() }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn AgendaRow(item: CalendarItem) -> Element {
    let when = item.start.format("%Y-%m-%d %H:%M").to_string();
    rsx! {
        li { class: "agenda-row", key: "{item.id}",
            strong { "{item.title}" }
            span { class: "when", "{when}" }
        }
    }
}
