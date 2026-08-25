//! View-model helpers: pure functions turning items into view-ready grids.
//! The Dioxus app renders these directly; widgets will reuse them via FFI.

use chrono::{Datelike, NaiveDate, Weekday};
use rrule::Tz as RRuleTz;
use std::str::FromStr;

use crate::models::{CalendarItem, DateTimeTz, Occurrence};
use rrule::RRule;
use std::collections::BTreeMap;

/// Number of weeks shown in a month grid row set (always render 6 rows so the
/// grid never changes height while navigating).
pub const MONTH_GRID_WEEKS: usize = 6;

/// Upper bound on expanded recurrences per item per query window.
const MAX_OCCURRENCES: u16 = 1000;

/// A rectangular grid of dates covering `year-month` (and spilling into
/// neighbouring months), starting on `first_day_of_week`, always
/// [`MONTH_GRID_WEEKS`] rows tall.
pub fn month_grid(year: i32, month: u32, first_day_of_week: Weekday) -> Vec<Vec<NaiveDate>> {
    let first_of_month = NaiveDate::from_ymd_opt(year, month, 1).expect("valid year/month");
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
pub fn items_on_date(items: &[CalendarItem], date: NaiveDate) -> Vec<Occurrence> {
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
pub fn agenda_range(
    items: &[CalendarItem],
    from: NaiveDate,
    to: NaiveDate,
) -> Vec<(NaiveDate, Occurrence)> {
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

/// Expand an item's occurrences (base + RRULE series minus EXDATEs) that fall
/// within `[from, to]` (inclusive by day).
///
/// Multi-day non-recurring events produce one occurrence per covered day so
/// they appear on every grid cell they span. Recurring occurrences keep the
/// original start time and duration.
pub fn expand_occurrences(item: &CalendarItem, from: NaiveDate, to: NaiveDate) -> Vec<Occurrence> {
    if item.deleted {
        return Vec::new();
    }
    // rrule 0.14 works in its own Tz enum; convert through UTC.
    let to_rrule = |dt: &DateTimeTz| dt.with_timezone(&chrono::Utc).with_timezone(&RRuleTz::UTC);
    let window_end_dt =
        to_rrule(&(item.start + (to - item.start.date_naive()) + chrono::Duration::days(1)));

    let mut out = Vec::new();
    match &item.rrule {
        None => {
            // Non-recurring: one occurrence per covered day in range.
            let start_date = item.start.date_naive();
            let end_date = item
                .end
                .as_ref()
                .map(|e| e.date_naive())
                .unwrap_or(start_date);
            let first = from.max(start_date);
            let last = to.min(end_date);
            let mut d = first;
            while d <= last {
                let offset_days = (d - start_date).num_days();
                let occ_start = item.start + chrono::Duration::days(offset_days);
                let occ_end = item.end.map(|e| e + chrono::Duration::days(offset_days));
                out.push(Occurrence {
                    item_id: item.id,
                    start: occ_start,
                    end: occ_end,
                });
                d += chrono::Duration::days(1);
            }
        }
        Some(rule_str) => {
            let Ok(unvalidated) = RRule::from_str(rule_str) else {
                return Vec::new();
            };
            let Ok(rule) = unvalidated.validate(to_rrule(&item.start)) else {
                return Vec::new();
            };
            let mut set = rrule::RRuleSet::new(to_rrule(&item.start)).rrule(rule);
            for ex in &item.exdates {
                if *ex >= item.start {
                    set = set.exdate(to_rrule(ex));
                }
            }
            let result = set.before(window_end_dt).all(MAX_OCCURRENCES);
            let duration = item.end.map(|e| e - item.start);
            for dt in result.dates {
                let dt = dt.with_timezone(item.start.offset());
                let d = dt.date_naive();
                if d < from || d > to {
                    continue;
                }
                out.push(Occurrence {
                    item_id: item.id,
                    start: dt,
                    end: duration.map(|dur| dt + dur),
                });
            }
        }
    }
    out.sort_by_key(|o| o.start);
    out
}

/// Group expanded occurrences of `items` by the day each starts on.
/// Days with no occurrences are absent; each day's list is time-sorted.
pub fn occurrences_by_date(
    items: &[CalendarItem],
    from: NaiveDate,
    to: NaiveDate,
) -> BTreeMap<NaiveDate, Vec<Occurrence>> {
    let mut map: BTreeMap<NaiveDate, Vec<Occurrence>> = BTreeMap::new();
    for item in items {
        for occ in expand_occurrences(item, from, to) {
            map.entry(occ.start.date_naive()).or_default().push(occ);
        }
    }
    for occs in map.values_mut() {
        occs.sort_by_key(|o| o.start);
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{datetime_from_parts, Calendar, Color, DateTimeTz, ItemKind};
    use chrono::Timelike;

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
        assert!(grid
            .iter()
            .flatten()
            .any(|&d| d == NaiveDate::from_ymd_opt(2026, 8, 1).unwrap()));
        // Sunday start shifts the first cell.
        let grid_sun = month_grid(2026, 8, Weekday::Sun);
        assert_eq!(grid_sun[0][0].weekday(), Weekday::Sun);
        assert_eq!(
            grid_sun[0][0],
            NaiveDate::from_ymd_opt(2026, 7, 26).unwrap()
        );
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
            assert!(item_covers_date(
                &it,
                NaiveDate::from_ymd_opt(2026, 8, d).unwrap()
            ));
        }
        assert!(!item_covers_date(
            &it,
            NaiveDate::from_ymd_opt(2026, 8, 28).unwrap()
        ));
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

#[cfg(test)]
mod recurrence_tests {
    use super::*;
    use crate::models::{datetime_from_parts, Calendar, Color, DateTimeTz};

    fn cal() -> Calendar {
        Calendar::local("Test", Color("#3366cc".into()))
    }

    fn item(y: i32, m: u32, d: u32) -> CalendarItem {
        let start: DateTimeTz = datetime_from_parts(y, m, d, 9, 0, 0).unwrap();
        let mut it = CalendarItem::new(crate::models::ItemKind::Event, "e", cal().id, start);
        it.end = Some(start + chrono::Duration::hours(1));
        it
    }

    fn date(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    #[test]
    fn daily_rule_expands() {
        let mut it = item(2026, 8, 24);
        it.rrule = Some("FREQ=DAILY;COUNT=3".into());
        let occs = expand_occurrences(&it, date(2026, 8, 1), date(2026, 9, 30));
        assert_eq!(occs.len(), 3);
        assert_eq!(occs[0].start.date_naive(), date(2026, 8, 24));
        assert_eq!(occs[2].start.date_naive(), date(2026, 8, 26));
        // Duration preserved per occurrence.
        assert_eq!(
            Some(occs[0].start + chrono::Duration::hours(1)),
            occs[0].end
        );
    }

    #[test]
    fn weekly_byday_expansion() {
        let mut it = item(2026, 8, 24); // Monday
        it.rrule = Some("FREQ=WEEKLY;BYDAY=MO,WE".into());
        let occs = expand_occurrences(&it, date(2026, 8, 24), date(2026, 8, 31));
        let days: Vec<NaiveDate> = occs.iter().map(|o| o.start.date_naive()).collect();
        assert_eq!(
            days,
            vec![date(2026, 8, 24), date(2026, 8, 26), date(2026, 8, 31)]
        );
    }

    #[test]
    fn exdate_removes_instance() {
        let mut it = item(2026, 8, 24);
        it.rrule = Some("FREQ=DAILY;COUNT=5".into());
        assert_eq!(
            expand_occurrences(&it, date(2026, 8, 1), date(2026, 12, 1)).len(),
            5
        );
        it.exdates
            .push(datetime_from_parts(2026, 8, 25, 9, 0, 0).unwrap());
        let occs = expand_occurrences(&it, date(2026, 8, 1), date(2026, 12, 1));
        assert_eq!(occs.len(), 4);
        assert!(!occs
            .iter()
            .any(|o| o.start.date_naive() == date(2026, 8, 25)));
    }

    #[test]
    fn until_bounds_expansion() {
        let mut it = item(2026, 8, 24);
        it.rrule = Some("FREQ=DAILY;UNTIL=20260826T000000Z".into());
        let occs = expand_occurrences(&it, date(2026, 8, 1), date(2027, 1, 1));
        assert!(occs.len() <= 3);
    }

    #[test]
    fn window_filters_recurrences() {
        let mut it = item(2026, 8, 24);
        it.rrule = Some("FREQ=DAILY".into()); // unbounded
        let occs = expand_occurrences(&it, date(2026, 9, 1), date(2026, 9, 7));
        assert_eq!(occs.len(), 7);
        assert_eq!(occs[0].start.date_naive(), date(2026, 9, 1));
    }

    #[test]
    fn invalid_rrule_yields_base_only_or_empty() {
        let mut it = item(2026, 8, 24);
        it.rrule = Some("NOT A RULE".into());
        assert!(expand_occurrences(&it, date(2026, 8, 1), date(2026, 12, 1)).is_empty());
    }

    #[test]
    fn multiday_nonrecurring_spans_days() {
        let mut it = item(2026, 8, 24);
        it.end = Some(datetime_from_parts(2026, 8, 27, 11, 0, 0).unwrap());
        for d in [24u32, 25, 26] {
            assert_eq!(
                expand_occurrences(&it, date(2026, 8, d), date(2026, 8, d)).len(),
                1
            );
        }
        assert!(expand_occurrences(&it, date(2026, 8, 28), date(2026, 8, 28)).is_empty());
    }

    #[test]
    fn grouped_map_is_sorted_and_complete() {
        let a = item(2026, 8, 24); // 09:00
        let mut b = item(2026, 8, 24);
        b.start = datetime_from_parts(2026, 8, 24, 8, 0, 0).unwrap();
        b.end = None;
        let mut c = item(2026, 8, 25);
        c.rrule = Some("FREQ=WEEKLY;COUNT=2".into());

        let map = occurrences_by_date(
            &[a.clone(), b.clone(), c],
            date(2026, 8, 23),
            date(2026, 9, 10),
        );
        assert_eq!(map.get(&date(2026, 8, 24)).unwrap().len(), 2);
        let day = &map[&date(2026, 8, 24)];
        assert_eq!(day[0].start.hour(), 8); // sorted
                                            // Weekly COUNT=2 → two Tuesdays (weekday of dtstart).
        assert_eq!(map.get(&date(2026, 9, 1)).unwrap().len(), 1);
        assert!(!map.contains_key(&date(2026, 9, 8)));
    }

    #[test]
    fn deleted_items_never_expand() {
        let mut it = item(2026, 8, 24);
        it.rrule = Some("FREQ=DAILY;COUNT=5".into());
        it.deleted = true;
        assert!(expand_occurrences(&it, date(2026, 8, 1), date(2026, 12, 1)).is_empty());
    }

    use chrono::Timelike as _;
}

/// One occurrence positioned inside a day time-grid (Google Calendar style).
///
/// `top_frac`/`height_frac` are fractions of the full day (0.0..1.0) so the
/// UI can multiply by its pixel height. Overlapping events share the day
/// width via greedy lane assignment within each overlap cluster.
#[derive(Debug, Clone, PartialEq)]
pub struct PositionedOccurrence {
    pub occ: Occurrence,
    pub top_frac: f64,
    pub height_frac: f64,
    pub lane: usize,
    pub lanes: usize,
}

const DAY_MINUTES: f64 = 24.0 * 60.0;

/// Lay out timed occurrences of a single day.
///
/// Input may be unsorted; all-day items should be excluded by the caller
/// (they render in their own strip). Events are clamped to the day and get a
/// small minimum height so short events remain clickable/visible.
pub fn layout_day(
    mut occs: Vec<Occurrence>,
    _day_start: chrono::DateTime<chrono::FixedOffset>,
    min_height_frac: f64,
) -> Vec<PositionedOccurrence> {
    use chrono::Timelike;

    occs.sort_by_key(|o| o.start);

    // Convert to minute ranges, splitting nothing across midnight (the
    // caller queries per-day; occurrences starting before the day clamp 0).
    let ranges: Vec<(f64, f64, Occurrence)> = occs
        .into_iter()
        .map(|o| {
            let start_min = (o.start.hour() as f64) * 60.0
                + (o.start.minute() as f64)
                + (o.start.second() as f64) / 60.0;
            let end_dt = o.end.unwrap_or(o.start + chrono::Duration::hours(1));
            let end_min = ((end_dt - o.start).num_seconds() as f64 / 60.0 + start_min)
                .clamp(start_min, DAY_MINUTES);
            (start_min.min(DAY_MINUTES), end_min.max(start_min), o)
        })
        .collect();

    // Lane assignment below is computed per overlap-cluster in a second
    // pass (rank of this event among everything it overlaps).

    // Recompute per-cluster lane counts with a simple second pass: for each
    // event count how many events overlap it (including itself); its lane
    // count is that number clamped to [1, lanes_used_in_cluster].
    let mut out = Vec::with_capacity(ranges.len());
    for i in 0..ranges.len() {
        let (si, ei, _) = &ranges[i];
        let overlapping: Vec<usize> = (0..ranges.len())
            .filter(|&j| {
                let (sj, ej, _) = &ranges[j];
                sj < ei && ej > si // open-ended overlap
            })
            .collect();
        let lanes_needed = overlapping.len().max(1);
        // Lane index = rank of i among overlapping by (start, id-order).
        let mut order: Vec<usize> = overlapping.clone();
        order.sort_by(|&a, &b| (ranges[a].0, a).partial_cmp(&(ranges[b].0, b)).unwrap());
        let lane = order.iter().position(|&x| x == i).unwrap_or(0);
        out.push(PositionedOccurrence {
            occ: ranges[i].2.clone(),
            top_frac: si / DAY_MINUTES,
            height_frac: ((ei - si) / DAY_MINUTES).max(min_height_frac),
            lane,
            lanes: lanes_needed,
        });
    }
    out
}

#[cfg(test)]
mod day_layout_tests {
    use super::*;
    use crate::models::{datetime_from_parts, Occurrence};
    use chrono::FixedOffset;

    fn occ(y: i32, mo: u32, d: u32, start_h: u32, dur_min: i64) -> Occurrence {
        let start = datetime_from_parts(y, mo, d, start_h, 0, 0).unwrap();
        Occurrence {
            item_id: ulid_like(),
            start,
            end: Some(start + chrono::Duration::minutes(dur_min)),
        }
    }

    // Deterministic stand-in ids (real ULIDs so nothing downstream chokes).
    fn ulid_like() -> crate::models::Ulid {
        crate::models::Ulid::from_string("01ARZ3NDEKTSV4RRFFQ69G5FAV").unwrap()
    }

    fn day() -> chrono::DateTime<FixedOffset> {
        datetime_from_parts(2026, 8, 24, 0, 0, 0).unwrap()
    }

    #[test]
    fn single_event_spans_full_width_at_right_offset() {
        let e = occ(2026, 8, 24, 10, 90);
        let out = layout_day(vec![e.clone()], day(), 0.02);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].lane, 0);
        assert_eq!(out[0].lanes, 1);
        let expect_top = (10.0 * 60.0) / 1440.0;
        assert!((out[0].top_frac - expect_top).abs() < 1e-9);
        let expect_h = 90.0 / 1440.0;
        assert!((out[0].height_frac - expect_h).abs() < 1e-9);
    }

    #[test]
    fn overlapping_events_share_lanes() {
        let a = occ(2026, 8, 24, 9, 120); // 09:00–11:00
        let b = occ(2026, 8, 24, 10, 60); // 10:00–11:00 overlaps a
        let c = occ(2026, 8, 24, 13, 30); // separate cluster
        let mut evs = vec![c, b.clone(), a.clone()];
        evs.sort_by_key(|o| o.start);
        let out = layout_day(evs, day(), 0.02);

        let lane_of = |s: u32| out.iter().find(|p| p.occ.start.hour() == s).unwrap();
        assert_eq!(lane_of(9).lanes, 2);
        assert_eq!(lane_of(10).lanes, 2);
        assert_ne!(lane_of(9).lane, lane_of(10).lane);
        assert_eq!(lane_of(13).lanes, 1); // own cluster
    }

    #[test]
    fn min_height_enforced_for_short_events() {
        let five_min = occ(2026, 8, 24, 22, 5);
        let out = layout_day(vec![five_min], day(), 0.02);
        assert!(out[0].height_frac >= 0.02 - 1e-9);
    }

    use chrono::Timelike as _;
}
