use semver::Version;
use serde::Deserialize;
use ureq::Agent;

use crate::error::{MultiPwshError, Result};
use crate::platform::{HostArch, HostOs};
use crate::versions::{MajorMinor, VersionSelector};

#[derive(Clone, Debug)]
pub struct ResolvedRelease {
    pub version: Version,
    pub asset_name: String,
    pub asset_url: String,
}

impl ResolvedRelease {
    pub fn version_line(&self) -> MajorMinor {
        MajorMinor::from_version(&self.version)
    }
}

#[derive(Clone)]
pub struct ReleaseClient {
    http: Agent,
    authorization_header: Option<String>,
}

impl ReleaseClient {
    pub fn new(github_token: Option<String>) -> Result<Self> {
        let authorization_header = github_token
            .filter(|token| !token.trim().is_empty())
            .map(|token| format!("Bearer {}", token));

        let http = ureq::AgentBuilder::new().build();

        Ok(ReleaseClient {
            http,
            authorization_header,
        })
    }

    pub fn http_client(&self) -> &Agent {
        &self.http
    }

    pub fn resolve_selector(
        &self,
        selector: VersionSelector,
        os: HostOs,
        arch: HostArch,
        include_prerelease: bool,
    ) -> Result<ResolvedRelease> {
        match selector {
            VersionSelector::Major(major) => self.resolve_latest_in_major(major, os, arch, include_prerelease),
            VersionSelector::Exact(version) => self.resolve_exact(version, os, arch),
            VersionSelector::MajorMinor(line) => self.resolve_latest_in_line(line, os, arch, include_prerelease),
            VersionSelector::MajorMinorWildcard(line) => {
                self.resolve_latest_in_line(line, os, arch, include_prerelease)
            }
        }
    }

    pub fn resolve_all_in_line(
        &self,
        line: MajorMinor,
        os: HostOs,
        arch: HostArch,
        include_prerelease: bool,
    ) -> Result<Vec<ResolvedRelease>> {
        let releases = self.fetch_releases()?;
        let mut candidates: Vec<ParsedRelease> = releases
            .into_iter()
            .filter(|release| include_prerelease || !release.prerelease)
            .filter_map(ParsedRelease::from_github_release)
            .filter(|release| release.version.major == line.major && release.version.minor == line.minor)
            .collect();

        candidates.sort_by(|a, b| b.version.cmp(&a.version));

        let mut resolved = Vec::new();
        for candidate in candidates {
            if let Ok(release) = resolve_release_asset(candidate, os, arch) {
                resolved.push(release);
            }
        }

        if resolved.is_empty() {
            return Err(MultiPwshError::ReleaseNotFound(format!(
                "no release found for line {}",
                line
            )));
        }

        Ok(resolved)
    }

    pub fn resolve_latest_in_major(
        &self,
        major: u64,
        os: HostOs,
        arch: HostArch,
        include_prerelease: bool,
    ) -> Result<ResolvedRelease> {
        let releases = self.fetch_releases()?;
        let mut candidates: Vec<ParsedRelease> = releases
            .into_iter()
            .filter(|release| include_prerelease || !release.prerelease)
            .filter_map(ParsedRelease::from_github_release)
            .filter(|release| release.version.major == major)
            .collect();

        candidates.sort_by(|a, b| b.version.cmp(&a.version));
        let release = candidates
            .into_iter()
            .next()
            .ok_or_else(|| MultiPwshError::ReleaseNotFound(format!("no release found for major {}", major)))?;

        resolve_release_asset(release, os, arch)
    }

    pub fn resolve_latest_in_line(
        &self,
        line: MajorMinor,
        os: HostOs,
        arch: HostArch,
        include_prerelease: bool,
    ) -> Result<ResolvedRelease> {
        let releases = self.fetch_releases()?;
        let mut candidates: Vec<ParsedRelease> = releases
            .into_iter()
            .filter(|release| include_prerelease || !release.prerelease)
            .filter_map(ParsedRelease::from_github_release)
            .filter(|release| release.version.major == line.major && release.version.minor == line.minor)
            .collect();

        candidates.sort_by(|a, b| b.version.cmp(&a.version));
        let release = candidates
            .into_iter()
            .next()
            .ok_or_else(|| MultiPwshError::ReleaseNotFound(format!("no release found for line {}", line)))?;

        resolve_release_asset(release, os, arch)
    }

    pub fn list_available_versions(&self, include_prerelease: bool) -> Result<Vec<Version>> {
        let releases = self.fetch_releases()?;
        let mut versions: Vec<Version> = releases
            .into_iter()
            .filter(|release| include_prerelease || !release.prerelease)
            .filter_map(ParsedRelease::from_github_release)
            .map(|release| release.version)
            .collect();

        versions.sort_by(|a, b| b.cmp(a));
        versions.dedup();
        Ok(versions)
    }

    fn resolve_exact(&self, version: Version, os: HostOs, arch: HostArch) -> Result<ResolvedRelease> {
        let releases = self.fetch_releases()?;
        let release = releases
            .into_iter()
            .filter_map(ParsedRelease::from_github_release)
            .find(|release| release.version == version)
            .ok_or_else(|| MultiPwshError::ReleaseNotFound(format!("version {}", version)))?;

        resolve_release_asset(release, os, arch)
    }

