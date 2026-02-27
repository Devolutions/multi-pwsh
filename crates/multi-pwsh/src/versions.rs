use std::fmt;

use semver::Version;

use crate::error::{MultiPwshError, Result};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct MajorMinor {
    pub major: u64,
    pub minor: u64,
}

impl MajorMinor {
    pub fn from_version(version: &Version) -> Self {
        MajorMinor {
            major: version.major,
            minor: version.minor,
        }
    }
}

impl fmt::Display for MajorMinor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}", self.major, self.minor)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum VersionSelector {
    Major(u64),
    Exact(Version),
    MajorMinor(MajorMinor),
}

pub fn parse_install_selector(value: &str) -> Result<VersionSelector> {
    if let Ok(major) = parse_major_selector(value) {
        return Ok(VersionSelector::Major(major));
    }

    if let Ok(line) = parse_major_minor_selector(value) {
        return Ok(VersionSelector::MajorMinor(line));
    }

    let exact = parse_exact_version(value)?;
    Ok(VersionSelector::Exact(exact))
}

pub fn parse_major_selector(value: &str) -> Result<u64> {
    let trimmed = value.trim().trim_start_matches('v');
    if trimmed.is_empty() || trimmed.contains('.') {
        return Err(MultiPwshError::InvalidArguments(format!(
            "'{}' is not a major selector",
            value
        )));
    }

    trimmed.parse::<u64>().map_err(|_| {
        MultiPwshError::InvalidArguments(format!("invalid major version '{}' in selector '{}'", trimmed, value))
    })
}

pub fn parse_major_minor_selector(value: &str) -> Result<MajorMinor> {
    let trimmed = value.trim().trim_start_matches('v');
    let parts: Vec<&str> = trimmed.split('.').collect();
    if parts.len() != 2 {
        return Err(MultiPwshError::InvalidArguments(format!(
            "'{}' is not a major.minor selector",
            value
        )));
    }

    let major = parts[0].parse::<u64>().map_err(|_| {
        MultiPwshError::InvalidArguments(format!("invalid major version '{}' in selector '{}'", parts[0], value))
    })?;

    let minor = parts[1].parse::<u64>().map_err(|_| {
        MultiPwshError::InvalidArguments(format!("invalid minor version '{}' in selector '{}'", parts[1], value))
    })?;

    Ok(MajorMinor { major, minor })
}

pub fn parse_exact_version(value: &str) -> Result<Version> {
    let trimmed = value.trim().trim_start_matches('v');
    if trimmed.is_empty() {
        return Err(MultiPwshError::InvalidArguments(format!(
            "'{}' is not an exact major.minor.patch version",
            value
        )));
    }

    if let Ok(version) = Version::parse(trimmed) {
        return Ok(version);
    }

    if let Some(normalized) = normalize_prerelease_shorthand(trimmed) {
        if let Ok(version) = Version::parse(&normalized) {
            return Ok(version);
        }
    }

    Err(MultiPwshError::InvalidArguments(format!(
        "'{}' is not an exact major.minor.patch version",
        value
    )))
}

fn normalize_prerelease_shorthand(value: &str) -> Option<String> {
    if let Some((major_minor, suffix)) = value.split_once('-') {
        return normalize_prerelease_parts(major_minor, suffix);
    }

    let dot_index = value.find('.')?;
    let major_text = &value[..dot_index];
    let rest = &value[dot_index + 1..];

    let minor_digit_count = rest.chars().take_while(|character| character.is_ascii_digit()).count();
    if minor_digit_count == 0 || minor_digit_count == rest.len() {
        return None;
    }

    let minor_text = &rest[..minor_digit_count];
    let suffix = &rest[minor_digit_count..];
    let major_minor = format!("{}.{}", major_text, minor_text);

    normalize_prerelease_parts(&major_minor, suffix)
}

fn normalize_prerelease_parts(major_minor: &str, suffix: &str) -> Option<String> {
    let parts: Vec<&str> = major_minor.split('.').collect();
    if parts.len() != 2 {
        return None;
    }

    let major = parts[0].parse::<u64>().ok()?;
    let minor = parts[1].parse::<u64>().ok()?;

    let lowercase = suffix.to_ascii_lowercase();
    let (label, number_text) = if let Some(rest) = lowercase.strip_prefix("preview") {
        ("preview", rest)
    } else if let Some(rest) = lowercase.strip_prefix("rc") {
        ("rc", rest)
    } else {
        return None;
    };

    let number_text = number_text.strip_prefix('.').unwrap_or(number_text);
    if number_text.is_empty() || !number_text.chars().all(|character| character.is_ascii_digit()) {
        return None;
    }

    Some(format!("{}.{}.0-{}.{}", major, minor, label, number_text))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_major_minor_selector() {
        let selector = parse_major_minor_selector("7.4").unwrap();
        assert_eq!(selector.major, 7);
        assert_eq!(selector.minor, 4);
    }

    #[test]
    fn parses_major_selector() {
        let selector = parse_install_selector("7").unwrap();
        match selector {
            VersionSelector::Major(major) => {
                assert_eq!(major, 7);
            }
            _ => panic!("expected major selector"),
        }
    }

    #[test]
    fn parses_exact_selector() {
        let selector = parse_install_selector("7.4.13").unwrap();
        match selector {
            VersionSelector::Exact(version) => {
                assert_eq!(version.major, 7);
                assert_eq!(version.minor, 4);
                assert_eq!(version.patch, 13);
            }
            _ => panic!("expected exact selector"),
        }
    }

    #[test]
    fn parses_exact_selector_with_prerelease() {
        let selector = parse_install_selector("7.6.0-rc.1").unwrap();
        match selector {
            VersionSelector::Exact(version) => {
                assert_eq!(version.major, 7);
                assert_eq!(version.minor, 6);
                assert_eq!(version.patch, 0);
                assert_eq!(version.pre.as_str(), "rc.1");
            }
            _ => panic!("expected exact selector"),
        }
    }

    #[test]
    fn parses_exact_selector_with_prerelease_shorthand() {
        let selector = parse_install_selector("7.6-preview6").unwrap();
        match selector {
            VersionSelector::Exact(version) => {
                assert_eq!(version.major, 7);
                assert_eq!(version.minor, 6);
                assert_eq!(version.patch, 0);
                assert_eq!(version.pre.as_str(), "preview.6");
            }
            _ => panic!("expected exact selector"),
        }
    }

    #[test]
    fn parses_exact_selector_with_rc_shorthand() {
        let selector = parse_install_selector("7.6-rc1").unwrap();
        match selector {
            VersionSelector::Exact(version) => {
                assert_eq!(version.major, 7);
                assert_eq!(version.minor, 6);
                assert_eq!(version.patch, 0);
                assert_eq!(version.pre.as_str(), "rc.1");
            }
            _ => panic!("expected exact selector"),
        }
    }

    #[test]
    fn parses_exact_selector_with_compact_rc_shorthand() {
        let selector = parse_install_selector("7.6rc1").unwrap();
        match selector {
            VersionSelector::Exact(version) => {
                assert_eq!(version.major, 7);
                assert_eq!(version.minor, 6);
                assert_eq!(version.patch, 0);
                assert_eq!(version.pre.as_str(), "rc.1");
            }
            _ => panic!("expected exact selector"),
        }
    }

    #[test]
    fn parses_line_selector() {
        let selector = parse_install_selector("7.5").unwrap();
        match selector {
            VersionSelector::MajorMinor(line) => {
                assert_eq!(line.major, 7);
                assert_eq!(line.minor, 5);
            }
            _ => panic!("expected major.minor selector"),
        }
    }
}
