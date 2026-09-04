#[cfg(target_os = "android")]
mod android_picker;
mod i18n;
#[cfg(all(not(target_os = "android"), not(target_arch = "wasm32")))]
mod mini_widget;
mod profile;
#[cfg(not(target_arch = "wasm32"))]
mod sync_live;
#[cfg(not(target_arch = "wasm32"))]
mod sync_log;
#[cfg(not(target_arch = "wasm32"))]
mod sync_ui;
mod ui;
mod updater;
#[cfg(target_os = "android")]
mod widget_ffi;

use chrono::{Datelike, Local, Months, NaiveDate};
use dioxus::prelude::*;
#[cfg(all(not(target_os = "android"), not(target_arch = "wasm32")))]
use dioxus_desktop::{Config, WindowBuilder};
use kal_core::models::{Calendar, CalendarItem, Color};
#[cfg(all(not(target_os = "android"), not(target_arch = "wasm32")))]
use kal_notify::DesktopNotifier;
#[cfg(not(target_arch = "wasm32"))]
use kal_notify::{ReminderScheduler as _, ThreadScheduler};
use kal_storage::Database;
use std::sync::Arc;

#[cfg(all(not(target_os = "android"), not(target_arch = "wasm32")))]
pub type SchedulerHandle = Arc<ThreadScheduler<DesktopNotifier>>;
#[cfg(target_os = "android")]
pub type SchedulerHandle = Arc<ThreadScheduler<kal_notify::NullNotifier>>;
#[cfg(target_arch = "wasm32")]
pub type SchedulerHandle = Arc<WebNoopScheduler>;

/// Web-only no-op reminder scheduler: browser notifications are out of scope
/// for this build, but the app has a shared `SchedulerHandle` so it stays
/// structurally uniform across platforms.
#[cfg(target_arch = "wasm32")]
#[derive(Default)]
pub struct WebNoopScheduler;

#[cfg(target_arch = "wasm32")]
impl WebNoopScheduler {
    pub fn new() -> Self {
        Self
    }
    #[allow(clippy::needless_lifetimes)]
    pub fn reschedule<'a>(&self, _firings: &'a [kal_core::reminders::ReminderFiring]) {}
}

/// App-wide shared handle to the SQLite database.
pub type DbHandle = Arc<Database>;

/// Resolve the writable, app-private data directory for the current platform.
///
/// Desktop: `~/.local/share` (or the platform equivalent).
/// Android: the app's private `getFilesDir()` on the data partition — the
/// generic `dirs_next::data_dir()` has no `$HOME`/`XDG` env on Android and
/// falls back to a relative path on the read-only root FS, which surfaces as
/// "Read-only file system (os error 30)".
#[cfg(not(target_arch = "wasm32"))]
fn app_data_dir() -> Option<std::path::PathBuf> {
    #[cfg(not(target_os = "android"))]
    {
        dirs_next::data_dir()
    }
    #[cfg(target_os = "android")]
    {
        android_files_dir()
    }
}

#[cfg(target_os = "android")]
fn android_files_dir() -> Option<std::path::PathBuf> {
    use std::sync::Mutex;
    use std::sync::OnceLock;

    // Cached so the (attached-thread) JNI lookup runs only once.
    static CACHE: OnceLock<Mutex<Option<std::path::PathBuf>>> = OnceLock::new();
    let mut cache = CACHE
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    if cache.is_none() {
        *cache = query_files_dir();
    }
    cache.clone()
}

#[cfg(target_os = "android")]
fn query_files_dir() -> Option<std::path::PathBuf> {
    let ctx = ndk_context::android_context();
    // Rely on the DA function never being called before the context is set.
    let vm = unsafe { jni::JavaVM::from_raw(ctx.vm().cast()) }.ok()?;
    let mut env = vm.attach_current_thread().ok()?;
    let activity = unsafe { jni::objects::JObject::from_raw(ctx.context().cast()) };
    let file = env
        .call_method(activity, "getFilesDir", "()Ljava/io/File;", &[])
        .ok()?
        .l()
        .ok()?;
    let abs: jni::objects::JObject = env
        .call_method(&file, "getAbsolutePath", "()Ljava/lang/String;", &[])
        .ok()?
        .l()
        .ok()?;
    let jstring = jni::objects::JString::from(abs);
    let s = env.get_string(&jstring).ok()?;
    Some(std::path::PathBuf::from(s.to_string_lossy().to_string()))
}

