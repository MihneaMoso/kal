//! Tests for model validation and error handling in kal-core.

use kal_core::models::{
    Calendar, CalendarItem, CalendarSource, Color, DateTimeTz, ItemKind, Reminder,
};

#[test]
fn calendar_validation_requires_non_empty_name() {
    let cal = Calendar {
        id: ulid::Ulid::new(),
        name: "".to_string(),
        color: Color::default(),
        source: CalendarSource::Local,
        visible: true,
        deleted: false,
        updated_at: DateTimeTz::default(),
    };

    assert!(cal.validate().is_err(), "empty name should fail validation");
}

#[test]
fn calendar_validation_accepts_valid_calendars() {
    let cal = Calendar::local("Test Calendar", Color::default());

    assert!(
        cal.validate().is_ok(),
        "valid calendar should pass validation"
    );
}

#[test]
fn calendar_validation_rejects_empty_color() {
    let cal = Calendar {
        id: ulid::Ulid::new(),
        name: "Test".to_string(),
        color: Color::from_hex("INVALID").unwrap(),
        source: CalendarSource::Local,
        visible: true,
        deleted: false,
        updated_at: DateTimeTz::default(),
    };

    assert!(
        cal.validate().is_err(),
        "invalid color should fail validation"
    );
}

#[test]
fn item_validation_requires_non_empty_title() {
    let mut item = CalendarItem::new(
        ItemKind::Event,
        "".to_string(),
        ulid::Ulid::new(),
        DateTimeTz::default(),
    );

    assert!(
        item.validate().is_err(),
        "empty title should fail validation"
    );
}

#[test]
fn item_validation_rejects_end_before_start() {
    let mut item = CalendarItem::new(
        ItemKind::Event,
        "Test".to_string(),
        ulid::Ulid::new(),
        DateTimeTz::default(),
    );
    item.start = DateTimeTz::from_rfc3339("2026-01-02T10:00:00+00:00").unwrap();
    item.end = Some(DateTimeTz::from_rfc3339("2026-01-01T10:00:00+00:00").unwrap());

    assert!(
        item.validate().is_err(),
        "end before start should fail validation"
    );
}

#[test]
fn item_validation_all_day_items_cannot_have_times() {
    let mut item = CalendarItem::new(
        ItemKind::Event,
        "Test".to_string(),
        ulid::Ulid::new(),
        DateTimeTz::default(),
    );
    item.all_day = true;
    item.start = DateTimeTz::from_rfc3339("2026-01-01T10:00:00+00:00").unwrap();
    item.end = Some(DateTimeTz::from_rfc3339("2026-01-01T11:00:00+00:00").unwrap());

    assert!(
        item.validate().is_err(),
        "all-day item cannot have time components"
    );
}

#[test]
fn item_validation_all_day_items_must_be_on_same_date() {
    let mut item = CalendarItem::new(
        ItemKind::Event,
        "Test".to_string(),
        ulid::Ulid::new(),
        DateTimeTz::default(),
    );
    item.all_day = true;
    item.start = DateTimeTz::from_rfc3339("2026-01-01T00:00:00+00:00").unwrap();
    item.end = Some(DateTimeTz::from_rfc3339("2026-01-02T00:00:00+00:00").unwrap());

    assert!(
        item.validate().is_err(),
        "all-day item must be on same date"
    );
}

#[test]
fn item_validation_tasks_can_have_no_end() {
    let mut item = CalendarItem::new(
        ItemKind::Task,
        "Test".to_string(),
        ulid::Ulid::new(),
        DateTimeTz::default(),
    );
    item.all_day = false;
    // No end time set

    assert!(item.validate().is_ok(), "task can have no end");
}

