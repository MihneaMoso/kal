//! Scoped edits to a recurring series (Google Calendar semantics), shared by
//! both storage backends.
//!
//! A scoped edit is pure model surgery over two rows, so it must not exist twice
//! (once for SQLite and once for the browser store): the Dioxus editor and the
//! Android bridge both rely on this exact behaviour.
//!
//! - `this_only`: EXDATE the clicked occurrence out of the base series and
//!   create a fresh single item carrying the edited values. That item keeps a
//!   link to its series ([`ItemMetadata::series_id`]), so it is an *exception*
//!   rather than an unrelated event: opening it still offers the scope choice,
//!   "This event" keeps editing just it, and "All events" reaches it.
//! - `this_and_following`: truncate the base series with an `UNTIL` just
//!   before the occurrence, then create the edited series from it with the
//!   original end (an existing `UNTIL`, or a `COUNT`'s last occurrence) carried
//!   onto the tail.
//! - `all` ([`anchored_on_series`] + [`apply_all_edit`]): a recurring base row
//!   keeps the series' own anchor date (the editor form is anchored on the
//!   clicked occurrence), a plain item takes the dialog's date, and the
//!   dialog's shared fields are carried to every exception of the series.
//!
//! [`delete_scoped`] is the delete-side counterpart: it EXDATEs, truncates or
//! tombstones, and never creates a replacement row.
//!
//! As before, the writes are not transactional (a failure midway leaves the
//! earlier writes applied).

use chrono::{TimeZone, Utc};
use kal_core::models::{CalendarItem, DateTimeTz};
use kal_core::viewmodel::series_end;
use ulid::Ulid;

use crate::Result;

/// The row operations a scoped edit needs, so either backend can run the same
/// algorithm. Implemented by both [`crate::Database`] backends.
pub trait ScopedStore {
    fn scoped_get_item(&self, id: Ulid) -> Result<Option<CalendarItem>>;
    fn scoped_upsert_item(&self, item: &CalendarItem) -> Result<()>;
    /// Live (non-tombstoned) items; the series' exceptions are found by their
    /// `metadata.series_id`, so no extra query surface is needed.
    fn scoped_list_items(&self) -> Result<Vec<CalendarItem>>;
    fn scoped_soft_delete_item(&self, id: Ulid) -> Result<bool>;
}

/// Which part of a recurring item a delete applies to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeleteScope {
    /// Only the clicked occurrence.
    ThisOnly,
    /// The clicked occurrence and every later one.
    ThisAndFollowing,
    /// The whole series, including occurrences detached from it.
    All,
}

/// Apply a scoped edit to `base_id`. Returns `Ok(false)` when the base row no
/// longer exists.
pub fn apply_scoped_edit<S: ScopedStore + ?Sized>(
    store: &S,
    base_id: Ulid,
    edited: &CalendarItem,
    occ_start: DateTimeTz,
    and_following: bool,
) -> Result<bool> {
    let now = Utc::now().fixed_offset();
    let Some(mut base) = store.scoped_get_item(base_id)? else {
        return Ok(false);
    };

    // The tail created by a split must keep the original series' end. Capture
    // it before the base rule is rewritten: an existing UNTIL is exactly what
    // the old code left untouched, so the base kept running through the split
    // while the copy started a second, unbounded series.
    let tail_end = and_following.then(|| series_end(&base)).flatten();

    if and_following {
        // Truncate the base series right before the edited occurrence, even
        // when it already has an UNTIL or COUNT. `truncate_rule_before`
        // converts COUNT to the equivalent UNTIL and never extends a series
        // that already ended earlier.
        let Some(rule) = truncate_rule_before(&base, occ_start) else {
            return Ok(false);
        };
        base.rrule = Some(rule);
        // EXDATEs at/after the split belong to the tail. The branch below
        // copies them there before dropping them here.
        base.exdates
            .retain(|d| d.timestamp() < occ_start.timestamp());
    } else {
        // Exclude just this occurrence from the base series.
        if !has_instant(&base.exdates, occ_start) {
            base.exdates.push(occ_start);
        }
    }
    base.updated_at = now;
    store.scoped_upsert_item(&base)?;

    // The new item/series carries the edited values.
    let mut new_item = edited.clone();
    new_item.id = Ulid::new();
    new_item.created_at = now;
    new_item.updated_at = now;
    if !and_following {
        new_item.rrule = None;
        new_item.exdates.clear();
        // The copy is one occurrence *of this series*, not an unrelated event:
        // remember the series and the excluded instant, so reopening it offers
        // the scope choice and a later "All events" reaches it.
        new_item.metadata.series_id = Some(base_id);
        new_item.metadata.recurrence_id = Some(occ_start);
    } else {
        // A split starts a series in its own right, but it must stop where the
        // original did. Keep the tail's own EXDATEs (the occurrences after the
        // split that were individually deleted) and drop the ones that belonged
        // to the now-truncated base.
        new_item.metadata.series_id = None;
        new_item.metadata.recurrence_id = None;
        new_item
            .exdates
            .retain(|d| d.timestamp() >= occ_start.timestamp());
        if let Some(end) = tail_end {
            if end.timestamp() < new_item.start.timestamp() {
                // The original series ended before the edited occurrence; keep
                // just that one item rather than an empty tail.
                new_item.rrule = None;
            } else if let Some(rule) = new_item.rrule.as_deref() {
                new_item.rrule = Some(with_tail_end(rule, end));
            }
        }
    }
    store.scoped_upsert_item(&new_item)?;
    Ok(true)
}

