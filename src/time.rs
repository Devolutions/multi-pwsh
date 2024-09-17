use time::format_description::well_known::Iso8601;
use time::macros::{date, time, format_description};
use time::{OffsetDateTime, PrimitiveDateTime, UtcOffset};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DateTime {
    inner: OffsetDateTime,
}

#[allow(dead_code)] 
impl DateTime {
    pub fn parse(input: &str) -> Option<Self> {
        // Try parsing as OffsetDateTime (with timezone)
        if let Ok(inner) = OffsetDateTime::parse(input, &Iso8601::DEFAULT) {
            return Some(Self { inner });
        }
        
        // If parsing without timezone, assume UTC (Offset +00:00)
        let format = format_description!("[year]-[month]-[day]T[hour]:[minute]:[second].[subsecond]");
        if let Ok(primitive) = PrimitiveDateTime::parse(input, &format) {
            let inner = primitive.assume_offset(UtcOffset::UTC);
            return Some(Self { inner });
        }

        None
    }

    pub fn format(&self) -> String {
        let format = format_description!("[year]-[month]-[day]T[hour]:[minute]:[second].[subsecond digits:7][offset_hour sign:mandatory]:[offset_minute]");
        self.inner.format(&format).unwrap()
    }
}

impl Default for DateTime {
    fn default() -> Self {
        let inner = OffsetDateTime::new_utc(date!(2000 - 01 - 01), time!(0:00));
        DateTime { inner: inner }
    }
}

pub fn parse_iso8601_duration(input: &str) -> Option<std::time::Duration> {
    iso8601_duration::Duration::parse(input)
        .ok()
        .map(|x| x.to_std())
}

#[cfg(test)]
mod pwsh {
    use crate::time::parse_iso8601_duration;
    use crate::time::DateTime;

    #[test]
    fn parse_duration() {
        // 0 seconds
        assert_eq!(
            parse_iso8601_duration("PT0S"),
            Some(std::time::Duration::new(0, 0))
        );

        // 9 seconds, 26.9026 milliseconds
        assert_eq!(
            parse_iso8601_duration("PT9.0269026S"),
            Some(std::time::Duration::from_secs_f32(9.0269026))
        );
    }

    #[test]
    fn parse_datetime() {
        assert_eq!(
            DateTime::parse("2024-09-17T10:55:56.7639518-04:00").unwrap().format(),
            "2024-09-17T10:55:56.7639518-04:00".to_string()
        )
    }
}
