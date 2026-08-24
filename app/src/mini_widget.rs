//! Always-on-top "mini calendar" secondary window (spec §5.6 desktop widgets).
//!
//! The popup runs as its own VirtualDom without the main app's contexts, so
//! it opens its own read-only DB handle and renders a compact month view.

use chrono::{Datelike, Local, Weekday};
use dioxus::prelude::*;
use dioxus_desktop::{Config, LogicalSize, WindowBuilder};
use kal_core::viewmodel;

use crate::DbHandle;

/// Spawn the popup from an event handler (must run on the app's runtime).
pub fn launch_mini_window() {
    spawn(async move {
        let cfg = Config::new().with_window(
            WindowBuilder::new()
                .with_title("Kal — mini")
                .with_always_on_top(true)
                .with_resizable(false)
                .with_inner_size(LogicalSize::new(280.0_f64, 320.0_f64)),
        );
        let dom = VirtualDom::new(MiniCalendar);
        let _ = dioxus::desktop::window().new_window(dom, cfg).await;
    });
}

#[allow(non_snake_case)]
fn MiniCalendar() -> Element {
    let db: DbHandle = crate::open_db();
    // Re-render every 5 minutes so "today" highlight stays honest.
    let mut tick = use_signal(|| 0u32);
    use_future(move || async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(300)).await;
            tick += 1;
        }
    });

    let now = Local::now();
    let c = now.date_naive();
    let items = db.list_items(false).unwrap_or_default();
    let first_day = if true { Weekday::Mon } else { Weekday::Sun };
    let grid = viewmodel::month_grid(c.year(), c.month(), first_day);
    let occs =
        viewmodel::occurrences_by_date(&items, grid[0][0], grid[viewmodel::MONTH_GRID_WEEKS - 1][6]);
    let today = now.date_naive();

    rsx! {
        div { style: "font-family:system-ui;padding:8px;background:#fff;color:#111;",
            div { style: "font-size:13px;font-weight:600;margin-bottom:4px;",
                "{c.format(\"%B %Y\")}"
            }
            for row in grid.iter() {
                div { style: "display:grid;grid-template-columns:repeat(7,1fr);gap:2px;margin-bottom:2px;",
                    for date in row.iter() {
                        {
                            let day_items = occs.get(date).cloned().unwrap_or_default();
                            let titles: Vec<String> = day_items
                                .iter()
                                .filter_map(|o| items.iter().find(|i| i.id == o.item_id).map(|i| i.title.clone()))
                                .collect();
                            let bg = if *date == today { "#eef4ff" } else { "transparent" };
                            let count = titles.len();
                            rsx! {
                                div {
                                    key: "{date}",
                                    style: "background:{bg};border-radius:4px;text-align:right;font-size:11px;padding:2px;min-height:26px;",
                                    title: "{titles.join(\", \")}",
                                    "{date.day()}"
                                    if count > 0 {
                                        div { style: "color:#3366cc;font-size:9px;", "•{count}" }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