/// Rewrite `base`'s rule so the series ends before `split`.
///
/// A `COUNT` becomes the equivalent `UNTIL` (the earlier of the original count
/// end, an existing `UNTIL`, and the split cutoff), which keeps the rule valid
/// and, crucially, never extends a series that already ended before the split.
/// Returns `None` when there is no rule to truncate.
fn truncate_rule_before(base: &CalendarItem, split: DateTimeTz) -> Option<String> {
    let rule = base.rrule.as_deref()?;
    let cutoff = split - chrono::Duration::minutes(1);
    let bound = match series_end(base) {
        // The series already ended before the split: keep its real end (and its
        // last occurrence) instead of pulling the end back to the cutoff.
        Some(end) if end.timestamp() < split.timestamp() => end,
        _ => cutoff,
    };
    let until = bound
        .with_timezone(&Utc)
        .format("%Y%m%dT%H%M%SZ")
        .to_string();
    let mut parts: Vec<String> = rule
        .split(';')
        .filter(|part| {
            let upper = part.to_ascii_uppercase();
            !part.is_empty() && !upper.starts_with("UNTIL=") && !upper.starts_with("COUNT=")
        })
        .map(str::to_string)
        .collect();
    if parts.is_empty() {
        return None;
    }
    parts.push(format!("UNTIL={until}"));
    Some(parts.join(";"))
}

/// Carry the original series' end onto the new tail.
///
/// The dialog shows the series' own rule, so the tail may arrive carrying the
/// original `COUNT`/`UNTIL` verbatim. Those bounds describe the old whole
/// series, not the tail; replace them with the original end captured before the
/// split. (The UI has no end-date field, so no user-authored bound is lost.)
fn with_tail_end(rule: &str, end: DateTimeTz) -> String {
    let until = end.with_timezone(&Utc).format("%Y%m%dT%H%M%SZ").to_string();
    let mut parts: Vec<String> = rule
        .split(';')
        .filter(|part| {
            let upper = part.to_ascii_uppercase();
            !part.is_empty() && !upper.starts_with("UNTIL=") && !upper.starts_with("COUNT=")
        })
        .map(str::to_string)
        .collect();
    if parts.is_empty() {
        return rule.to_string();
    }
    parts.push(format!("UNTIL={until}"));
    parts.join(";")
}

/// Whether `list` already contains `instant`. Compare instants rather than the
/// stored offsets: the same occurrence can arrive with a different fixed offset
/// (the editor uses the device's current offset).
fn has_instant(list: &[DateTimeTz], instant: DateTimeTz) -> bool {
    list.iter().any(|d| d.timestamp() == instant.timestamp())
}

/// Copy the fields an "All events" save shares with every row of the series:
/// everything the editor dialog shows except the date/time (each occurrence
/// keeps its own) and the repeat rule (which only the series row carries).
fn copy_shared_fields(target: &mut CalendarItem, edited: &CalendarItem) {
    target.kind = edited.kind;
    target.title = edited.title.clone();
    target.location = edited.location.clone();
    target.calendar_id = edited.calendar_id;
    target.all_day = edited.all_day;
    target.reminders = edited.reminders.clone();
    target.color_override = edited.color_override.clone();
    // The one other dialog field that lives in metadata: a birthday's person.
    // (The series link in `metadata` is deliberately not touched here — it
    // belongs to the row, not to the dialog.)
    target.metadata.birthday_of = edited.metadata.birthday_of.clone();
}

