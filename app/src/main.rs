mod i18n;
mod mini_widget;
mod sync_ui;
mod ui;

use chrono::{Datelike, Local, Months, NaiveDate};
use dioxus::prelude::*;
use dioxus_desktop::{Config, WindowBuilder};
use kal_core::models::{Calendar, CalendarItem, Color};
use kal_notify::{DesktopNotifier, ReminderScheduler as _, ThreadScheduler};
use kal_storage::Database;
use std::sync::Arc;
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
    // Frameless: hide the GTK titlebar/menu entirely (user preference).
    let cfg = Config::new().with_window(
        WindowBuilder::new()
            .with_title("Kal")
            .with_decorations(false)
            .with_maximized(true),
    );
    dioxus::LaunchBuilder::new().with_cfg(cfg).launch(App);
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

    let prefs = ui::Settings::load(&db);
    let default_view = prefs.default_view;

    // Dedicated theme signal: toggling it must NOT re-render the calendar
    // views (they subscribe to `settings`, not `theme`) — this is what keeps
    // the switch instant and immune to rapid clicking.
    let theme = Signal::new(prefs.theme.clone());
    use_context_provider(|| theme);

    // Layout state: a SINGLE signal — Dioxus contexts are keyed by type, so
    // multiple Signal<bool> providers would alias each other.
    use_context_provider(|| {
        Signal::new(ui::UiLayout {
            sidebar_open: true,
            sidebar_width: 230,
            ..Default::default()
        })
    });

    let today: NaiveDate = Local::now().date_naive();
    use_context_provider(|| Signal::new(today));
    let settings = Signal::new(prefs);
    use_context_provider(|| settings);
    use_context_provider(|| Signal::new(default_view));
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
        let version = items_res
            .value()
            .read()
            .as_ref()
            .map(|v| v.len())
            .unwrap_or(0);
        if version != *last_items_len.read() {
            last_items_len.set(version);
            if let Some(items) = items_res.value().read().as_ref() {
                let from = Local::now().fixed_offset();
                let firings = kal_core::reminders::compute_firings(items, from, 14);
                sched.reschedule(&firings);
                let _ = &db_sched; // reserved: per-item deep-link payload later
            }
        }
    });

    let css = include_str!("../assets/main.css");
    // Whole-document theming: variables are injected at :root AFTER the base
    // stylesheet so every element (html/body included) follows the theme.
    // Direct read (not use_memo): App re-renders on theme change and rebuilds
    // the string synchronously — no memo evaluation timing involved.
    let dark = theme.read().as_str() == "dark";
    let themed_css = if dark {
        format!("{css}{DARK_THEME_VARS}")
    } else {
        css.to_string()
    };

    // Sidebar resize dragging (root-level so fast cursor movement can't
    // escape the handle).
    let mut layout = use_context::<Signal<ui::UiLayout>>();
    let app_class = if layout.read().sidebar_resizing {
        "app resizing"
    } else {
        "app"
    };
    let on_root_mousemove = move |e: Event<MouseData>| {
        if layout.read().sidebar_resizing {
            let x = e.client_coordinates().x as u32;
            layout.write().sidebar_width = x.clamp(170, 480);
        }
    };
    let end_resize = move |_| layout.write().sidebar_resizing = false;

    rsx! {
        div {
            class: "{app_class}",
            onmousemove: on_root_mousemove,
            onmouseup: end_resize,
            onmouseleave: end_resize,
            style { dangerous_inner_html: "{themed_css}" }

            TopBar {}
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

const DARK_THEME_VARS: &str = r#"
:root {
    --select-chevron: url("data:image/svg+xml;charset=utf-8,%3Csvg xmlns='http://www.w3.org/2000/svg' width='10' height='6'%3E%3Cpath d='M1 1l4 4 4-4' stroke='%239aa4ae' stroke-width='1.5' fill='none'/%3E%3C/svg%3E");
    --bg: #16191d;
    --bg-alt: #1f2329;
    --fg: #e6e8eb;
    --fg-muted: #9aa4ae;
    --accent: #6c99f0;
    --border: #33393f;
    --today: #22304a;
}
"#;

#[component]
fn TopBar() -> Element {
    let mut layout = use_context::<Signal<ui::UiLayout>>();

    rsx! {
        header { class: "topbar",
            button {
                class: "icon-btn hamburger",
                "aria-label": "Toggle sidebar",
                aria_expanded: "{layout.read().sidebar_open}",
                onclick: move |_| {
                    let cur = layout.read().sidebar_open;
                    layout.write().sidebar_open = !cur;
                },
                "\u{2630}"
            }
            h1 { "{i18n::tr(\"app-title\")}" }
            span { class: "subtitle", "{subtitle_label()}" }
            div { class: "spacer" }
            nav { class: "tb-controls", "aria-label": "Preferences",
                PreferencesControls {}
            }
            button {
                class: "icon-btn prefs-toggle",
                "aria-label": "Preferences menu",
                "aria-expanded": "{layout.read().prefs_drawer_open}",
                onclick: move |_| {
                    let cur = layout.read().prefs_drawer_open;
                    layout.write().prefs_drawer_open = !cur;
                },
                "\u{2699}"
            }
        }
        if layout.read().prefs_drawer_open {
            div { class: "prefs-drawer", role: "menu", "aria-label": "Preferences",
                PreferencesControls {}
            }
        }
    }
}

fn subtitle_label() -> String {
    format!("{} \u{2014} {}", i18n::tr("app-subtitle"), ui::today_line())
}

/// The preference selects + theme toggle, rendered either inline in the
/// top bar (wide screens) or inside the drawer (narrow screens).
#[component]
fn PreferencesControls() -> Element {
    let db = use_context::<DbHandle>();
    let mut settings = use_context::<Signal<ui::Settings>>();
    let mut theme = use_context::<Signal<String>>();
    let dark = theme.read().as_str() == "dark";
    let db_time = db.clone();
    let db_week = db.clone();
    let db_view = db.clone();
    let db_theme = db.clone();

    rsx! {
        select {
            title: "Time format",
            value: if settings.read().time_24h { "24h" } else { "12h" },
            onchange: move |e| {
                let mut p = settings.read().clone();
                p.time_24h = e.value() == "24h";
                p.save(&db_time);
                settings.set(p);
            },
            option { value: "24h", selected: settings.read().time_24h, "24-hour" }
            option { value: "12h", selected: !settings.read().time_24h, "12-hour" }
        }
        select {
            title: "First day of week",
            onchange: move |e| {
                let mut p = settings.read().clone();
                p.first_day_monday = e.value() == "mon";
                p.save(&db_week);
                settings.set(p);
            },
            option { value: "mon", selected: settings.read().first_day_monday, "Week starts Monday" }
            option { value: "sun", selected: !settings.read().first_day_monday, "Week starts Sunday" }
        }
        select {
            title: "Default view",
            onchange: move |e| {
                let mut p = settings.read().clone();
                p.default_view = match e.value().as_str() {
                    "week" => ui::ViewMode::Week,
                    "day" => ui::ViewMode::Day,
                    "agenda" => ui::ViewMode::Agenda,
                    _ => ui::ViewMode::Month,
                };
                p.save(&db_view);
                settings.set(p);
            },
            for m in ui::ViewMode::ALL {
                option {
                    key: "{m:?}",
                    value: "{default_view_value(m)}",
                    selected: settings.read().default_view == m,
                    "{m.label()}"
                }
            }
        }
        button {
            "aria-label": if dark { "Switch to light mode" } else { "Switch to dark mode" },
            onclick: move |_| {
                // Flip ONLY the dedicated theme signal (cheap: re-renders the
                // label + CSS injection, never the calendar views) and
                // persist it under its own key. Reading current state here
                // keeps rapid clicks consistent.
                let next = if theme.read().as_str() == "dark" { "light" } else { "dark" };
                theme.set(next.to_string());
                let _ = db_theme.set_setting("theme", &serde_json::to_string(next).unwrap());
            },
            {if dark { crate::i18n::tr("theme-toggle-light") } else { crate::i18n::tr("theme-toggle-dark") }}
        }
    }
}

fn default_view_value(m: ui::ViewMode) -> &'static str {
    match m {
        ui::ViewMode::Month => "month",
        ui::ViewMode::Week => "week",
        ui::ViewMode::Day => "day",
        ui::ViewMode::Agenda => "agenda",
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
    let mut layout = use_context::<Signal<ui::UiLayout>>();

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

    let l = layout.read();
    let style_width = if l.sidebar_open {
        format!("width:{}px", l.sidebar_width)
    } else {
        "width:0;padding:0;border-right:none".to_string()
    };

    rsx! {
        aside { class: "sidebar", style: "{style_width}",
            div { class: "resize-handle",
                title: "Drag to resize",
                onmousedown: move |_| layout.write().sidebar_resizing = true,
            }
            h2 { "Calendars" }
            ul {
                for cal in calendars.iter().cloned() {
                    CalendarRow { key: "{cal.id}", calendar: cal }
                }
            }
            sync_ui::SyncPanel {}
            h2 { "Widget" }
            button {
                onclick: move |_| crate::mini_widget::launch_mini_window(),
                "Open mini calendar"
            }
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
            format!(
                "{} – {}",
                ws.format("%b %d"),
                (ws + chrono::Duration::days(6)).format("%b %d, %Y")
            )
        }
        ui::ViewMode::Day => c.format("%A, %B %d %Y").to_string(),
        ui::ViewMode::Agenda => "Agenda — next 30 days".to_string(),
    };

    let step_back = move |_| {
        let v = view.read();
        cursor.set(match *v {
            ui::ViewMode::Day => *cursor.read() - chrono::Duration::days(1),
            ui::ViewMode::Week => *cursor.read() - chrono::Duration::days(7),
            _ => cursor
                .read()
                .checked_sub_months(Months::new(1))
                .unwrap_or(*cursor.read()),
        });
    };
    let step_fwd = move |_| {
        let v = view.read();
        cursor.set(match *v {
            ui::ViewMode::Day => *cursor.read() + chrono::Duration::days(1),
            ui::ViewMode::Week => *cursor.read() + chrono::Duration::days(7),
            _ => cursor
                .read()
                .checked_add_months(Months::new(1))
                .unwrap_or(*cursor.read()),
        });
    };

    rsx! {
        div { class: "month-nav",
            button { "aria-label": "Previous period", onclick: step_back, "‹" }
            button {
                "aria-label": "Jump to today",
                onclick: move |_| cursor.set(Local::now().date_naive()),
                {crate::i18n::tr("nav-today")}
            }
            button { "aria-label": "Next period", onclick: step_fwd, "›" }
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
