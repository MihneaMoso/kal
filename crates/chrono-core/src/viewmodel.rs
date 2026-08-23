//! View-model helpers: pure functions turning items into view-ready grids.
//! The Dioxus app renders these directly; widgets will reuse them via FFI.

use chrono::{Datelike, NaiveDate, Weekday};

use crate::models::{CalendarItem, Occurrence};

/// Number of weeks shown in a month grid row set (always render 6 rows so the
/// grid never changes height while navigating).
pub const MONTH_GRID_WEEKS: usize = 6;

/// A rectangular grid of dates covering `year-month` (and spilling into
/// neighbouring months), starting on `first_day_of_week`, always
/// [`MONTH_GRID_WEEKS`] rows tall.
pub fn month_grid(year: i32, month: u32, first_day_of_week: Weekday) -> Vec<Vec<NaiveDate>> {
    let first_of_month =
        NaiveDate::from_ymd_opt(year, month, 1).expect("valid year/month");
    // Days back from the 1st to reach the configured week start.
    let offset = (first_of_month.weekday().num_days_from_monday() as i64
        - first_day_of_week.num_days_from_monday() as i64)
        .rem_euclid(7);
    let grid_start = first_of_month - chrono::Duration::days(offset);

    let mut grid = Vec::with_capacity(MONTH_GRID_WEEKS);
    for w in 0..MONTH_GRID_WEEKS {
        let row_start = grid_start + chrono::Duration::weeks(w as i64);
        grid.push(
            (0..7)
                .map(|d| row_start + chrono::Duration::days(d))
                .collect(),
        );
    }
    grid
}

/// Does this item's span (start ..= effective end) cover `date`?
/// Non-recurring only; recurring expansion arrives in phase 3.
pub fn item_covers_date(item: &CalendarItem, date: NaiveDate) -> bool {
    if item.deleted {
        return false;
    }
    let start_date = item.start.date_naive();
    let end_date = item
        .end
        .as_ref()
        .map(|e| e.date_naive())
        .unwrap_or(start_date);
    date >= start_date && date <= end_date
}

/// Items overlapping a given day, sorted by start time.
pub fn items_on_date<'a>(items: &'a [CalendarItem], date: NaiveDate) -> Vec<Occurrence> {
    let mut occ: Vec<Occurrence> = items
        .iter()
        .filter(|i| item_covers_date(i, date))
        .map(|i| Occurrence {
            item_id: i.id,
            start: i.start,
            end: i.end,
        })
        .collect();
    occ.sort_by_key(|o| o.start);
    occ
}

/// Flat chronological list of occurrences over `[from, to]` (inclusive days).
pub fn agenda_range(items: &[CalendarItem], from: NaiveDate, to: NaiveDate) -> Vec<(NaiveDate, Occurrence)> {
    let mut out = Vec::new();
    let mut d = from;
    while d <= to {
        for occ in items_on_date(items, d) {
            out.push((d, occ));
        }
        d += chrono::Duration::days(1);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Timelike;
    use crate::models::{
        datetime_from_parts, Calendar, Color, DateTimeTz, ItemKind,
    };

    fn cal() -> Calendar {
        Calendar::local("Test", Color("#3366cc".into()))
    }

    fn item_at(y: i32, m: u32, d: u32, hour: u32) -> CalendarItem {
        let start: DateTimeTz = datetime_from_parts(y, m, d, hour, 0, 0).unwrap();
        CalendarItem::new(ItemKind::Event, "e", cal().id, start)
    }

    #[test]
    fn month_grid_shape_and_alignment() {
        let grid = month_grid(2026, 8, Weekday::Mon);
        assert_eq!(grid.len(), MONTH_GRID_WEEKS);
        for row in &grid {
            assert_eq!(row.len(), 7);
            assert_eq!(row[0].weekday(), Weekday::Mon);
        }
        // August 2026 starts on a Saturday.
        assert_eq!(grid[0][0], NaiveDate::from_ymd_opt(2026, 7, 27).unwrap());
        assert!(grid.iter().flatten().any(|&d| d == NaiveDate::from_ymd_opt(2026, 8, 1).unwrap()));
        // Sunday start shifts the first cell.
        let grid_sun = month_grid(2026, 8, Weekday::Sun);
        assert_eq!(grid_sun[0][0].weekday(), Weekday::Sun);
        assert_eq!(grid_sun[0][0], NaiveDate::from_ymd_opt(2026, 7, 26).unwrap());
    }

    #[test]
    fn items_on_date_filters_and_sorts() {
        let mut a = item_at(2026, 8, 24, 10);
        a.end = Some(a.start + chrono::Duration::hours(2));
        let b = item_at(2026, 8, 24, 8);
        let c = item_at(2026, 8, 25, 8);
        let all = vec![a.clone(), b.clone(), c];

        let day = NaiveDate::from_ymd_opt(2026, 8, 24).unwrap();
        let occ = items_on_date(&all, day);
        assert_eq!(occ.len(), 2);
        assert_eq!(occ[0].start.hour(), 8); // sorted by start

        let mut deleted = a.clone();
        deleted.deleted = true;
        assert!(!item_covers_date(&deleted, day));
    }

    #[test]
    fn multi_day_event_covers_intermediate_days() {
        let mut it = item_at(2026, 8, 24, 9);
        it.end = Some(datetime_from_parts(2026, 8, 27, 11, 0, 0).unwrap());
        for d in [24u32, 25, 26, 27] {
            assert!(item_covers_date(&it, NaiveDate::from_ymd_opt(2026, 8, d).unwrap()));
        }
        assert!(!item_covers_date(&it, NaiveDate::from_ymd_opt(2026, 8, 28).unwrap()));
    }

    #[test]
    fn agenda_range_is_chronological() {
        let items = vec![
            item_at(2026, 8, 25, 9),
            item_at(2026, 8, 24, 18),
            item_at(2026, 8, 24, 9),
        ];
        let from = NaiveDate::from_ymd_opt(2026, 8, 24).unwrap();
        let out = agenda_range(&items, from, from + chrono::Duration::days(1));
        assert_eq!(out.len(), 3);
        assert_eq!(out[0].0, from);
        assert_eq!(out[0].1.start.hour(), 9);
        assert_eq!(out[2].0, from + chrono::Duration::days(1));
    }
}
