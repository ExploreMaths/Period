use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::package_manager::downloader::sha256_hex;
use crate::package_manager::manifest::PeriodToml;
use crate::package_manager::registry::{RegistryIndex, RegistryVersion};

/// Options for publishing a package.
pub struct PublishOptions<'a> {
    pub file: &'a Path,
    pub name: Option<&'a str>,
    pub version: Option<&'a str>,
    pub registry_file: Option<&'a Path>,
    pub base_url: Option<&'a str>,
    pub upload: bool,
    pub repo: Option<&'a str>,
}

/// Publish a `.period` file and produce a registry entry.
///
/// If `version` is `None`, tries to read it from `period.toml` in the current
/// directory, falling back to `"1.0.0"`.
///
/// The file is read, its SHA256 checksum is computed, and a registry entry is
/// built. If `registry_file` is provided, the entry is merged into that file
/// (creating it if necessary). Otherwise the entry JSON is printed to stdout.
///
/// When `upload` is `true`, the package file is uploaded to a GitHub Release
/// using the `gh` CLI. The `--repo` option must be `owner/repo`; if omitted,
/// the current repository is detected from `git remote`.
pub fn publish(options: PublishOptions<'_>) -> Result<(), String> {
    let package_name = determine_package_name(options.file, options.name)?;
    let package_version = determine_version(options.version)?;
    let tag = format!("{}-{}", package_name, package_version);
    let asset_name = format!("{}-{}.period", package_name, package_version);

    let bytes = fs::read(options.file)
        .map_err(|e| format!("cannot read '{}': {}", options.file.display(), e))?;
    let checksum = format!("sha256:{}", sha256_hex(&bytes));

    let repo = if options.upload {
        Some(determine_repo(options.repo)?)
    } else {
        options.repo.map(|r| r.to_string())
    };

    let registry_url = if let Some(base) = options.base_url {
        format!("{}/{}", base.trim_end_matches('/'), asset_name)
    } else if let Some(ref repo) = repo {
        format!(
            "https://github.com/{}/releases/download/{}/{}",
            repo, tag, asset_name
        )
    } else {
        default_registry_url_for(&package_name, &package_version)
    };

    if options.upload {
        let repo = repo.as_ref().expect("upload requires a repo");
        upload_to_github_release(options.file, &asset_name, &tag, repo)?;
    }

    let entry = RegistryVersion {
        url: registry_url,
        checksum: Some(checksum),
        dependencies: BTreeMap::new(),
    };

    if let Some(registry_file) = options.registry_file {
        let mut index = load_or_create_registry(registry_file)?;
        let package_entry = index
            .packages
            .entry(package_name.clone())
            .or_default();
        if package_entry.contains_key(&package_version) {
            return Err(format!(
                "package '{} {}' already exists in registry",
                package_name, package_version
            ));
        }
        package_entry.insert(package_version.clone(), entry);

        let index_json = serde_json::to_string_pretty(&index)
            .map_err(|e| format!("cannot serialize registry: {}", e))?;
        fs::write(registry_file, index_json)
            .map_err(|e| format!("cannot write {}: {}", registry_file.display(), e))?;

        println!(
            "Updated {} with {} {}",
            registry_file.display(),
            package_name,
            package_version
        );
    } else {
        let snippet = serde_json::to_string_pretty(&BTreeMap::from([(
            package_version.clone(),
            entry,
        )]))
        .map_err(|e| format!("cannot serialize entry: {}", e))?;
        println!(
            "Add the following entry for package '{}' to your registry.json:\n{}",
            package_name, snippet
        );
    }

    Ok(())
}

