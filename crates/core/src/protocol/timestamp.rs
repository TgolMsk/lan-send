//! RFC 3339 timestamps, the format the protocol uses for file metadata.

use std::time::SystemTime;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

pub(crate) fn parse(value: &str) -> Option<SystemTime> {
    OffsetDateTime::parse(value, &Rfc3339)
        .ok()
        .map(SystemTime::from)
}

pub(crate) fn format(value: SystemTime) -> Option<String> {
    OffsetDateTime::from(value).format(&Rfc3339).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn round_trips_with_fraction() {
        let time = SystemTime::UNIX_EPOCH + Duration::from_millis(500);
        let text = format(time).unwrap();
        assert_eq!(text, "1970-01-01T00:00:00.5Z");
        assert_eq!(parse(&text), Some(time));
        assert_eq!(parse("1970-01-01T00:00:00.500Z"), Some(time));
        assert_eq!(
            parse("2000-01-01T01:00:00+01:00"),
            parse("2000-01-01T00:00:00Z")
        );
        assert_eq!(parse("yesterday"), None);
    }
}