#[cfg(not(target_arch = "wasm32"))]
pub fn default_db_path() -> Option<std::path::PathBuf> {
    app_data_dir().map(|d| d.join("kal").join("calendar.db"))
}

#[cfg(not(target_arch = "wasm32"))]
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

// Web: the database is an in-memory store backed by IndexedDB. `App` hands out
// an `Arc` of this single shared instance; the async snapshot restore then runs
// in-place (via `load_into`) once the renderer is up, so every handle observes
// the loaded data. The wasm store is `!Send`/`!Sync` (it wraps `Rc`), so it
// lives in a thread-local rather than a `Sync` static — fine on single-threaded
// wasm. `dioxus::launch` drives persistence because the wasm `Database`
// schedules IndexedDB flushes on its own.
#[cfg(target_arch = "wasm32")]
thread_local! {
    static WEB_DB: std::cell::RefCell<Option<DbHandle>> = const { std::cell::RefCell::new(None) };
}

#[cfg(target_arch = "wasm32")]
fn open_db() -> DbHandle {
    WEB_DB.with(|slot| {
        slot.borrow_mut()
            .get_or_insert_with(|| {
                Arc::new(Database::open_in_memory().expect("failed to init web store"))
            })
            .clone()
    })
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

#[cfg(all(not(target_os = "android"), not(target_arch = "wasm32")))]
fn main() {
    // Capture the iroh/gossip bootstrap trace (see sync_log.rs) before any
    // sync transport is built.
    sync_log::init_trace_log();
    // Swap in a staged update (if any) before any window/DB work, so the new
    // binary relaunches cleanly. No-op when nothing is staged.
    updater::apply_staged_update();
    // Frameless: hide the GTK titlebar/menu entirely (user preference).
    let mut cfg = Config::new().with_window(
        WindowBuilder::new()
            .with_title("Kal")
            .with_decorations(false)
            .with_maximized(true),
    );
    // Brand the window/taskbar with the project logo. The PNG is embedded at
    // compile time so the icon is identical whether running via `cargo run`,
    // `dx serve`, or a packaged/installable build (it never depends on the
    // current working directory). `image` is already a runtime dependency of
    // dioxus-desktop, so decoding PNG -> RGBA needs no extra crate.
    if let Ok(icon) = dioxus_desktop::icon_from_memory::<dioxus_desktop::tao::window::Icon>(
        include_bytes!("../../assets/icons/desktop/kal-256.png"),
    ) {
        cfg = cfg.with_icon(icon);
    }
    // Wayland/GNOME resolves the taskbar & dash icon from the app's .desktop
    // file (looked up by the GTK app id) rather than the window icon above,
    // which is what X11 and other WMs use. Install the branded .desktop +
    // icon under the user's XDG data dir so the logo shows whether run via
    // `dx serve` or a packaged build. Best-effort; never blocks launch.
    #[cfg(target_os = "linux")]
    register_desktop_app();
    dioxus::LaunchBuilder::new().with_cfg(cfg).launch(App);
}

#[cfg(target_os = "linux")]
fn register_desktop_app() {
    use std::path::PathBuf;
    let data_home = std::env::var("XDG_DATA_HOME")
        .ok()
        .filter(|p| !p.trim().is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var("HOME")
                .ok()
                .map(|home| PathBuf::from(home).join(".local/share"))
        });
    let Some(data_home) = data_home else { return };
    let icon = include_bytes!("../../assets/icons/desktop/kal-256.png");
    let desktop = include_str!("../../assets/applications/kal.desktop");
    write_if_changed(
        data_home.join("applications/kal.desktop"),
        desktop.as_bytes(),
    );
    write_if_changed(data_home.join("icons/hicolor/256x256/apps/kal.png"), icon);
    write_if_changed(data_home.join("pixmaps/kal.png"), icon);
}