fn upload_to_github_release(file: &Path, asset_name: &str, tag: &str, repo: &str) -> Result<(), String> {
    // Verify gh is available.
    Command::new("gh")
        .arg("--version")
        .output()
        .map_err(|e| format!("'gh' CLI is required for --upload but was not found: {}", e))?;

    // Check whether the release already exists.
    let view = Command::new("gh")
        .args([
            "release",
            "view",
            tag,
            "--repo",
            repo,
        ])
        .output()
        .map_err(|e| format!("failed to run 'gh release view': {}", e))?;

    if view.status.success() {
        // Release exists: upload (overwrite) the asset.
        let output = Command::new("gh")
            .args([
                "release",
                "upload",
                tag,
                file.to_string_lossy().as_ref(),
                "--repo",
                repo,
                "--clobber",
            ])
            .output()
            .map_err(|e| format!("failed to run 'gh release upload': {}", e))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("gh release upload failed: {}", stderr.trim()));
        }
        println!(
            "Uploaded {} to release {} on {}",
            asset_name,
            tag,
            repo
        );
    } else {
        // Release does not exist: create it with the asset.
        let output = Command::new("gh")
            .args([
                "release",
                "create",
                tag,
                file.to_string_lossy().as_ref(),
                "--repo",
                repo,
                "--title",
                tag,
                "--notes",
                "",
            ])
            .output()
            .map_err(|e| format!("failed to run 'gh release create': {}", e))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("gh release create failed: {}", stderr.trim()));
        }
        println!(
            "Created release {} on {} and uploaded {}",
            tag,
            repo,
            asset_name
        );
    }

    Ok(())
}

fn determine_repo(override_repo: Option<&str>) -> Result<String, String> {
    if let Some(repo) = override_repo {
        return Ok(repo.to_string());
    }

    // Try to detect owner/repo from the origin remote.
    let output = Command::new("git")
        .args(["remote", "get-url", "origin"])
        .output()
        .map_err(|e| format!("cannot run git to detect repo: {}", e))?;
    if !output.status.success() {
        return Err("--repo is required when uploading and no git origin remote is available".to_string());
    }
    let url = String::from_utf8_lossy(&output.stdout);
    parse_github_repo(url.trim())
}

fn parse_github_repo(url: &str) -> Result<String, String> {
    // https://github.com/owner/repo.git
    // git@github.com:owner/repo.git
    let url = url.strip_suffix(".git").unwrap_or(url);
    if let Some(rest) = url.strip_prefix("https://github.com/") {
        let parts: Vec<&str> = rest.split('/').collect();
        if parts.len() >= 2 {
            return Ok(format!("{}/{}", parts[0], parts[1]));
        }
    }
    if let Some(rest) = url.strip_prefix("git@github.com:") {
        let parts: Vec<&str> = rest.split('/').collect();
        if parts.len() >= 2 {
            return Ok(format!("{}/{}", parts[0], parts[1]));
        }
    }
    Err(format!("cannot parse GitHub repo from remote url '{}'", url))
}

fn determine_package_name(file: &Path, override_name: Option<&str>) -> Result<String, String> {
    if let Some(name) = override_name {
        return Ok(name.to_string());
    }

    // Try period.toml in the file's directory or current directory.
    let candidates = [
        file.parent().map(|p| p.join("period.toml")),
        Some(PathBuf::from("period.toml")),
    ];
    for candidate in candidates.into_iter().flatten() {
        if candidate.exists()
            && let Ok(manifest) = PeriodToml::load(&candidate)
        {
            return Ok(manifest.package.name);
        }
    }

    // Fall back to the file stem.
    file.file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .ok_or_else(|| format!("cannot determine package name from '{}'", file.display()))
}

fn determine_version(override_version: Option<&str>) -> Result<String, String> {
    if let Some(v) = override_version {
        return Ok(v.to_string());
    }

    let manifest_path = PathBuf::from("period.toml");
    if manifest_path.exists()
        && let Ok(manifest) = PeriodToml::load(&manifest_path)
    {
        return Ok(manifest.package.version);
    }

    Ok("1.0.0".to_string())
}

fn default_registry_url_for(name: &str, version: &str) -> String {
    format!(
        "https://github.com/period-lang/registry/releases/download/{}-{}/{}-{}.period",
        name, version, name, version
    )
}

