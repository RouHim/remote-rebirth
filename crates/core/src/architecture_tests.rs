//! Tests for the architecture-16 primitives.

use time::{Date, Month, OffsetDateTime, PrimitiveDateTime, Time};

use crate::architecture::*;

fn moment(year: i32) -> OffsetDateTime {
    let date = Date::from_calendar_date(year, Month::November, 5).expect("a valid date");
    let time = Time::from_hms(12, 34, 56).expect("a valid time");
    PrimitiveDateTime::new(date, time).assume_utc()
}

#[test]
fn the_stored_clock_fields_count_the_day_and_weekday_from_zero() {
    let fields = clock_fields(&moment(2026)).expect("2026 is storable");
    assert_eq!(fields, [56, 34, 12, 4, 4, 10, 26]);
}

#[test]
fn a_stamp_outside_the_clock_range_is_refused() {
    assert!(clock_fields(&moment(1999)).is_err());
    assert!(clock_fields(&moment(2256)).is_err());
}