/// The end instant for an edit whose dialog has one date field: keep the row's
/// stored day-span (the number of days between its start and end dates) and
/// apply the dialog's end time to the shifted end date. Without this, every
/// save of a multi-day item collapsed its end onto the start day, because the
/// form carries no second date to preserve.
fn end_for_edit(
    base: &CalendarItem,
    edited: &CalendarItem,
    start: DateTimeTz,
) -> Option<DateTimeTz> {
    let edited_end = edited.end?;
    let days = base
        .end
        .map(|end| {
            (end.date_naive() - base.start.date_naive())
                .num_days()
                .max(0)
        })
        .unwrap_or(0);
    let date = start.date_naive() + chrono::Duration::days(days);
    date.and_time(edited_end.time())
        .and_local_timezone(*start.offset())
        .single()
        .map(|dt| dt.fixed_offset())
}

/// "All events": rewrite the series row and carry the dialog's shared fields to
/// every occurrence detached from it.
///
/// `edited` may be the series row itself or one of its exceptions (the dialog
/// sends whichever row was opened, and the exception's link names the series).
///
/// Each exception keeps its own date — that is what made it an exception — and
/// a series-wide time-of-day change moves its clock by the same amount, so a
/// recolour reaches everyone without re-dating anybody. The series' rule and
/// EXDATEs always come from the series row: the dialog shows the series' rule
/// while editing an exception, so writing the form row verbatim would turn that
/// exception into a second series and drop the exclusions.
///
/// Returns `Ok(false)` when neither row exists (the caller saves plainly). When
/// the series row is gone but `edited` is an exception, the occurrence becomes
/// an ordinary event (its link is cleared) and this returns `Ok(true)`.
pub fn apply_all_edit<S: ScopedStore + ?Sized>(store: &S, edited: &CalendarItem) -> Result<bool> {
    let series_id = edited.metadata.series_id.unwrap_or(edited.id);
    let base = match store.scoped_get_item(series_id)? {
        Some(base) if !base.deleted => base,
        _ if edited.metadata.series_id.is_some() => {
            // The series was deleted underneath us: the occurrence is a plain
            // event now, and `save_exception` detaches it cleanly.
            save_exception(store, edited)?;
            return Ok(true);
        }
        _ => return Ok(false),
    };

    let now = Utc::now().fixed_offset();

    let mut series = base.clone();
    copy_shared_fields(&mut series, edited);
    if base.rrule.is_some() {
        // A recurring series is edited from one of its occurrences, so the
        // series keeps its own anchor date and takes only the dialog's
        // wall-clock time.
        let anchored = anchored_on_series(&base, edited);
        series.start = anchored.start;
        series.end = end_for_edit(&base, edited, anchored.start);
    } else {
        // A plain item is edited from its own row, so the dialog's date is
        // authoritative. (The dialog has a single date field; `end_for_edit`
        // carries a multi-day span over instead of collapsing it.)
        series.start = edited.start;
        series.end = end_for_edit(&base, edited, edited.start);
    }
    series.rrule = edited.rrule.clone();
    // How far the series' own clock moved; its exceptions move with it, keeping
    // the dates the user gave them. EXDATEs and recurrence ids are instants of
    // the old clock, so they move by the same amount: otherwise a time-of-day
    // edit would leave every exclusion pointing at an occurrence the rule no
    // longer generates, and deleted occurrences would reappear at the new time.
    let shift = series.start - base.start;
    if base.rrule.is_some() && shift != chrono::Duration::zero() {
        for exdate in &mut series.exdates {
            *exdate += shift;
        }
    }
    series.updated_at = now;
    store.scoped_upsert_item(&series)?;

    for mut occurrence in store.scoped_list_items()? {
        if occurrence.metadata.series_id != Some(series.id) {
            continue;
        }
        copy_shared_fields(&mut occurrence, edited);
        if occurrence.id == edited.id {
            // The row the dialog was opened on: take its date and time from the
            // form (the stored row still holds the pre-edit values) and do not
            // shift it a second time.
            occurrence.start = edited.start;
            occurrence.end = edited.end;
        } else if shift != chrono::Duration::zero() {
            // Every other exception follows the series' clock on its own date.
            occurrence.start += shift;
            if let Some(end) = occurrence.end {
                occurrence.end = Some(end + shift);
            }
        }
        // The exception's position in the series moves with the series clock,
        // even when its own date was moved away from that occurrence.
        if let Some(recurrence_id) = occurrence.metadata.recurrence_id {
            occurrence.metadata.recurrence_id = Some(recurrence_id + shift);
        }
        // An exception is always a single item, whatever repeat preset the
        // dialog carried (it shows the series' rule).
        occurrence.rrule = None;
        occurrence.exdates.clear();
        occurrence.updated_at = now;
        store.scoped_upsert_item(&occurrence)?;
    }
    Ok(true)
}

