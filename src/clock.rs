use time::{OffsetDateTime, format_description::well_known::Rfc3339};

use crate::error::Error;

pub trait Clock: Send + Sync {
    fn now(&self) -> OffsetDateTime;
}

pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> OffsetDateTime {
        OffsetDateTime::now_utc()
    }
}

pub fn format_rfc3339(value: &OffsetDateTime) -> Result<String, Error> {
    value
        .format(&Rfc3339)
        .map_err(|error| Error::Time(error.to_string()))
}