    fn fetch_releases(&self) -> Result<Vec<GithubRelease>> {
        let mut all_releases = Vec::new();

        for page in 1..=10 {
            let url = format!(
                "https://api.github.com/repos/PowerShell/PowerShell/releases?per_page=100&page={}",
                page
            );

            let mut request = self
                .http
                .get(&url)
                .set("Accept", "application/vnd.github.v3+json")
                .set("User-Agent", "multi-pwsh");

            if let Some(value) = self.authorization_header.as_deref() {
                request = request.set("Authorization", value);
            }

            let response = request.call()?;
            let body = response.into_string()?;
            let mut page_releases: Vec<GithubRelease> = serde_json::from_str(&body)?;

            if page_releases.is_empty() {
                break;
            }

            let is_last_page = page_releases.len() < 100;
            all_releases.append(&mut page_releases);

            if is_last_page {
                break;
            }
        }

        Ok(all_releases)
    }
}

fn resolve_release_asset(release: ParsedRelease, os: HostOs, arch: HostArch) -> Result<ResolvedRelease> {
    let pattern = asset_pattern(os, arch)?;
    let tag_name = release.tag_name.clone();
    let asset = release
        .assets
        .into_iter()
        .find(|asset| wildcard_match(pattern, &asset.name))
        .ok_or_else(|| {
            MultiPwshError::AssetNotFound(format!("no asset found for pattern '{}' in {}", pattern, tag_name))
        })?;

    Ok(ResolvedRelease {
        version: release.version,
        asset_name: asset.name,
        asset_url: asset.browser_download_url,
    })
}

fn asset_pattern(os: HostOs, arch: HostArch) -> Result<&'static str> {
    match os {
        HostOs::Windows => match arch {
            HostArch::X64 => Ok("PowerShell-*-win-x64.zip"),
            HostArch::X86 => Ok("PowerShell-*-win-x86.zip"),
            HostArch::Arm64 => Ok("PowerShell-*-win-arm64.zip"),
            HostArch::Arm32 => Err(MultiPwshError::UnsupportedPlatform(
                "arm32 is not supported on windows".to_string(),
            )),
        },
        HostOs::Macos => match arch {
            HostArch::X64 => Ok("powershell-*-osx-x64.tar.gz"),
            HostArch::Arm64 => Ok("powershell-*-osx-arm64.tar.gz"),
            HostArch::X86 | HostArch::Arm32 => Err(MultiPwshError::UnsupportedPlatform(
                "architecture is not supported on macos".to_string(),
            )),
        },
        HostOs::Linux => match arch {
            HostArch::X64 => Ok("powershell-*-linux-x64.tar.gz"),
            HostArch::Arm64 => Ok("powershell-*-linux-arm64.tar.gz"),
            HostArch::Arm32 => Ok("powershell-*-linux-arm32.tar.gz"),
            HostArch::X86 => Err(MultiPwshError::UnsupportedPlatform(
                "x86 is not supported on linux".to_string(),
            )),
        },
    }
}

fn wildcard_match(pattern: &str, text: &str) -> bool {
    if pattern == "*" {
        return true;
    }

    let starts_with_wildcard = pattern.starts_with('*');
    let ends_with_wildcard = pattern.ends_with('*');
    let parts: Vec<&str> = pattern.split('*').filter(|part| !part.is_empty()).collect();

    if parts.is_empty() {
        return true;
    }

    let mut cursor = 0usize;
    for (index, part) in parts.iter().enumerate() {
        if index == 0 && !starts_with_wildcard {
            if !text[cursor..].starts_with(part) {
                return false;
            }
            cursor += part.len();
            continue;
        }

        if index == parts.len() - 1 && !ends_with_wildcard {
            if let Some(found) = text[cursor..].rfind(part) {
                let absolute = cursor + found;
                if absolute + part.len() != text.len() {
                    return false;
                }
                cursor = absolute + part.len();
            } else {
                return false;
            }
            continue;
        }

        if let Some(found) = text[cursor..].find(part) {
            cursor += found + part.len();
        } else {
            return false;
        }
    }

    true
}

#[derive(Debug, Deserialize)]
struct GithubRelease {
    tag_name: String,
    prerelease: bool,
    assets: Vec<GithubAsset>,
}

#[derive(Debug, Deserialize)]
struct GithubAsset {
    name: String,
    browser_download_url: String,
}

#[derive(Debug)]
struct ParsedRelease {
    tag_name: String,
    version: Version,
    assets: Vec<GithubAsset>,
}

impl ParsedRelease {
    fn from_github_release(release: GithubRelease) -> Option<Self> {
        let version_text = release.tag_name.trim_start_matches('v');
        let version = Version::parse(version_text).ok()?;

        Some(ParsedRelease {
            tag_name: release.tag_name,
            version,
            assets: release.assets,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wildcard_match_supports_single_star_segments() {
        assert!(wildcard_match(
            "PowerShell-*-win-x64.zip",
            "PowerShell-7.4.13-win-x64.zip"
        ));
        assert!(wildcard_match(
            "powershell-*-linux-arm64.tar.gz",
            "powershell-7.5.1-linux-arm64.tar.gz"
        ));
        assert!(!wildcard_match(
            "powershell-*-linux-arm64.tar.gz",
            "powershell-7.5.1-linux-x64.tar.gz"
        ));
    }
}
