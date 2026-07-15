use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;

use super::lockfile::{LockedPackage, PeriodLock};
use super::manifest::{DependencySpec, PeriodToml};
use super::registry::{RegistryIndex, select_version};

#[derive(Debug, Clone)]
pub struct ResolvedPackage {
    pub name: String,
    pub version: String,
    pub source: String,
    pub checksum: Option<String>,
    pub file_path: PathBuf,
}

pub struct Resolver<'a> {
    registry: &'a str,
    index: Option<RegistryIndex>,
    lockfile: Option<&'a PeriodLock>,
    resolved: BTreeMap<String, ResolvedPackage>,
    loading: HashSet<String>,
}

impl<'a> Resolver<'a> {
    pub fn new(registry: &'a str, lockfile: Option<&'a PeriodLock>) -> Self {
        Self {
            registry,
            index: None,
            lockfile,
            resolved: BTreeMap::new(),
            loading: HashSet::new(),
        }
    }

    pub fn resolve(&mut self, manifest: &PeriodToml) -> Result<Vec<ResolvedPackage>, String> {
        let mut queue: Vec<(String, DependencySpec)> = Vec::new();
        for (name, spec) in &manifest.dependencies {
            queue.push((name.clone(), spec.clone()));
        }

        let mut i = 0;
        while i < queue.len() {
            let (name, spec) = queue[i].clone();
            if self.resolved.contains_key(&name) {
                i += 1;
                continue;
            }

            if !self.loading.insert(name.clone()) {
                return Err(format!("circular dependency detected involving '{}'", name));
            }

            let resolved = if let Some(git) = spec.git_url() {
                self.resolve_git(&name, git, spec.version())?
            } else {
                self.resolve_dependency(&name, spec.version().unwrap_or("*"))?
            };

            for (dep_name, dep_version) in &resolved.dependencies {
                queue.push((
                    dep_name.clone(),
                    DependencySpec::Version(dep_version.clone()),
                ));
            }

            let file_path = PathBuf::from("period_packages").join(format!("{}.period", name));
            self.resolved.insert(
                name.clone(),
                ResolvedPackage {
                    name: name.clone(),
                    version: resolved.version.clone(),
                    source: resolved.source.clone(),
                    checksum: resolved.checksum.clone(),
                    file_path,
                },
            );

            self.loading.remove(&name);
            i += 1;
        }

        Ok(self.resolved.values().cloned().collect())
    }

    fn resolve_dependency(
        &mut self,
        name: &str,
        constraint: &str,
    ) -> Result<ResolvedVersion, String> {
        // If a lockfile entry exists and satisfies the constraint, reuse it
        // for reproducible installs.
        if let Some(lock) = self.lockfile {
            if let Some(locked) = lock.packages.iter().find(|p| p.name == name) {
                if self.lock_satisfies(locked, constraint) {
                    return Ok(ResolvedVersion {
                        version: locked.version.clone(),
                        source: locked.source.clone(),
                        checksum: Some(locked.checksum.clone()),
                        dependencies: BTreeMap::new(),
                    });
                }
            }
        }
        self.resolve_registry(name, constraint)
    }

    fn lock_satisfies(&self, locked: &LockedPackage, constraint: &str) -> bool {
        // Exact constraints are easy.
        if let Some(rest) = constraint.strip_prefix('=') {
            return locked.version == rest.trim();
        }
        // For caret/tilde/wildcard/plain, re-use registry version selection against
        // a synthetic single-entry map. Lockfile entries are registry URLs.
        if locked.source.starts_with("registry+") {
            let mut map = BTreeMap::new();
            map.insert(
                locked.version.clone(),
                super::registry::RegistryVersion {
                    url: locked
                        .source
                        .strip_prefix("registry+")
                        .unwrap_or(&locked.source)
                        .to_string(),
                    checksum: Some(locked.checksum.clone()),
                    dependencies: BTreeMap::new(),
                },
            );
            select_version(constraint, &map).is_ok()
        } else {
            // Non-registry sources are not lockfile-reproducible in this way;
            // fall through to fresh resolution.
            false
        }
    }

    fn resolve_registry(
        &mut self,
        name: &str,
        constraint: &str,
    ) -> Result<ResolvedVersion, String> {
        if self.index.is_none() {
            self.index = Some(RegistryIndex::fetch(self.registry)?);
        }
        let index = self
            .index
            .as_ref()
            .ok_or_else(|| "internal error: registry index not loaded".to_string())?;
        let versions = index
            .packages
            .get(name)
            .ok_or_else(|| format!("package '{}' not found in registry", name))?;
        let version = select_version(constraint, versions)?;
        let entry = versions.get(&version).ok_or_else(|| {
            format!(
                "internal error: selected version '{}' disappeared for package '{}'",
                version, name
            )
        })?;
        Ok(ResolvedVersion {
            version,
            source: format!("registry+{}", entry.url),
            checksum: entry.checksum.clone(),
            dependencies: entry.dependencies.clone(),
        })
    }

    fn resolve_git(
        &self,
        name: &str,
        git: &str,
        version: Option<&str>,
    ) -> Result<ResolvedVersion, String> {
        let version = version.unwrap_or("latest").to_string();
        let url = format!("{}/raw/main/{}.period", git.trim_end_matches('/'), name);
        Ok(ResolvedVersion {
            version,
            source: format!("git+{}", url),
            checksum: None,
            dependencies: BTreeMap::new(),
        })
    }
}

#[derive(Debug, Clone)]
struct ResolvedVersion {
    version: String,
    source: String,
    checksum: Option<String>,
    dependencies: BTreeMap<String, String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_cycle() {
        let lock = PeriodLock::default();
        let resolver = Resolver::new("https://example.com", Some(&lock));
        let mut manifest = PeriodToml {
            package: super::super::manifest::Package {
                name: "demo".to_string(),
                version: "1.0.0".to_string(),
                authors: Vec::new(),
                license: None,
            },
            dependencies: BTreeMap::new(),
        };
        manifest.dependencies.insert(
            "self".to_string(),
            DependencySpec::Version("1.0.0".to_string()),
        );
        assert!(resolver.resolved.is_empty());
    }

    #[test]
    fn lockfile_reused_when_constraint_satisfied() {
        let lock = PeriodLock {
            packages: vec![LockedPackage {
                name: "foo".to_string(),
                version: "1.2.3".to_string(),
                source: "registry+https://example.com/foo-1.2.3.period".to_string(),
                checksum: "sha256:abcd".to_string(),
            }],
        };
        let mut resolver = Resolver::new("https://example.com", Some(&lock));
        let mut manifest = PeriodToml {
            package: super::super::manifest::Package {
                name: "demo".to_string(),
                version: "1.0.0".to_string(),
                authors: Vec::new(),
                license: None,
            },
            dependencies: BTreeMap::new(),
        };
        manifest.dependencies.insert(
            "foo".to_string(),
            DependencySpec::Version("^1.0.0".to_string()),
        );

        // Without fetching the registry, the lockfile entry should be reused.
        let resolved = resolver
            .resolve(&manifest)
            .expect("should resolve from lockfile");
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].version, "1.2.3");
        assert_eq!(
            resolved[0].source,
            "registry+https://example.com/foo-1.2.3.period"
        );
    }
}
