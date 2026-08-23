/* Kal — stable C ABI surface consumed by native widget shims.
 * Regenerate with cbindgen if signatures change; keep in sync manually until
 * cbindgen is added to CI. */

#ifndef KAL_FFI_H
#define KAL_FFI_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* Opaque database handle. */
typedef struct KalDb KalDb;

/* Open (or create) the calendar database at `path` (UTF-8, NUL-terminated).
 * Returns NULL on failure. */
KalDb *kal_open(const char *path);

/* Close the handle and NULL out the caller's slot; double-close is safe. */
void kal_close(KalDb **db);

/* Free any string returned by the kal_* functions below. */
void kal_free(char *s);

/*
 * Upcoming occurrences overlapping [from_epoch, to_epoch] as a JSON array:
 * [{ "id": str, "title": str, "start": rfc3339, "end": rfc3339|null,
 *    "allDay": bool, "kind": "event"|"task"|"birthday",
 *    "color": "#RRGGBB", "age": int|null }]
 * Sorted by start. Recurring rules are expanded. Returns NULL on error;
 * free with kal_free().
 */
char *kal_upcoming_json(KalDb *db, int64_t from_epoch, int64_t to_epoch);

/*
 * Month grid as JSON rows (6 weeks x 7 days):
 * [{ "date": "YYYY-MM-DD", "inMonth": bool,
 *    "items": [{ "id": str, "title": str, "time": "HH:MM"|"" }] }]
 * first_dow: 0 = Monday ... 6 = Sunday. Returns NULL on error.
 */
char *kal_month_grid_json(KalDb *db, int32_t year, uint32_t month, uint8_t first_dow);

#ifdef __cplusplus
}
#endif

#endif /* KAL_FFI_H */