/// "This event" on an occurrence that is already detached: update that row in
/// place, keeping its series link so "All events" still reaches it and it keeps
/// offering the scope choice.
///
/// The row stays single whatever repeat preset the dialog carried: while editing
/// an exception the preset shows the *series'* rule, and that rule belongs to the
/// series row only. Returns `Ok(false)` when `edited` is not an exception.
pub fn save_exception<S: ScopedStore + ?Sized>(store: &S, edited: &CalendarItem) -> Result<bool> {
    let Some(series_id) = edited.metadata.series_id else {
        return Ok(false);
    };
    let mut row = edited.clone();
    row.rrule = None;
    row.exdates.clear();
    if !matches!(store.scoped_get_item(series_id)?, Some(base) if !base.deleted) {
        // The series is gone; the occurrence becomes an ordinary event.
        row.metadata.series_id = None;
        row.metadata.recurrence_id = None;
    }
    row.updated_at = Utc::now().fixed_offset();
    store.scoped_upsert_item(&row)?;
    Ok(true)
}

/// Tombstone an item, and when it is a series also tombstone the occurrences
/// detached from it: a series delete must not leave its "This event" edits
/// behind as orphans.
///
/// Deleting a detached occurrence deletes only that occurrence — the series
/// keeps its EXDATE, so the excluded instant stays gone (as in Google
/// Calendar). Returns whether a row was written, like `soft_delete_item`.
pub fn delete_with_exceptions<S: ScopedStore + ?Sized>(store: &S, id: Ulid) -> Result<bool> {
    let deleted = store.scoped_soft_delete_item(id)?;
    if deleted {
        // Read after the tombstone: the list still holds the series' live
        // exceptions but no longer the row just deleted.
        for occurrence in store.scoped_list_items()? {
            if occurrence.metadata.series_id == Some(id) {
                store.scoped_soft_delete_item(occurrence.id)?;
            }
        }
    }
    Ok(deleted)
}

/// Delete one item, or one part of a recurring item.
///
/// `occ_start` is the clicked occurrence. For a detached occurrence its
/// `metadata.recurrence_id` wins, because that is its position in the series
/// even when the user moved the occurrence elsewhere.
///
/// - `ThisOnly` excludes a base-series occurrence, or tombstones one detached
///   occurrence.
/// - `ThisAndFollowing` truncates the series before `occ_start` and tombstones
///   every detached occurrence at/after it; it never creates a replacement row.
/// - `All` from a detached occurrence deletes its whole series, matching the
///   scope the editor offers for a recurring item.
pub fn delete_scoped<S: ScopedStore + ?Sized>(
    store: &S,
    id: Ulid,
    occ_start: Option<DateTimeTz>,
    scope: DeleteScope,
) -> Result<bool> {
    let target = store.scoped_get_item(id)?;
    match scope {
        DeleteScope::All => {
            let series_id = target
                .as_ref()
                .and_then(|item| item.metadata.series_id)
                .unwrap_or(id);
            let deleted = delete_with_exceptions(store, series_id)?;
            if !deleted && series_id != id {
                // The series is already gone; remove the orphaned occurrence.
                return store.scoped_soft_delete_item(id);
            }
            Ok(deleted)
        }
        DeleteScope::ThisOnly => match target {
            Some(target) => delete_one(store, target, occ_start),
            None => Ok(false),
        },
        DeleteScope::ThisAndFollowing => match target {
            Some(target) => delete_following(store, target, occ_start),
            None => Ok(false),
        },
    }
}

/// "This event" on a delete: remove just this occurrence.
fn delete_one<S: ScopedStore + ?Sized>(
    store: &S,
    mut target: CalendarItem,
    occ_start: Option<DateTimeTz>,
) -> Result<bool> {
    if let Some(series_id) = target.metadata.series_id {
        // A detached occurrence is its own row. Keep the series' EXDATE (and
        // add it if an import produced the row without one), then tombstone the
        // copy so the instant stays gone.
        let occurrence = target.metadata.recurrence_id.unwrap_or(target.start);
        if let Some(mut base) = store.scoped_get_item(series_id)? {
            if !base.deleted && !has_instant(&base.exdates, occurrence) {
                base.exdates.push(occurrence);
                base.updated_at = Utc::now().fixed_offset();
                store.scoped_upsert_item(&base)?;
            }
        }
        return store.scoped_soft_delete_item(target.id);
    }

    let occurrence = occ_start.unwrap_or(target.start);
    if target.rrule.is_none() {
        return delete_with_exceptions(store, target.id);
    }
    if !has_instant(&target.exdates, occurrence) {
        target.exdates.push(occurrence);
    }
    target.updated_at = Utc::now().fixed_offset();
    store.scoped_upsert_item(&target)?;
    // An imported RECURRENCE-ID can leave a live copy without an EXDATE on the
    // base; remove it too so "This event" really removes the instant.
    for row in store.scoped_list_items()? {
        if row.id != target.id && row.metadata.series_id == Some(target.id) {
            let at = row.metadata.recurrence_id.unwrap_or(row.start);
            if at.timestamp() == occurrence.timestamp() {
                store.scoped_soft_delete_item(row.id)?;
            }
        }
    }
    Ok(true)
}

