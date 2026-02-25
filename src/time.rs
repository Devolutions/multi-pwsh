use jiff::fmt::temporal::SpanParser;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DateTime {
    inner: String,
}

#[allow(dead_code)]
impl DateTime {
    pub fn parse(input: &str) -> Option<Self> {
        if input.parse::<jiff::Timestamp>().is_ok() {
            return Some(Self {
                inner: normalize_offset_datetime(input),
            });
        }

        if input.parse::<jiff::civil::DateTime>().is_ok() {
            return Some(Self {
                inner: format!("{}+00:00", input),
            });
        }

        None
    }

    pub fn format(&self) -> String {
        self.inner.clone()
    }
}

impl Default for DateTime {
    fn default() -> Self {
        DateTime {
            inner: "2000-01-01T00:00:00.0000000+00:00".to_string(),
        }
    }
}

pub fn parse_iso8601_duration(input: &str) -> Option<std::time::Duration> {
    static PARSER: SpanParser = SpanParser::new();

    PARSER.parse_unsigned_duration(input).ok()
}

fn normalize_offset_datetime(input: &str) -> String {
    if let Some(without_zulu) = input.strip_suffix('Z') {
        return format!("{}+00:00", without_zulu);
    }
    if let Some(without_zulu) = input.strip_suffix('z') {
        return format!("{}+00:00", without_zulu);
    }
    input.to_string()
}

#[cfg(test)]
mod pwsh {
    use crate::time::{parse_iso8601_duration, DateTime};

    #[test]
    fn parse_duration() {
        // 0 seconds
        assert_eq!(parse_iso8601_duration("PT0S"), Some(std::time::Duration::new(0, 0)));

        // 9 seconds, 26.9026 milliseconds
        assert_eq!(
            parse_iso8601_duration("PT9.0269026S"),
            Some(std::time::Duration::new(9, 26_902_600))
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
