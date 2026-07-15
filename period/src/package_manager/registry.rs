use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Default registry URL.
pub fn default_registry() -> String {
    std::env::var("PERIOD_REGISTRY")
        .unwrap_or_else(|_| "https://period-lang.github.io/registry".to_string())
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct RegistryIndex {
    pub schema_version: String,
    #[serde(default)]
    pub packages: BTreeMap<String, BTreeMap<String, RegistryVersion>>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct RegistryVersion {
    pub url: String,
    #[serde(default)]
    pub checksum: Option<String>,
    #[serde(default)]
    pub dependencies: BTreeMap<String, String>,
}

impl RegistryIndex {
    pub fn fetch(registry: &str) -> Result<Self, String> {
        let url = format!("{}/registry.json", registry.trim_end_matches('/'));
        let text = ureq::get(&url)
            .call()
            .map_err(|e| format!("failed to fetch registry from '{}': {}", url, e))?
            .into_string()
            .map_err(|e| format!("failed to read registry from '{}': {}", url, e))?;
        serde_json::from_str(&text).map_err(|e| format!("invalid registry at '{}': {}", url, e))
    }
}

/// Select the latest version from `available` that satisfies `constraint`.
///
/// Supported constraints:
/// - `*` or `x` / `X` — any version.
/// - `=1.2.3` — exact version.
/// - `^1.2.3` — compatible with 1.2.3 (SemVer caret).
/// - `~1.2.3` — approximately equivalent to 1.2.3 (SemVer tilde).
/// - `1.2.3`, `1.2`, `1` — plain versions are treated as caret constraints
///   (`1.2.3` means `^1.2.3`), matching the usual dependency-management
///   convention.
pub fn select_version(
    constraint: &str,
    available: &BTreeMap<String, RegistryVersion>,
) -> Result<String, String> {
    let req = VersionReq::parse(constraint)?;
    let mut best: Option<Version> = None;
    let mut best_string = String::new();
    for version_str in available.keys() {
        let version = Version::parse(version_str)
            .map_err(|e| format!("registry contains invalid version '{}': {}", version_str, e))?;
        if !req.matches(&version) {
            continue;
        }
        if best.as_ref().is_none_or(|b| version.is_greater_than(b)) {
            best = Some(version);
            best_string = version_str.clone();
        }
    }
    if best_string.is_empty() {
        return Err(format!("no version matching '{}' found", constraint));
    }
    Ok(best_string)
}

/// A parsed SemVer-like version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Version {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
    pub prerelease: Option<String>,
}

impl Version {
    pub fn parse(s: &str) -> Result<Self, String> {
        let s = s.trim();
        let (core, prerelease) = s
            .split_once('-')
            .map(|(c, p)| (c, Some(p)))
            .unwrap_or((s, None));
        let parts: Vec<&str> = core.split('.').collect();
        if parts.is_empty() || parts.len() > 3 {
            return Err(format!("invalid version '{}'", s));
        }
        let major = parse_number(parts[0], s)?;
        let minor = parts
            .get(1)
            .map(|p| parse_number(p, s))
            .transpose()?
            .unwrap_or(0);
        let patch = parts
            .get(2)
            .map(|p| parse_number(p, s))
            .transpose()?
            .unwrap_or(0);
        let prerelease = prerelease.map(|p| p.to_string());
        Ok(Version {
            major,
            minor,
            patch,
            prerelease,
        })
    }

    pub fn as_triple(&self) -> (u64, u64, u64) {
        (self.major, self.minor, self.patch)
    }

    pub fn is_greater_than(&self, other: &Version) -> bool {
        match self.as_triple().cmp(&other.as_triple()) {
            std::cmp::Ordering::Greater => true,
            std::cmp::Ordering::Less => false,
            std::cmp::Ordering::Equal => {
                compare_prerelease(&self.prerelease, &other.prerelease)
                    == std::cmp::Ordering::Greater
            }
        }
    }
}

fn parse_number(s: &str, original: &str) -> Result<u64, String> {
    s.parse::<u64>()
        .map_err(|_| format!("invalid version segment '{}' in '{}'", s, original))
}

fn compare_prerelease(a: &Option<String>, b: &Option<String>) -> std::cmp::Ordering {
    match (a, b) {
        (None, None) => std::cmp::Ordering::Equal,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (Some(_), None) => std::cmp::Ordering::Less,
        (Some(a), Some(b)) => {
            let a_parts: Vec<&str> = a.split('.').collect();
            let b_parts: Vec<&str> = b.split('.').collect();
            let len = a_parts.len().max(b_parts.len());
            for i in 0..len {
                let av = a_parts.get(i);
                let bv = b_parts.get(i);
                match (av, bv) {
                    (None, None) => return std::cmp::Ordering::Equal,
                    (None, Some(_)) => return std::cmp::Ordering::Less,
                    (Some(_), None) => return std::cmp::Ordering::Greater,
                    (Some(a), Some(b)) => {
                        // Numeric identifiers compare as numbers; otherwise lexical.
                        match (a.parse::<u64>(), b.parse::<u64>()) {
                            (Ok(an), Ok(bn)) => match an.cmp(&bn) {
                                std::cmp::Ordering::Equal => continue,
                                other => return other,
                            },
                            (Ok(_), Err(_)) => return std::cmp::Ordering::Less,
                            (Err(_), Ok(_)) => return std::cmp::Ordering::Greater,
                            (Err(_), Err(_)) => match a.cmp(b) {
                                std::cmp::Ordering::Equal => continue,
                                other => return other,
                            },
                        }
                    }
                }
            }
            std::cmp::Ordering::Equal
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VersionReq {
    Wildcard,
    Exact(Version),
    Caret(Version),
    Tilde(Version),
}

impl VersionReq {
    pub fn parse(s: &str) -> Result<Self, String> {
        let s = s.trim();
        if s == "*" || s.eq_ignore_ascii_case("x") {
            return Ok(VersionReq::Wildcard);
        }
        if let Some(rest) = s.strip_prefix('=') {
            return Ok(VersionReq::Exact(Version::parse(rest)?));
        }
        if let Some(rest) = s.strip_prefix('^') {
            return Ok(VersionReq::Caret(Version::parse(rest)?));
        }
        if let Some(rest) = s.strip_prefix('~') {
            return Ok(VersionReq::Tilde(Version::parse(rest)?));
        }
        // Plain version strings are treated as caret constraints.
        Ok(VersionReq::Caret(Version::parse(s)?))
    }

    pub fn matches(&self, version: &Version) -> bool {
        match self {
            VersionReq::Wildcard => true,
            VersionReq::Exact(req) => {
                req.as_triple() == version.as_triple() && req.prerelease == version.prerelease
            }
            VersionReq::Caret(req) => caret_matches(req, version),
            VersionReq::Tilde(req) => tilde_matches(req, version),
        }
    }
}

fn caret_matches(req: &Version, version: &Version) -> bool {
    // >= req, with upper bound depending on the most significant non-zero component.
    if !version.is_greater_than(req) && version.as_triple() != req.as_triple() {
        return false;
    }
    let upper = if req.major > 0 {
        (req.major + 1, 0, 0)
    } else if req.minor > 0 {
        (0, req.minor + 1, 0)
    } else {
        (0, 0, req.patch + 1)
    };
    version.as_triple() < upper
}

fn tilde_matches(req: &Version, version: &Version) -> bool {
    // >= req, < major.minor+1.0
    if !version.is_greater_than(req) && version.as_triple() != req.as_triple() {
        return false;
    }
    version.as_triple() < (req.major, req.minor + 1, 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn versions() -> BTreeMap<String, RegistryVersion> {
        let mut versions = BTreeMap::new();
        versions.insert(
            "1.0.0".to_string(),
            RegistryVersion {
                url: "a".to_string(),
                checksum: None,
                dependencies: BTreeMap::new(),
            },
        );
        versions.insert(
            "1.2.0".to_string(),
            RegistryVersion {
                url: "b".to_string(),
                checksum: None,
                dependencies: BTreeMap::new(),
            },
        );
        versions.insert(
            "1.2.3".to_string(),
            RegistryVersion {
                url: "c".to_string(),
                checksum: None,
                dependencies: BTreeMap::new(),
            },
        );
        versions.insert(
            "2.0.0".to_string(),
            RegistryVersion {
                url: "d".to_string(),
                checksum: None,
                dependencies: BTreeMap::new(),
            },
        );
        versions.insert(
            "2.1.0".to_string(),
            RegistryVersion {
                url: "e".to_string(),
                checksum: None,
                dependencies: BTreeMap::new(),
            },
        );
        versions.insert(
            "0.5.1".to_string(),
            RegistryVersion {
                url: "f".to_string(),
                checksum: None,
                dependencies: BTreeMap::new(),
            },
        );
        versions
    }

    #[test]
    fn select_wildcard() {
        assert_eq!(select_version("*", &versions()).unwrap(), "2.1.0");
        assert_eq!(select_version("x", &versions()).unwrap(), "2.1.0");
    }

    #[test]
    fn select_exact() {
        assert_eq!(select_version("=1.2.0", &versions()).unwrap(), "1.2.0");
        assert_eq!(select_version("=0.5.1", &versions()).unwrap(), "0.5.1");
    }

    #[test]
    fn select_caret() {
        // ^1.0.0 -> >=1.0.0, <2.0.0
        assert_eq!(select_version("^1.0.0", &versions()).unwrap(), "1.2.3");
        // ^1.2.0 -> >=1.2.0, <2.0.0
        assert_eq!(select_version("^1.2.0", &versions()).unwrap(), "1.2.3");
        // ^1.2.3 -> >=1.2.3, <2.0.0
        assert_eq!(select_version("^1.2.3", &versions()).unwrap(), "1.2.3");
        // ^0.5.1 -> >=0.5.1, <0.6.0
        assert_eq!(select_version("^0.5.1", &versions()).unwrap(), "0.5.1");
        // Plain version is caret.
        assert_eq!(select_version("1.0.0", &versions()).unwrap(), "1.2.3");
    }

    #[test]
    fn select_tilde() {
        // ~1.2.0 -> >=1.2.0, <1.3.0
        assert_eq!(select_version("~1.2.0", &versions()).unwrap(), "1.2.3");
        // ~1.0.0 -> >=1.0.0, <1.1.0
        assert_eq!(select_version("~1.0.0", &versions()).unwrap(), "1.0.0");
    }

    #[test]
    fn select_caret_across_major_boundary() {
        assert_eq!(select_version("^2.0.0", &versions()).unwrap(), "2.1.0");
    }

    #[test]
    fn select_no_match() {
        assert!(select_version("=3.0.0", &versions()).is_err());
        assert!(select_version("^0.6.0", &versions()).is_err());
        assert!(select_version("~1.3.0", &versions()).is_err());
    }

    #[test]
    fn select_wildcard_with_single_version() {
        let mut versions = BTreeMap::new();
        versions.insert(
            "0.5.1".to_string(),
            RegistryVersion {
                url: "a".to_string(),
                checksum: None,
                dependencies: BTreeMap::new(),
            },
        );
        assert_eq!(select_version("*", &versions).unwrap(), "0.5.1");
    }

    #[test]
    fn deserialize_registry_index() {
        let json = r#"{
            "schema_version": "1",
            "packages": {
                "list": {
                    "1.0.0": {
                        "url": "https://github.com/period-lang/registry/releases/download/list-1.0.0/list-1.0.0.period",
                        "checksum": "sha256:abcd",
                        "dependencies": {}
                    }
                }
            }
        }"#;
        let index: RegistryIndex =
            serde_json::from_str(json).expect("registry JSON should deserialize");
        assert_eq!(index.schema_version, "1");
        let list = index.packages.get("list").expect("list package");
        let version = list.get("1.0.0").expect("1.0.0 version");
        assert_eq!(
            version.url,
            "https://github.com/period-lang/registry/releases/download/list-1.0.0/list-1.0.0.period"
        );
        assert_eq!(version.checksum.as_deref(), Some("sha256:abcd"));
    }

    #[test]
    fn version_parsing_and_comparison() {
        assert!(
            Version::parse("1.2.3")
                .unwrap()
                .is_greater_than(&Version::parse("1.2.2").unwrap())
        );
        assert!(
            !Version::parse("1.2.3")
                .unwrap()
                .is_greater_than(&Version::parse("1.2.3").unwrap())
        );
        assert!(
            Version::parse("2.0.0")
                .unwrap()
                .is_greater_than(&Version::parse("1.9.9").unwrap())
        );
        assert!(
            Version::parse("1.0.0")
                .unwrap()
                .is_greater_than(&Version::parse("1.0.0-alpha").unwrap())
        );
        assert!(
            Version::parse("1.0.0-alpha.2")
                .unwrap()
                .is_greater_than(&Version::parse("1.0.0-alpha.1").unwrap())
        );
    }
}