#[cfg(target_os = "linux")]
fn write_if_changed(path: std::path::PathBuf, bytes: &[u8]) {
    use std::fs;
    if fs::read(&path)
        .map(|existing| existing == bytes)
        .unwrap_or(false)
    {
        return;
    }
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let _ = fs::write(path, bytes);
}

#[cfg(target_os = "android")]
fn main() {
    // Same syncing trace as desktop; lands in <files>/kal/sync-trace.log
    // (readable via `adb shell run-as com.kal.calendar cat files/kal/...`).
    sync_log::init_trace_log();
    dioxus::launch(App);
}

#[cfg(target_arch = "wasm32")]
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

    let prefs = ui::Settings::load(&db);
    *profile::PROFILE_VIEW.write() = profile::load_profile(&db);

    // Layout + theme in ONE signal: the resize path proves this signal
    // drives visual updates reliably; theme rides along on that proven path.
    let mobile = cfg!(target_os = "android") || cfg!(target_os = "ios");
    use_context_provider(|| {
        Signal::new(ui::UiLayout {
            sidebar_open: !mobile, // collapsed by default on mobile
            sidebar_width: 230,
            theme: prefs.theme.clone(),
            mobile,
            ..Default::default()
        })
    });

    // Startup view is always Month (user request); the persisted default_view
    // preference no longer overrides it.
    use_context_provider(|| Signal::new(ui::ViewMode::Month));

    let today: NaiveDate = Local::now().date_naive();
    use_context_provider(|| Signal::new(today));
    let settings = Signal::new(prefs);
    use_context_provider(|| settings);

    // Background auto-update check on startup, if enabled. Runs once on mount;
    // the result lands in the settings screen's update section.
    let auto_check = settings.read().auto_check_updates;
    use_effect(move || {
        if auto_check {
            updater::run_check();
        }
    });

    // Eagerly start the live P2P transport on app launch so the DHT record is
    // published early — without this, a peer that taps "Sync now" before we've
    // been online long enough sees "no peers yet" because our DHT entry hasn't
    // been discovered yet.  The transport is cheap to keep alive (a tokio
    // runtime + one gossip subscription) and is keyed by chain fingerprint so it
    // is recreated automatically if the user switches chains.
    //
    // live_transport builds any new transport on a dedicated thread (so the
    // runtime's block_on inside IrohTransport::connect doesn't panic), which
    // makes it safe to call directly from this effect.
    #[cfg(not(target_arch = "wasm32"))]
    {
        use_effect(move || {
            if let Some(identity) = sync_ui::load_identity() {
                sync_live::live_transport(&identity);
            }
            // Background driver: periodically re-runs sync rounds so paired
            // devices converge automatically once they are online together,
            // instead of relying on a single manual "Sync now" press.
            sync_ui::start_background_sync();
        });
    }

    // Single shared item list; views read it via context and restart it after
    // mutations.
    let mut items_res = use_resource(move || {
        let db = db_items.clone();
        async move { db.list_items(false).unwrap_or_default() }
    });
    use_context_provider(|| items_res);

    // Reminder scheduler shared app-wide; reconciled in the effect below.
    #[cfg(all(not(target_os = "android"), not(target_arch = "wasm32")))]
    let scheduler: SchedulerHandle = Arc::new(ThreadScheduler::new(DesktopNotifier));
    #[cfg(target_os = "android")]
    let scheduler: SchedulerHandle = Arc::new(ThreadScheduler::new(kal_notify::NullNotifier));
    #[cfg(target_arch = "wasm32")]
    let scheduler: SchedulerHandle = Arc::new(WebNoopScheduler::new());
    use_context_provider(|| scheduler);

    // Reload once default calendars are ensured.
    use_effect(move || {
        if cal_res.value().read().is_some() {
            calendars_res.restart();
            items_res.restart();
        }
    });

    // After a sync merge (sync_ui bumps this), reload everything.
    #[cfg(not(target_arch = "wasm32"))]
    {
        let mut cal_res_sync = calendars_res;
        let db_dirty = db.clone();
        use_effect(move || {
            if *sync_ui::RESOURCES_DIRTY.read() > 0 {
                cal_res_sync.restart();
                let _ = &db_dirty;
                items_res.restart();
            }
        });

        // Bridge the background driver's thread-safe counter into the Dioxus
        // refresh signal. The driver runs on a plain OS thread (no runtime), so
        // it can't bump the signal itself; this poller runs on the runtime and
        // forwards any change.
        let mut last_driver_dirty =
            use_signal(|| sync_ui::SYNC_DRIVER_DIRTY.load(std::sync::atomic::Ordering::Relaxed));
        use_future(move || async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_millis(250)).await;
                let now = sync_ui::SYNC_DRIVER_DIRTY.load(std::sync::atomic::Ordering::Relaxed);
                if now != *last_driver_dirty.read() {
                    *last_driver_dirty.write() = now;
                    *sync_ui::RESOURCES_DIRTY.write() += 1;
                }
            }
        });
    }

    // Web: restore the persisted IndexedDB snapshot into the shared store once,
    // then reload the views so the restored data is shown. Runs as an effect so
    // it has the Dioxus runtime (for the async IndexedDB calls) available.
    #[cfg(target_arch = "wasm32")]
    {
        let db_load = db.clone();
        let mut cal_res_w = calendars_res;
        let mut items_res_w = items_res;
        let mut loaded = use_signal(|| false);
        use_effect(move || {
            if *loaded.read() {
                return;
            }
            let db2 = db_load.clone();
            spawn(async move {
                let _ = db2.load_into().await;
                cal_res_w.restart();
                items_res_w.restart();
            });
            *loaded.write() = true;
        });
    }

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

    // Base stylesheet: injected once per render as a plain <style> tag.
    // (main.css is only delivered through this inline tag — there is no
    // external asset pipeline in the desktop build.)
    let css = include_str!("../assets/main.css");

    // Sidebar resize dragging (root-level so fast cursor movement can't
    // escape the handle).
    let mut layout = use_context::<Signal<ui::UiLayout>>();
    let is_mobile = layout.read().mobile;
    let app_class = if layout.read().sidebar_resizing {
        "app resizing"
    } else if is_mobile {
        "app mobile"
    } else {
        "app"
    };
    let on_root_mousemove = move |e: Event<MouseData>| {
        if layout.read().sidebar_resizing {
            let x = e.client_coordinates().x as u32;
            layout.write().sidebar_width = x.clamp(170, 480);
        }
    };
    let end_resize = move |_: Event<MouseData>| layout.write().sidebar_resizing = false;

    // Theme rides on the layout signal (see provider above): reading it here
    // subscribes App, so toggling re-renders and the data-theme attribute —
    // the only theming hook the CSS needs — is recomputed.
    let cur_theme = layout.read().theme.clone();
    let drawer_open = is_mobile && layout.read().sidebar_open;
    let close_drawer = move |_| layout.write().sidebar_open = false;

    rsx! {
        div {
            class: "{app_class}",
            "data-theme": "{cur_theme}",
            onmousemove: on_root_mousemove,
            onmouseup: end_resize,
            onmouseleave: end_resize,
            style { dangerous_inner_html: "{css}" }
            if drawer_open {
                // Semi-transparent scrim behind the mobile overlay drawer;
                // tapping it closes the drawer. Doesn't push content aside.
                div {
                    class: "drawer-scrim",
                    onclick: close_drawer,
                }
            }
            TopBar {}
            div { class: "body",
                Sidebar {}
                Content {}
            }

            match ui::EDITOR_OPEN.read().clone() {
                Some(state) => rsx! { ui::EditorModal { state } },
                None => rsx! {},
            }

            if *profile::SETTINGS_SCREEN_OPEN.read() {
                profile::SettingsScreen {}
            }
        }
    }
}

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
    let mut layout = use_context::<Signal<ui::UiLayout>>();
    let dark = layout.read().theme.as_str() == "dark";
    let db_time = db.clone();
    let db_week = db.clone();
    // Theme persists under its own key; the signal of record is UiLayout.
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
        button {
            "aria-label": if dark { "Switch to light mode" } else { "Switch to dark mode" },
            onclick: move |_| {
                // Flip theme on the UiLayout signal (drives the data-theme
                // attribute on the root div) and persist under its own key.
                let next = if layout.read().theme.as_str() == "dark" { "light" } else { "dark" };
                layout.write().theme = next.to_string();
                let _ = db_theme.set_setting("theme", next);
            },
            {if dark { crate::i18n::tr("theme-toggle-dark") } else { crate::i18n::tr("theme-toggle-light") }}
        }
        button {
            class: "icon-btn",
            "aria-label": "Settings",
            title: "Settings",
            onclick: move |_| profile::open_settings_screen(&db),
            "\u{2699}"
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

#[cfg(not(target_arch = "wasm32"))]
fn sync_panel() -> Element {
    rsx! { sync_ui::SyncPanel {} }
}

#[cfg(target_arch = "wasm32")]
fn sync_panel() -> Element {
    rsx! {}
}

#[cfg(all(not(target_os = "android"), not(target_arch = "wasm32")))]
fn desktop_sidebar_sections(
    db: &DbHandle,
    mut cal_res: Resource<Vec<Calendar>>,
    mut items_res: Resource<Vec<CalendarItem>>,
) -> Element {
    let db_import = db.clone();
    let db_export = db.clone();
    rsx! {
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
    }
}

#[cfg(target_os = "android")]
fn desktop_sidebar_sections(
    _db: &DbHandle,
    _cal_res: Resource<Vec<Calendar>>,
    _items_res: Resource<Vec<CalendarItem>>,
) -> Element {
    rsx! {}
}

// Web has no desktop mini-window policy nor a native filesystem for the .ics
// import/export file dialogs, so the corresponding sidebar sections are absent.
#[cfg(target_arch = "wasm32")]
fn desktop_sidebar_sections(
    _db: &DbHandle,
    _cal_res: Resource<Vec<Calendar>>,
    _items_res: Resource<Vec<CalendarItem>>,
) -> Element {
    rsx! {}
}

#[component]
fn Sidebar() -> Element {
    let mut layout = use_context::<Signal<ui::UiLayout>>();

    let db = use_context::<DbHandle>();
    let items_res = use_context::<Resource<Vec<CalendarItem>>>();

    let cal_res = use_context::<Resource<Vec<Calendar>>>();
    let calendars = cal_res.value().read().clone().unwrap_or_default();

    let db_event = db.clone();
    let db_task = db.clone();
    let db_bday = db.clone();

    let l = layout.read();
    let is_mobile = l.mobile;
    // On desktop the sidebar is a flex child that pushes content; on mobile it
    // is an overlay drawer that slides in above the calendar via translateX.
    let style_width = if is_mobile {
        if l.sidebar_open {
            "width:280px;transform:translateX(0);visibility:visible;".to_string()
        } else {
            "width:280px;transform:translateX(-100%);visibility:hidden;".to_string()
        }
    } else if l.sidebar_open {
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
            {sync_panel()}
            {desktop_sidebar_sections(&db, cal_res, items_res)}
            h2 { "New item" }
            div { style: "display:flex; flex-direction:column; gap:6px;",
                button {
                    class: "primary",
                    onclick: move |_| { *ui::EDITOR_OPEN.write() = Some(ui::EditorState::new_kind(&db_event, kal_core::models::ItemKind::Event)); },
                    "+ Event"
                }
                button {
                    onclick: move |_| { *ui::EDITOR_OPEN.write() = Some(ui::EditorState::new_kind(&db_task, kal_core::models::ItemKind::Task)); },
                    "+ Task"
                }
                button {
                    onclick: move |_| { *ui::EDITOR_OPEN.write() = Some(ui::EditorState::new_kind(&db_bday, kal_core::models::ItemKind::Birthday)); },
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

#[cfg(all(not(target_os = "android"), not(target_arch = "wasm32")))]
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