fn load_or_create_registry(path: &Path) -> Result<RegistryIndex, String> {
    if path.exists() {
        let text = fs::read_to_string(path)
            .map_err(|e| format!("cannot read {}: {}", path.display(), e))?;
        serde_json::from_str(&text)
            .map_err(|e| format!("invalid registry {}: {}", path.display(), e))
    } else {
        Ok(RegistryIndex {
            schema_version: "1".to_string(),
            packages: BTreeMap::new(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn publish_prints_entry_without_registry_file() {
        let tmp = env::temp_dir().join(format!("period-publish-test-{}", std::process::id()));
        fs::create_dir_all(&tmp).expect("should create temp dir");
        let src = tmp.join("greet.period");
        fs::write(&src,
            "export hi.\ndefine hi with x:\n    return x.\n",
        )
        .expect("should write source file");

        publish(PublishOptions {
            file: &src,
            name: None,
            version: Some("1.2.3"),
            registry_file: None,
            base_url: None,
            upload: false,
            repo: None,
        })
        .expect("publish should succeed");

        fs::remove_dir_all(&tmp).expect("should remove temp dir");
    }

    #[test]
    fn publish_creates_registry_file() {
        let tmp = env::temp_dir().join(format!("period-publish-reg-test-{}", std::process::id()));
        fs::create_dir_all(&tmp).expect("should create temp dir");
        let src = tmp.join("greet.period");
        fs::write(&src, "export hi.").expect("should write source file");
        let reg = tmp.join("registry.json");

        publish(PublishOptions {
            file: &src,
            name: None,
            version: Some("1.2.3"),
            registry_file: Some(&reg),
            base_url: None,
            upload: false,
            repo: None,
        })
        .expect("publish should succeed");

        assert!(reg.exists());
        let index: RegistryIndex = serde_json::from_str(&fs::read_to_string(&reg).expect("registry file should read")).expect("registry should parse as JSON");
        assert_eq!(index.schema_version, "1");
        let versions = index.packages.get("greet").expect("greet package");
        assert!(versions.contains_key("1.2.3"));
        assert!(versions
            .get("1.2.3")
            .expect("1.2.3 version")
            .checksum
            .as_ref()
            .expect("1.2.3 checksum")
            .starts_with("sha256:"));

        fs::remove_dir_all(&tmp).expect("should remove temp dir");
    }

    #[test]
    fn publish_rejects_duplicate_version() {
        let tmp = env::temp_dir().join(format!("period-publish-dup-test-{}", std::process::id()));
        fs::create_dir_all(&tmp).expect("should create temp dir");
        let src = tmp.join("greet.period");
        fs::write(&src, "export hi.").expect("should write source file");
        let reg = tmp.join("registry.json");

        publish(PublishOptions {
            file: &src,
            name: None,
            version: Some("1.0.0"),
            registry_file: Some(&reg),
            base_url: None,
            upload: false,
            repo: None,
        })
        .expect("first publish should succeed");
        let result = publish(PublishOptions {
            file: &src,
            name: None,
            version: Some("1.0.0"),
            registry_file: Some(&reg),
            base_url: None,
            upload: false,
            repo: None,
        });
        assert!(result.is_err());

        fs::remove_dir_all(&tmp).expect("should remove temp dir");
    }

    #[test]
    fn parse_github_repo_variants() {
        assert_eq!(parse_github_repo("https://github.com/owner/repo.git").unwrap(), "owner/repo");
        assert_eq!(parse_github_repo("https://github.com/owner/repo").unwrap(), "owner/repo");
        assert_eq!(parse_github_repo("git@github.com:owner/repo.git").unwrap(), "owner/repo");
        assert!(parse_github_repo("https://gitlab.com/owner/repo.git").is_err());
    }

    #[test]
    fn publish_uses_repo_release_url() {
        let tmp = env::temp_dir().join(format!("period-publish-url-test-{}", std::process::id()));
        fs::create_dir_all(&tmp).expect("should create temp dir");
        let src = tmp.join("greet.period");
        fs::write(&src, "export hi.").expect("should write source file");
        let reg = tmp.join("registry.json");

        publish(PublishOptions {
            file: &src,
            name: Some("greet"),
            version: Some("1.0.0"),
            registry_file: Some(&reg),
            base_url: None,
            upload: false,
            repo: Some("myorg/registry"),
        })
        .expect("publish should succeed");

        let index: RegistryIndex = serde_json::from_str(&fs::read_to_string(&reg).expect("registry should read")).expect("registry should parse");
        let entry = index.packages.get("greet").unwrap().get("1.0.0").unwrap();
        assert_eq!(
            entry.url,
            "https://github.com/myorg/registry/releases/download/greet-1.0.0/greet-1.0.0.period"
        );

        fs::remove_dir_all(&tmp).expect("should remove temp dir");
    }
}
