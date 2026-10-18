//! `helixforge fetch-release`: download one of HelixForge's own release files
//! and check it against the release's SHA256SUMS before keeping it. The
//! Updates screen's HelixForge channel runs it before the package manager
//! installs the file; it takes no URL, only a release version and one of the
//! release's file names.

use anyhow::{Context, Result, bail};
use helixforge_core::PackageManager;
use helixforge_engine::updates::{RELEASES, package_asset_for, program_asset};
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::Command,
};

/// The files a release has that HelixForge may fetch, for each architecture
/// there are builds for.
fn allowed(version: &str) -> Vec<String> {
    let mut files = Vec::new();
    for architecture in helixforge_core::ARCHITECTURES {
        let packages: Vec<String> = [
            PackageManager::Arch,
            PackageManager::Debian,
            PackageManager::Fedora,
        ]
        .into_iter()
        .filter_map(|manager| package_asset_for(manager, version, architecture))
        .collect();
        if !packages.is_empty() {
            files.extend(packages);
            files.extend(program_asset(architecture));
        }
    }
    files
}

/// Download `asset` of release `version` into `directory`, checked; returns
/// its path. `base` is where releases are downloaded from, and `protocols`
/// what curl may use to reach it (only https outside tests).
pub fn fetch(
    version: &str,
    asset: &str,
    directory: &Path,
    base: &str,
    protocols: &str,
) -> Result<PathBuf> {
    if !allowed(version).iter().any(|file| file == asset) {
        bail!("{asset} is not a file of HelixForge release {version}");
    }
    fs::create_dir_all(directory)
        .with_context(|| format!("cannot create {}", directory.display()))?;
    fs::set_permissions(directory, fs::Permissions::from_mode(0o700))
        .with_context(|| format!("cannot make {} private", directory.display()))?;
    let url = |file: &str| format!("{base}/v{version}/{file}");
    let download = |file: &str, to: &Path, progress: bool| -> Result<()> {
        let status = Command::new("curl")
            .args(["--fail", "--location", "--proto", protocols])
            .arg(if progress {
                "--progress-bar"
            } else {
                "--silent"
            })
            .arg("--output")
            .arg(to)
            .arg(url(file))
            .status()
            .context("cannot run curl")?;
        if !status.success() {
            bail!("downloading {} failed", url(file));
        }
        Ok(())
    };
    let sums = directory.join("SHA256SUMS");
    download("SHA256SUMS", &sums, false)?;
    let expected = fs::read_to_string(&sums)?
        .lines()
        .find_map(|line| {
            let (sum, name) = line.split_once(char::is_whitespace)?;
            (name.trim_start().trim_start_matches('*') == asset
                && sum.len() == 64
                && sum.bytes().all(|byte| byte.is_ascii_hexdigit()))
            .then(|| sum.to_ascii_lowercase())
        })
        .with_context(|| format!("release {version}'s SHA256SUMS does not list {asset}"))?;
    let _ = fs::remove_file(&sums);
    let partial = directory.join(format!("{asset}.part"));
    println!("Downloading {} …", url(asset));
    download(asset, &partial, true)?;
    let output = Command::new("sha256sum")
        .arg("--")
        .arg(&partial)
        .output()
        .context("cannot run sha256sum")?;
    let actual = String::from_utf8_lossy(&output.stdout)
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    if !output.status.success() || actual != expected {
        let _ = fs::remove_file(&partial);
        bail!("{asset} does not match the release's SHA256SUMS; it was deleted");
    }
    let file = directory.join(asset);
    fs::rename(&partial, &file)?;
    println!("Checked {asset} against the release's SHA256SUMS.");
    Ok(file)
}

/// Where the published releases are downloaded from.
pub fn base() -> String {
    format!("{RELEASES}/releases/download")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sha256(path: &Path) -> String {
        let output = Command::new("sha256sum").arg(path).output().unwrap();
        String::from_utf8_lossy(&output.stdout)
            .split_whitespace()
            .next()
            .unwrap()
            .to_string()
    }

    /// A release folder served from the file system, as curl's file:// sees it.
    fn release(good: bool) -> (tempfile::TempDir, String) {
        let server = tempfile::tempdir().unwrap();
        let folder = server.path().join("v1.2.3");
        fs::create_dir_all(&folder).unwrap();
        let asset = "helixforge-1.2.3-1-x86_64.pkg.tar.zst";
        fs::write(folder.join(asset), b"package").unwrap();
        let sum = if good {
            sha256(&folder.join(asset))
        } else {
            "0".repeat(64)
        };
        fs::write(folder.join("SHA256SUMS"), format!("{sum}  {asset}\n")).unwrap();
        let base = format!("file://{}", server.path().display());
        (server, base)
    }

    #[test]
    fn a_matching_file_is_kept_and_a_mismatched_one_deleted() {
        let asset = "helixforge-1.2.3-1-x86_64.pkg.tar.zst";
        let (_server, base) = release(true);
        let cache = tempfile::tempdir().unwrap();
        let directory = cache.path().join("release");
        let file = fetch("1.2.3", asset, &directory, &base, "=file").unwrap();
        assert_eq!(fs::read(&file).unwrap(), b"package");
        let mode = fs::metadata(&directory).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o700);

        let (_server, base) = release(false);
        let other = cache.path().join("other");
        let error = fetch("1.2.3", asset, &other, &base, "=file").unwrap_err();
        assert!(error.to_string().contains("does not match"), "{error}");
        assert!(!other.join(asset).exists());
        assert!(!other.join(format!("{asset}.part")).exists());
    }

    #[test]
    fn only_release_files_are_fetched() {
        let cache = tempfile::tempdir().unwrap();
        for (version, asset) in [
            ("1.2.3", "../../etc/passwd"),
            ("1.2.3", "helixforge-9.9.9-1-x86_64.pkg.tar.zst"),
            ("1.2", "helixforge-1.2-1-x86_64.pkg.tar.zst"),
            ("1.2.3", "SHA256SUMS"),
        ] {
            assert!(
                fetch(version, asset, cache.path(), "file:///nowhere", "=file").is_err(),
                "{version} {asset}"
            );
        }
        // Nothing reaches the network or the disk for a refused file.
        assert!(fs::read_dir(cache.path()).unwrap().next().is_none());
        assert!(allowed("1.2.3").contains(&"helixforge_1.2.3-1_amd64.deb".to_string()));
        assert!(allowed("1.2.3").contains(&"helixforge-1.2.3-1.aarch64.rpm".to_string()));
        assert!(allowed("1.2.3").contains(&"helixforge-linux-aarch64".to_string()));
    }
}
