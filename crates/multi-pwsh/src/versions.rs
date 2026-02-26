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
    Exact(Version),
    MajorMinor(MajorMinor),
}

pub fn parse_install_selector(value: &str) -> Result<VersionSelector> {
    if let Ok(line) = parse_major_minor_selector(value) {
        return Ok(VersionSelector::MajorMinor(line));
    }

    let exact = parse_exact_version(value)?;
    Ok(VersionSelector::Exact(exact))
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
    if trimmed.matches('.').count() != 2 {
        return Err(MultiPwshError::InvalidArguments(format!(
            "'{}' is not an exact major.minor.patch version",
            value
        )));
    }

    Ok(Version::parse(trimmed)?)
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
