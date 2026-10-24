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
//!   before the occurrence, then create the edited series from it.
//! - `all` ([`anchored_on_series`] + [`apply_all_edit`]): the base row is
//!   rewritten, so it keeps the series' own anchor date (the editor form is
//!   anchored on the clicked occurrence), and the dialog's shared fields are
//!   carried to every exception of the series.
//!
//! As before, the writes are not transactional (a failure midway leaves the
//! earlier writes applied).

use chrono::{TimeZone, Utc};
use kal_core::models::{CalendarItem, DateTimeTz};
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

    if and_following {
        // Truncate the base series right before the edited occurrence.
        if !base.rrule.as_deref().unwrap_or("").contains("COUNT=")
            && !base.rrule.as_deref().unwrap_or("").contains("UNTIL=")
        {
            let until = (occ_start - chrono::Duration::minutes(1))
                .with_timezone(&Utc)
                .format("%Y%m%dT%H%M%SZ")
                .to_string();
            let rule = base.rrule.take().unwrap_or_default();
            base.rrule = Some(format!("{rule};UNTIL={until}"));
        }
    } else {
        // Exclude just this occurrence from the base series.
        base.exdates.push(occ_start);
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
        // A split starts a series in its own right.
        new_item.metadata.series_id = None;
        new_item.metadata.recurrence_id = None;
    }
    store.scoped_upsert_item(&new_item)?;
    Ok(true)
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
    let anchored = anchored_on_series(&base, edited);

    let mut series = base.clone();
    copy_shared_fields(&mut series, edited);
    series.start = anchored.start;
    series.end = anchored.end;
    series.rrule = edited.rrule.clone();
    series.updated_at = now;
    store.scoped_upsert_item(&series)?;

    // How far the series' own clock moved; its exceptions move with it, keeping
    // the dates the user gave them.
    let shift = series.start - base.start;
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