/// "This and following" on a delete: truncate the series, then remove the
/// detached occurrences that belonged to the deleted tail.
fn delete_following<S: ScopedStore + ?Sized>(
    store: &S,
    target: CalendarItem,
    occ_start: Option<DateTimeTz>,
) -> Result<bool> {
    let (series_id, split) = match target.metadata.series_id {
        Some(series_id) => (
            series_id,
            target
                .metadata
                .recurrence_id
                .or(occ_start)
                .unwrap_or(target.start),
        ),
        None => (target.id, occ_start.unwrap_or(target.start)),
    };
    let Some(mut base) = store.scoped_get_item(series_id)? else {
        return store.scoped_soft_delete_item(target.id);
    };
    if base.deleted {
        return store.scoped_soft_delete_item(target.id);
    }
    if base.rrule.is_none() {
        return delete_with_exceptions(store, series_id);
    }
    if split.timestamp() <= base.start.timestamp() {
        // Deleting from the first occurrence onward is the whole series.
        return delete_with_exceptions(store, series_id);
    }
    let Some(rule) = truncate_rule_before(&base, split) else {
        return Ok(false);
    };
    base.rrule = Some(rule);
    base.exdates.retain(|d| d.timestamp() < split.timestamp());
    base.updated_at = Utc::now().fixed_offset();
    store.scoped_upsert_item(&base)?;

    for row in store.scoped_list_items()? {
        if row.id == series_id || row.metadata.series_id != Some(series_id) {
            continue;
        }
        let at = row.metadata.recurrence_id.unwrap_or(row.start);
        if at.timestamp() >= split.timestamp() {
            store.scoped_soft_delete_item(row.id)?;
        }
    }
    // The dialog can be opened on a moved detached occurrence whose original
    // recurrence is before the split; still remove the row the user deleted.
    if target.id != series_id && !target.deleted {
        if let Some(row) = store.scoped_get_item(target.id)? {
            if !row.deleted {
                store.scoped_soft_delete_item(target.id)?;
            }
        }
    }
    Ok(true)
}

/// The row an "All events" save should write: `edited`'s fields, moved back onto
/// the series' own start date.
///
/// The editor form is anchored on the **clicked occurrence** — deliberately, so
/// "This event" and "…and following" land on the day the user clicked — which
/// means an "All events" save arrives carrying that occurrence's date. Writing
/// it as-is moves the series start forward and strands every earlier occurrence
/// outside the new start: the reported "editing the whole series made my past
/// events disappear". The series therefore keeps its own calendar date and
/// offset, and takes the form's wall-clock times (and every other edited field);
/// an occurrence the user never sees is left where it was.
///
/// A date edit made while "All events" is selected is consequently *not* a way
/// to move the series: the series start is only ever shown as the clicked
/// occurrence, so its date field cannot mean "the new series start".
pub fn anchored_on_series(base: &CalendarItem, edited: &CalendarItem) -> CalendarItem {
    anchored_on_series_start(base.start, edited)
}

/// [`anchored_on_series`] from just the series' own start instant, for callers
/// that overwrite the fields of a loaded row in place.
pub fn anchored_on_series_start(base_start: DateTimeTz, edited: &CalendarItem) -> CalendarItem {
    let mut item = edited.clone();
    item.start = on_series_date(base_start, item.start);
    item.end = item.end.map(|e| on_series_date(base_start, e));
    item
}

/// `edited`'s wall-clock time on `base_start`'s calendar date, in the series'
/// own offset. A fixed offset is never ambiguous, so the fallback is unreachable.
fn on_series_date(base_start: DateTimeTz, edited: DateTimeTz) -> DateTimeTz {
    base_start
        .offset()
        .from_local_datetime(&base_start.date_naive().and_time(edited.time()))
        .single()
        .unwrap_or(edited)
}