#[test]
fn item_validation_tasks_with_end_must_be_valid_times() {
    let mut item = CalendarItem::new(
        ItemKind::Task,
        "Test".to_string(),
        ulid::Ulid::new(),
        DateTimeTz::default(),
    );
    item.start = DateTimeTz::from_rfc3339("2026-01-01T10:00:00+00:00").unwrap();
    item.end = Some(DateTimeTz::from_rfc3339("2026-01-01T11:00:00+00:00").unwrap());

    assert!(item.validate().is_ok(), "task with valid times should pass");
}

#[test]
fn reminder_validation_minutes_before_accepts_positive_values() {
    let reminder = Reminder::minutes_before(10);
    assert!(reminder.validate().is_ok());
}

#[test]
fn reminder_validation_minutes_before_rejects_zero() {
    let reminder = Reminder::minutes_before(0);
    assert!(reminder.validate().is_err());
}

#[test]
fn reminder_validation_minutes_before_rejects_negative() {
    let reminder = Reminder::minutes_before(-10);
    assert!(reminder.validate().is_err());
}

#[test]
fn reminder_validation_custom_reject_zero() {
    let reminder = Reminder::custom(0);
    assert!(reminder.validate().is_err());
}

#[test]
fn reminder_validation_custom_accepts_positive() {
    let reminder = Reminder::custom(10);
    assert!(reminder.validate().is_ok());
}

#[test]
fn reminder_validation_custom_rejects_negative() {
    let reminder = Reminder::custom(-10);
    assert!(reminder.validate().is_err());
}

#[test]
fn item_with_valid_reminders_passes_validation() {
    let mut item = CalendarItem::new(
        ItemKind::Event,
        "Test".to_string(),
        ulid::Ulid::new(),
        DateTimeTz::default(),
    );
    item.reminders = vec![Reminder::minutes_before(10), Reminder::minutes_before(30)];

    assert!(item.validate().is_ok());
}

#[test]
fn item_with_invalid_reminders_fails_validation() {
    let mut item = CalendarItem::new(
        ItemKind::Event,
        "Test".to_string(),
        ulid::Ulid::new(),
        DateTimeTz::default(),
    );
    item.reminders = vec![
        Reminder::minutes_before(10),
        Reminder::custom(-5), // Invalid
    ];

    assert!(item.validate().is_err());
}

#[test]
fn item_kind_enum_values_are_valid() {
    let valid_kinds = vec![ItemKind::Event, ItemKind::Task, ItemKind::Birthday];

    for kind in valid_kinds {
        assert!(matches!(
            kind,
            ItemKind::Event | ItemKind::Task | ItemKind::Birthday
        ));
    }
}

#[test]
fn color_validation_accepts_valid_hex_colors() {
    let valid_colors = vec![
        "#FF0000".to_string(),
        "#00FF00".to_string(),
        "#0000FF".to_string(),
        "#123456".to_string(),
        "#FFFFFF".to_string(),
        "#000000".to_string(),
    ];

    for hex in valid_colors {
        let color = Color::from_hex(&hex).unwrap();
        assert!(color.validate().is_ok());
    }
}

#[test]
fn color_validation_rejects_invalid_hex() {
    let invalid_colors = vec![
        "#GGGGGG".to_string(),
        "#12345".to_string(),
        "#1234567".to_string(),
        "not-a-color".to_string(),
        "".to_string(),
    ];

    for hex in invalid_colors {
        let color = Color::from_hex(&hex);
        assert!(color.is_err(), "should reject: {}", hex);
    }
}

#[test]
fn default_calendars_have_correct_ids() {
    assert_eq!(kal_core::DEFAULT_PERSONAL_ID.len(), 26);
    assert_eq!(kal_core::DEFAULT_BIRTHDAYS_ID.len(), 26);

    let personal = kal_core::models::Calendar::local("Personal", Color::default());
    let birthdays = kal_core::models::Calendar::birthdays();

    assert_eq!(personal.id.to_string(), kal_core::DEFAULT_PERSONAL_ID);
    assert_eq!(birthdays.id.to_string(), kal_core::DEFAULT_BIRTHDAYS_ID);
}
