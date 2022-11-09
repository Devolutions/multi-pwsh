use time::format_description::well_known::Iso8601;
use time::macros::{date, time};
use time::PrimitiveDateTime;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DateTime {
    inner: PrimitiveDateTime,
}

impl DateTime {
    pub fn parse(input: &str) -> Option<Self> {
        let inner = time::PrimitiveDateTime::parse(input, &Iso8601::DEFAULT).ok()?;
        Some(Self { inner: inner })
    }

    pub fn format(&self) -> String {
        self.inner.format(&Iso8601::DEFAULT).unwrap()
    }
}

impl Default for DateTime {
    fn default() -> Self {
        let inner = PrimitiveDateTime::new(date!(2000 - 01 - 01), time!(0:00));
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
            DateTime::parse("2022-11-09 01:23:45").unwrap().format(),
            "2022-11-09 01:23:45".to_string()
        )
    }
}
