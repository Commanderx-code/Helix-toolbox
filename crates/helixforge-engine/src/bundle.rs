//! Offline installation bundles: a folder holding HelixForge itself (with
//! its catalog inside), the packages chosen entries need for this machine's
//! distribution and architecture, and a manifest of every file's checksum.
//! On a machine of the same distribution without reliable internet, the
//! bundle is checked, then its packages are installed by the package manager
//! from the files, without downloading. What needs the network (Flatpaks,
//! AUR builds, scripts) is named, never pretended to be offline.

use crate::{Plan, Step, doctor::System, quote};
use helixforge_catalog::Catalog;
use helixforge_core::{Action, Entry, PackageManager, Risk, printable, valid_package_name};
use helixforge_platform::LinuxInfo;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256, Sha512};
use std::path::{Component, Path, PathBuf};

/// The manifest format this version writes.
pub const FORMAT: u32 = 1;
/// The manifest's file name in a bundle.
pub const MANIFEST: &str = "bundle.toml";
/// The checksums, for `sha256sum --check` by hand.
pub const SUMS: &str = "SHA256SUMS";

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct Manifest {
    pub format: u32,
    /// Unix seconds.
    pub created: u64,
    pub helixforge: String,
    /// Whether the HelixForge in it is a release build.
    #[serde(default)]
    pub release: bool,
    pub distribution: String,
    /// arch, debian or fedora.
    pub family: String,
    /// x86_64, aarch64.
    pub architecture: String,
    /// Whether every dependency is in it, not only those this machine lacked.
    #[serde(default)]
    pub complete: bool,
    #[serde(default, rename = "entry")]
    pub entries: Vec<BundledEntry>,
    #[serde(default, rename = "file")]
    pub files: Vec<BundledFile>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct BundledEntry {
    pub id: String,
    pub name: String,
    /// Whether it installs from the bundle alone.
    pub offline: bool,
    /// Its own packages, by name.
    #[serde(default)]
    pub packages: Vec<String>,
    /// Why it needs the network, when it does.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub network: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct BundledFile {
    /// Relative to the bundle.
    pub path: String,
    pub sha256: String,
    pub bytes: u64,
    /// Where it was downloaded from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// What else vouches for it: a signature the package manager checks, or
    /// the repository's signed index it was checked against.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assurance: Option<String>,
}

/// A package to download.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Download {
    pub url: String,
    pub file: String,
    /// The checksum the repository's signed index gives, as `sha256:…` or
    /// `sha512:…`, when the package manager says.
    pub checksum: Option<String>,
}

/// What a bundle would hold.
#[derive(Clone, Debug, Default)]
pub struct Contents {
    pub entries: Vec<BundledEntry>,
    pub downloads: Vec<Download>,
    /// Fedora's dnf downloads the packages itself: these names.
    pub dnf: Vec<String>,
}

/// Why a bundle cannot be made, read or used.
#[derive(Debug, thiserror::Error)]
pub enum BundleError {
    #[error("bundles are made with pacman, apt or dnf; {0} is not supported yet")]
    Unsupported(String),
    #[error("{0} already exists and is not empty; choose a new folder")]
    NotEmpty(PathBuf),
    #[error("the package manager could not resolve the packages: {0}")]
    Resolve(String),
    #[error("downloading {0} failed")]
    Download(String),
    #[error("{0} does not match the checksum its repository gives; it was deleted")]
    Mismatch(String),
    #[error("{0}")]
    Io(String),
    #[error("{0} is not a HelixForge bundle: {1}")]
    Format(PathBuf, String),
    #[error("the bundle was made for {0}; this machine is {1}")]
    Machine(String, String),
    #[error("the bundle is damaged: {0}")]
    Damaged(String),
    #[error(
        "the bundle installs as a whole: its packages, shared between {0}, go in together; make a bundle of only the entries wanted"
    )]
    Partial(String),
}

/// The family a bundle is made for.
fn family(manager: Option<PackageManager>) -> Result<(PackageManager, &'static str), BundleError> {
    match manager {
        Some(PackageManager::Arch) => Ok((PackageManager::Arch, "arch")),
        Some(PackageManager::Debian) => Ok((PackageManager::Debian, "debian")),
        Some(PackageManager::Fedora) => Ok((PackageManager::Fedora, "fedora")),
        Some(other) => Err(BundleError::Unsupported(other.executable().to_string())),
        None => Err(BundleError::Unsupported("this system".to_string())),
    }
}

/// Which entries can go offline, and the packages they need. `complete`
/// takes every dependency; otherwise those this machine lacks, which suits
/// a machine set up like this one. `scratch` is a folder for pacman's empty
/// database.
pub fn contents(
    ids: &[String],
    catalog: &Catalog,
    platform: &LinuxInfo,
    complete: bool,
    system: &dyn System,
    scratch: &Path,
) -> Result<Contents, BundleError> {
    let (manager, _) = family(platform.manager)?;
    let mut found = Contents::default();
    let mut names: Vec<String> = Vec::new();
    let mut seen: Vec<String> = Vec::new();
    let mut queue: Vec<String> = ids.iter().rev().cloned().collect();
    while let Some(id) = queue.pop() {
        if seen.contains(&id) {
            continue;
        }
        seen.push(id.clone());
        let Some(entry) = catalog.find(&id) else {
            found.entries.push(BundledEntry {
                name: printable(&id),
                id,
                offline: false,
                packages: Vec::new(),
                network: Some("not in this catalog".to_string()),
            });
            continue;
        };
        if let Action::Profile { entries } = &entry.action {
            queue.extend(entries.iter().rev().cloned());
            continue;
        }
        let (offline, network) = classify(entry, manager);
        let packages = offline.clone().into_iter().collect::<Vec<_>>();
        for name in &packages {
            if !names.contains(name) {
                names.push(name.clone());
            }
        }
        found.entries.push(BundledEntry {
            id: entry.id.clone(),
            name: entry.name.clone(),
            offline: offline.is_some(),
            packages,
            network,
        });
    }
    if names.is_empty() {
        return Ok(found);
    }
    let words: Vec<&str> = names.iter().map(String::as_str).collect();
    match manager {
        PackageManager::Arch => {
            // An empty cache, so pacman gives the mirrors' addresses rather
            // than those of packages it keeps on this machine.
            let cache = scratch.join("cache");
            std::fs::create_dir_all(&cache).map_err(|error| BundleError::Io(error.to_string()))?;
            let mut command: Vec<String> = vec![
                "pacman".into(),
                "-Sp".into(),
                "--cachedir".into(),
                cache.display().to_string(),
            ];
            if complete {
                // An empty local database, so every dependency is listed; the
                // sync databases are this machine's.
                let local = scratch.join("db/local");
                std::fs::create_dir_all(&local)
                    .map_err(|error| BundleError::Io(error.to_string()))?;
                let sync = scratch.join("db/sync");
                if !sync.exists() {
                    std::os::unix::fs::symlink("/var/lib/pacman/sync", &sync)
                        .map_err(|error| BundleError::Io(error.to_string()))?;
                }
                command.push("--dbpath".into());
                command.push(scratch.join("db").display().to_string());
            }
            command.extend(["--print-format".into(), "%f %l".into()]);
            command.extend(names.iter().cloned());
            let command: Vec<&str> = command.iter().map(String::as_str).collect();
            let Some((true, text)) = system.run(&command) else {
                return Err(BundleError::Resolve("pacman -Sp".to_string()));
            };
            for line in text.lines() {
                let Some((file, url)) = line.trim().split_once(' ') else {
                    continue;
                };
                found.downloads.push(Download {
                    url: url.to_string(),
                    file: file.to_string(),
                    checksum: None,
                });
                // Arch signs every package: the signature goes with it.
                found.downloads.push(Download {
                    url: format!("{url}.sig"),
                    file: format!("{file}.sig"),
                    checksum: None,
                });
            }
        }
        PackageManager::Debian => {
            // --reinstall lists packages this machine already has, too.
            let mut command = vec!["apt-get", "--print-uris", "-qq", "--reinstall"];
            if complete {
                command.extend(["-o", "Dir::State::status=/dev/null"]);
            }
            command.push("install");
            command.extend(words);
            let Some((true, text)) = system.run(&command) else {
                return Err(BundleError::Resolve("apt-get --print-uris".to_string()));
            };
            for line in text.lines() {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() < 4 {
                    continue;
                }
                found.downloads.push(Download {
                    url: parts[0].trim_matches('\'').to_string(),
                    file: parts[1].to_string(),
                    checksum: Some(parts[3].to_ascii_lowercase()),
                });
            }
        }
        _ => found.dnf = names,
    }
    for download in &found.downloads {
        if !plain_file(&download.file) {
            return Err(BundleError::Resolve(format!(
                "the package manager named a file HelixForge does not accept: {}",
                printable(&download.file)
            )));
        }
    }
    Ok(found)
}

/// The native package an entry installs offline, or why it needs the network.
fn classify(entry: &Entry, manager: PackageManager) -> (Option<String>, Option<String>) {
    match &entry.action {
        Action::Package {
            packages,
            aur,
            flatpak,
            ..
        } => match packages.for_manager(manager) {
            Some(name) if valid_package_name(name) => (Some(name.to_string()), None),
            _ if aur.is_some() => (None, Some("built from the AUR".to_string())),
            _ if flatpak.is_some() => (None, Some("installed from Flathub".to_string())),
            _ => (None, Some("no package for this distribution".to_string())),
        },
        Action::Script { .. } => (
            None,
            Some("its script may download what it installs".to_string()),
        ),
        Action::Command { .. } => (None, Some("its command may need the network".to_string())),
        Action::Nix { .. } => (None, Some("Nix fetches from its caches".to_string())),
        Action::Native { .. } => (
            None,
            Some("nothing to install; it runs anywhere".to_string()),
        ),
        Action::Profile { .. } => (None, None),
    }
}

/// A file name a bundle may hold: one name, no path, plain characters.
fn plain_file(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 255
        && !name.starts_with('.')
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"+._@:~-".contains(&byte))
}

fn sha256_file(path: &Path) -> Result<(String, u64), BundleError> {
    let mut file = std::fs::File::open(path).map_err(|error| BundleError::Io(error.to_string()))?;
    let mut hasher = Sha256::new();
    let bytes = std::io::copy(&mut file, &mut hasher)
        .map_err(|error| BundleError::Io(error.to_string()))?;
    Ok((hex(&hasher.finalize()), bytes))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Whether a file matches a repository's `sha256:…` or `sha512:…`.
fn matches(path: &Path, checksum: &str) -> Result<bool, BundleError> {
    let (kind, want) = checksum.split_once(':').unwrap_or(("sha256", checksum));
    let bytes = std::fs::read(path).map_err(|error| BundleError::Io(error.to_string()))?;
    let have = match kind {
        "sha512" => hex(&Sha512::digest(&bytes)),
        _ => hex(&Sha256::digest(&bytes)),
    };
    Ok(have == want)
}

/// What making a bundle asks the machine; tests answer instead.
pub struct Tools<'a> {
    pub system: &'a dyn System,
    /// Download a URL to a file; false when it failed.
    pub download: &'a dyn Fn(&str, &Path) -> bool,
    /// Have dnf download packages with their dependencies into a folder.
    pub dnf_download: &'a dyn Fn(&[String], bool, &Path) -> bool,
}

/// Make a bundle in `directory`, which must be new or empty: the packages,
/// a copy of `helixforge`, the manifest and the checksums.
#[allow(clippy::too_many_arguments)]
pub fn create(
    directory: &Path,
    contents: &Contents,
    platform: &LinuxInfo,
    complete: bool,
    helixforge: Option<(&Path, &str, bool)>,
    now: u64,
    tools: &Tools,
) -> Result<Manifest, BundleError> {
    let (_, family_name) = family(platform.manager)?;
    if std::fs::read_dir(directory).is_ok_and(|mut listing| listing.next().is_some()) {
        return Err(BundleError::NotEmpty(directory.to_path_buf()));
    }
    let packages = directory.join("packages");
    std::fs::create_dir_all(&packages).map_err(|error| BundleError::Io(error.to_string()))?;
    let mut files = Vec::new();
    for download in &contents.downloads {
        let target = packages.join(&download.file);
        if !(tools.download)(&download.url, &target) {
            return Err(BundleError::Download(printable(&download.url)));
        }
        let mut assurance = None;
        if let Some(checksum) = &download.checksum {
            let strong = checksum.starts_with("sha256:") || checksum.starts_with("sha512:");
            let good = if strong {
                matches(&target, checksum)?
            } else {
                // apt printed a weaker hash: ask its index for the SHA-256.
                let (sha256, _) = sha256_file(&target)?;
                known_to_apt(&target, &sha256, tools.system)
            };
            if !good {
                let _ = std::fs::remove_file(&target);
                return Err(BundleError::Mismatch(printable(&download.file)));
            }
            assurance = Some("its checksum matched the repository's signed index".to_string());
        } else if download.file.ends_with(".sig") {
            assurance = Some(
                "the package's signature; pacman checks it against its keyring when it installs"
                    .to_string(),
            );
        } else if family_name == "arch" {
            assurance = Some("signed: pacman checks its .sig when it installs".to_string());
        }
        let (sha256, bytes) = sha256_file(&target)?;
        files.push(BundledFile {
            path: format!("packages/{}", download.file),
            sha256,
            bytes,
            source: Some(printable(&download.url)),
            assurance,
        });
    }
    if !contents.dnf.is_empty() {
        if !(tools.dnf_download)(&contents.dnf, complete, &packages) {
            return Err(BundleError::Download(
                "the packages, through dnf download".to_string(),
            ));
        }
        let mut names: Vec<String> = std::fs::read_dir(&packages)
            .map_err(|error| BundleError::Io(error.to_string()))?
            .flatten()
            .map(|item| item.file_name().to_string_lossy().into_owned())
            .filter(|name| name.ends_with(".rpm") && plain_file(name))
            .collect();
        names.sort();
        for name in names {
            let (sha256, bytes) = sha256_file(&packages.join(&name))?;
            files.push(BundledFile {
                path: format!("packages/{name}"),
                sha256,
                bytes,
                source: Some("dnf download".to_string()),
                assurance: Some("signed: rpm -K checks it before it installs".to_string()),
            });
        }
    }
    let (version, release) = match helixforge {
        Some((program, version, release)) => {
            let target = directory.join("helixforge");
            std::fs::copy(program, &target).map_err(|error| BundleError::Io(error.to_string()))?;
            let (sha256, bytes) = sha256_file(&target)?;
            files.push(BundledFile {
                path: "helixforge".to_string(),
                sha256,
                bytes,
                source: None,
                assurance: Some(if release {
                    "a release build: gh attestation verify, against the release's own file, shows where it was built".to_string()
                } else {
                    "built from source, so there is no build provenance".to_string()
                }),
            });
            (version.to_string(), release)
        }
        None => (String::new(), false),
    };
    let manifest = Manifest {
        format: FORMAT,
        created: now,
        helixforge: version,
        release,
        distribution: printable(&platform.pretty_name),
        family: family_name.to_string(),
        architecture: std::env::consts::ARCH.to_string(),
        complete,
        entries: contents.entries.clone(),
        files,
    };
    let text = toml::to_string(&manifest).map_err(|error| BundleError::Io(error.to_string()))?;
    std::fs::write(
        directory.join(MANIFEST),
        format!("# A HelixForge offline bundle: ./helixforge bundle install <this folder>\n{text}"),
    )
    .map_err(|error| BundleError::Io(error.to_string()))?;
    let sums: String = manifest
        .files
        .iter()
        .map(|file| format!("{}  {}\n", file.sha256, file.path))
        .collect();
    std::fs::write(directory.join(SUMS), sums)
        .map_err(|error| BundleError::Io(error.to_string()))?;
    Ok(manifest)
}

/// Read a bundle's manifest and check every file against it: the same
/// distribution family and architecture, every file there, with its
/// checksum, and nothing that climbs out of the bundle.
pub fn verify(directory: &Path, platform: &LinuxInfo) -> Result<Manifest, BundleError> {
    let path = directory.join(MANIFEST);
    let size = std::fs::metadata(&path)
        .map_err(|error| BundleError::Format(path.clone(), error.to_string()))?
        .len();
    if size > 16 << 20 {
        return Err(BundleError::Damaged(
            "its manifest is larger than any bundle's".to_string(),
        ));
    }
    let text = std::fs::read_to_string(&path)
        .map_err(|error| BundleError::Format(path.clone(), error.to_string()))?;
    let manifest: Manifest = toml::from_str(&text)
        .map_err(|error| BundleError::Format(path.clone(), printable(error.message())))?;
    if manifest.format > FORMAT {
        return Err(BundleError::Format(
            path,
            format!("format {} is from a later HelixForge", manifest.format),
        ));
    }
    if !plain_manifest(&manifest) {
        return Err(BundleError::Damaged(
            "its manifest has control characters, malformed checksums or too many records"
                .to_string(),
        ));
    }
    let (_, family_name) = family(platform.manager)?;
    if manifest.family != family_name || manifest.architecture != std::env::consts::ARCH {
        return Err(BundleError::Machine(
            printable(&format!(
                "{} ({}, {})",
                manifest.distribution, manifest.family, manifest.architecture
            )),
            format!(
                "{} ({family_name}, {})",
                printable(&platform.pretty_name),
                std::env::consts::ARCH
            ),
        ));
    }
    for file in &manifest.files {
        let relative = Path::new(&file.path);
        let inside = relative
            .components()
            .all(|component| matches!(component, Component::Normal(_)));
        let name_ok = relative
            .file_name()
            .is_some_and(|name| plain_file(&name.to_string_lossy()));
        if !inside || !name_ok || !(file.path == "helixforge" || file.path.starts_with("packages/"))
        {
            return Err(BundleError::Damaged(format!(
                "it names {}",
                printable(&file.path)
            )));
        }
        let full = directory.join(relative);
        let (sha256, bytes) = sha256_file(&full)
            .map_err(|_| BundleError::Damaged(format!("{} is missing", printable(&file.path))))?;
        if sha256 != file.sha256 || bytes != file.bytes {
            return Err(BundleError::Damaged(format!(
                "{} does not match its checksum",
                printable(&file.path)
            )));
        }
    }
    for entry in &manifest.entries {
        if !helixforge_core::valid_id(&entry.id)
            || !entry.packages.iter().all(|name| valid_package_name(name))
        {
            return Err(BundleError::Damaged(format!(
                "it names {}",
                printable(&entry.id)
            )));
        }
    }
    // Arch packages install only with the signature beside them, which is
    // checked before they do: a detached signature and nothing else.
    if family_name == "arch" {
        for file in manifest
            .files
            .iter()
            .filter(|file| file.path.ends_with(".sig"))
        {
            let bytes = std::fs::read(directory.join(&file.path))
                .map_err(|error| BundleError::Io(error.to_string()))?;
            if !detached_signature(&bytes) {
                return Err(BundleError::Damaged(format!(
                    "{} is not a detached signature",
                    printable(&file.path)
                )));
            }
        }
        for file in &manifest.files {
            let signed = format!("{}.sig", file.path);
            if file.path.starts_with("packages/")
                && !file.path.ends_with(".sig")
                && !manifest.files.iter().any(|other| other.path == signed)
            {
                return Err(BundleError::Damaged(format!(
                    "{} has no signature",
                    printable(&file.path)
                )));
            }
        }
    }
    Ok(manifest)
}

/// Whether bytes are binary OpenPGP signature packets and nothing else, as a
/// detached signature is: no literal data, no compressed or one-pass
/// packets that could carry a message of their own.
pub fn detached_signature(bytes: &[u8]) -> bool {
    if bytes.is_empty() || bytes.len() > 64 * 1024 {
        return false;
    }
    let mut at = 0;
    while at < bytes.len() {
        let header = bytes[at];
        if header & 0x80 == 0 {
            return false;
        }
        let (tag, length, used) = if header & 0x40 != 0 {
            // New format: one, two or five length octets.
            let tag = header & 0x3f;
            let Some(&first) = bytes.get(at + 1) else {
                return false;
            };
            match first {
                0..=191 => (tag, first as usize, 2),
                192..=223 => {
                    let Some(&second) = bytes.get(at + 2) else {
                        return false;
                    };
                    (
                        tag,
                        ((first as usize - 192) << 8) + second as usize + 192,
                        3,
                    )
                }
                255 => {
                    let Some(four) = bytes.get(at + 2..at + 6) else {
                        return false;
                    };
                    (
                        tag,
                        u32::from_be_bytes([four[0], four[1], four[2], four[3]]) as usize,
                        6,
                    )
                }
                // Partial lengths belong to data packets, not signatures.
                _ => return false,
            }
        } else {
            let tag = (header >> 2) & 0x0f;
            match header & 0x03 {
                0 => {
                    let Some(&one) = bytes.get(at + 1) else {
                        return false;
                    };
                    (tag, one as usize, 2)
                }
                1 => {
                    let Some(two) = bytes.get(at + 1..at + 3) else {
                        return false;
                    };
                    (tag, u16::from_be_bytes([two[0], two[1]]) as usize, 3)
                }
                2 => {
                    let Some(four) = bytes.get(at + 1..at + 5) else {
                        return false;
                    };
                    (
                        tag,
                        u32::from_be_bytes([four[0], four[1], four[2], four[3]]) as usize,
                        5,
                    )
                }
                _ => return false,
            }
        };
        if tag != 2 {
            return false;
        }
        at += used + length;
    }
    at == bytes.len()
}

/// Plain text throughout: a manifest from someone else could otherwise put
/// escape sequences on the terminal. Checksums are hexadecimal.
fn plain_manifest(manifest: &Manifest) -> bool {
    let plain = |text: &str| text.len() <= 1000 && !text.chars().any(char::is_control);
    let mut texts: Vec<&str> = vec![
        &manifest.helixforge,
        &manifest.distribution,
        &manifest.family,
        &manifest.architecture,
    ];
    for entry in &manifest.entries {
        texts.push(&entry.id);
        texts.push(&entry.name);
        texts.extend(entry.network.as_deref());
        texts.extend(entry.packages.iter().map(String::as_str));
    }
    for file in &manifest.files {
        texts.push(&file.path);
        texts.extend(file.source.as_deref());
        texts.extend(file.assurance.as_deref());
    }
    texts.into_iter().all(plain)
        && manifest.files.iter().all(|file| {
            file.sha256.len() == 64 && file.sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
        })
        && manifest.files.len() <= 10_000
        && manifest.entries.len() <= 10_000
}

/// A plan that installs a verified bundle's packages from its files, for the
/// entries named (all of the bundle's when none). The files are copied into
/// a new private folder and checked there against the checksums of the
/// manifest verified here, which are written into the plan itself, so
/// nothing changed in the bundle afterwards is installed. Then each
/// package's signature is checked (pacman's with pacman-key, rpm's with rpm
/// -K), and the package manager installs them from the files.
///
/// Debian packages carry no signature of their own: each must be known to
/// this machine's repository index (signed, and checked by apt) with the
/// same checksum, unless `trust` takes the bundle's word for them.
pub fn plan_install(
    directory: &Path,
    manifest: &Manifest,
    ids: &[String],
    platform: &LinuxInfo,
    system: &dyn System,
    trust: bool,
) -> Result<Plan, BundleError> {
    let (manager, _) = family(platform.manager)?;
    // Entries share dependencies, so the bundle's packages install together:
    // naming some would still install them all, which the plan would not say.
    let offline_ids: Vec<&str> = manifest
        .entries
        .iter()
        .filter(|entry| entry.offline)
        .map(|entry| entry.id.as_str())
        .collect();
    if !ids.is_empty()
        && !offline_ids
            .iter()
            .all(|id| ids.iter().any(|wanted| wanted == id))
    {
        return Err(BundleError::Partial(offline_ids.join(", ")));
    }
    let chosen: Vec<&BundledEntry> = manifest.entries.iter().collect();
    let mut plan = Plan::default();
    for entry in &chosen {
        if let Some(network) = &entry.network {
            plan.notes.push(format!(
                "{}: needs the network ({}); run it when online",
                printable(&entry.name),
                printable(network)
            ));
        }
    }
    let offline: Vec<&&BundledEntry> = chosen.iter().filter(|entry| entry.offline).collect();
    if offline.is_empty() {
        return Ok(plan);
    }
    let directory =
        std::fs::canonicalize(directory).map_err(|error| BundleError::Io(error.to_string()))?;
    let packages: Vec<&BundledFile> = manifest
        .files
        .iter()
        .filter(|file| file.path.starts_with("packages/"))
        .collect();
    let name = |file: &BundledFile| {
        Path::new(&file.path)
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default()
    };
    if manager == PackageManager::Debian && !trust {
        let mut unknown = Vec::new();
        for file in packages.iter().filter(|file| file.path.ends_with(".deb")) {
            if !known_to_apt(&directory.join(&file.path), &file.sha256, system) {
                unknown.push(name(file));
            }
        }
        if !unknown.is_empty() {
            return Err(BundleError::Damaged(format!(
                "this machine's repository index does not know {} with the same checksum, so nothing but the bundle vouches for them; install with --trust-bundle only if you made the bundle yourself",
                unknown.join(", ")
            )));
        }
    }
    let staged = |file: &BundledFile| format!("\"$hf_staging\"/{}", quote(&name(file)));
    let mut lines = vec![
        "hf_staging=$(mktemp -d) || exit 1".to_string(),
        "trap 'rm -rf \"$hf_staging\"' EXIT".to_string(),
    ];
    for file in &packages {
        lines.push(format!(
            "cp -- {} \"$hf_staging\"/ || exit 1",
            quote(&directory.join(&file.path).display().to_string())
        ));
    }
    // The checksums verified here, not the bundle's own file.
    let sums: Vec<String> = packages
        .iter()
        .map(|file| quote(&format!("{}  {}", file.sha256, name(file))))
        .collect();
    lines.push(format!(
        "( cd \"$hf_staging\" && printf '%s\\n' {} | sha256sum --quiet --check - ) || {{ echo 'HelixForge: the bundle changed after it was checked; nothing was installed' >&2; exit 1; }}",
        sums.join(" ")
    ));
    let installable: Vec<&&BundledFile> = packages
        .iter()
        .filter(|file| !file.path.ends_with(".sig"))
        .collect();
    let mut requires = vec![
        "sha256sum".to_string(),
        "sudo".to_string(),
        "mktemp".to_string(),
    ];
    match manager {
        PackageManager::Arch => {
            for file in &installable {
                lines.push(format!(
                    "sudo pacman-key --verify {}.sig {} || exit 1",
                    staged(file),
                    staged(file)
                ));
            }
            requires.push("pacman-key".to_string());
        }
        PackageManager::Fedora => {
            let files: Vec<String> = installable.iter().map(|file| staged(file)).collect();
            lines.push(format!("rpm -K {} || exit 1", files.join(" ")));
            requires.push("rpm".to_string());
        }
        _ => {}
    }
    let files: Vec<String> = installable.iter().map(|file| staged(file)).collect();
    let (install, tool) = match manager {
        PackageManager::Arch => ("sudo pacman -U --needed", "pacman"),
        // apt's --no-download refuses the local files too; with everything
        // in the bundle, nothing is downloaded.
        PackageManager::Debian => ("sudo apt-get install", "apt-get"),
        _ => ("sudo dnf install --disablerepo='*'", "dnf"),
    };
    lines.push(format!("{install} {}", files.join(" ")));
    requires.push(tool.to_string());
    let names: Vec<String> = offline.iter().map(|entry| printable(&entry.name)).collect();
    let shown: Vec<String> = installable.iter().map(|file| name(file)).collect();
    plan.steps.push(Step::Shell {
        id: "bundle-install".to_string(),
        name: format!("Install {} from the bundle", names.join(", ")),
        program: lines.join("\n"),
        display: printable(&format!(
            "copy the packages from {} to a private folder, check them against the verified checksums{}, then {install} {}",
            directory.display(),
            match manager {
                PackageManager::Arch => " and their signatures (pacman-key --verify)",
                PackageManager::Fedora => " and their signatures (rpm -K)",
                _ => "",
            },
            shown.join(" ")
        )),
        requires: requires.clone(),
        changes: Vec::new(),
    });
    requires.sort();
    requires.dedup();
    plan.requirements = requires;
    plan.privileged = true;
    plan.risk = Risk::Medium;
    plan.depends = vec![Vec::new()];
    Ok(plan)
}

/// Whether this machine's apt index lists the package in this file, at its
/// version, with this checksum: `dpkg-deb` reads its name and version, and
/// `apt-cache show` what the signed index says.
fn known_to_apt(file: &Path, sha256: &str, system: &dyn System) -> bool {
    let path = file.display().to_string();
    let Some((true, fields)) = system.run(&["dpkg-deb", "--field", &path, "Package", "Version"])
    else {
        return false;
    };
    let field = |key: &str| {
        fields.lines().find_map(|line| {
            let (name, value) = line.split_once(':')?;
            (name.trim() == key).then(|| value.trim().to_string())
        })
    };
    let (Some(package), Some(version)) = (field("Package"), field("Version")) else {
        return false;
    };
    if !valid_package_name(&package) || version.chars().any(|c| c.is_whitespace() || c.is_control())
    {
        return false;
    }
    let Some((true, shown)) = system.run(&["apt-cache", "show", &format!("{package}={version}")])
    else {
        return false;
    };
    shown
        .lines()
        .filter_map(|line| line.strip_prefix("SHA256:"))
        .any(|value| value.trim().eq_ignore_ascii_case(sha256))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[derive(Default)]
    struct Fake {
        commands: HashMap<String, String>,
    }

    impl System for Fake {
        fn read(&self, _: &Path) -> Option<String> {
            None
        }
        fn exists(&self, _: &Path) -> bool {
            false
        }
        fn list(&self, _: &Path) -> Vec<String> {
            Vec::new()
        }
        fn link(&self, _: &Path) -> Option<PathBuf> {
            None
        }
        fn run(&self, command: &[&str]) -> Option<(bool, String)> {
            self.commands
                .get(&command.join(" "))
                .map(|text| (true, text.clone()))
        }
        fn resolve(&self, _: &str) -> Result<Vec<String>, String> {
            Err("offline".into())
        }
        fn space(&self, _: &Path) -> Option<(u64, u64)> {
            None
        }
        fn owner(&self, _: &Path) -> Option<u32> {
            None
        }
        fn is_link(&self, _: &Path) -> bool {
            false
        }
        fn canonical(&self, _: &Path) -> Option<PathBuf> {
            None
        }
        fn user(&self) -> (u32, u32, PathBuf) {
            (1000, 1000, PathBuf::from("/home/me"))
        }
        fn on_path(&self, _: &str) -> bool {
            true
        }
        fn processes(&self) -> Vec<String> {
            Vec::new()
        }
    }

    fn platform(manager: PackageManager, id: &str) -> LinuxInfo {
        LinuxInfo {
            id: id.into(),
            pretty_name: format!("{id} Linux"),
            id_like: vec![],
            manager: Some(manager),
        }
    }

    /// The smallest detached signature the checks accept: one signature
    /// packet, new format, one byte long.
    const SIGNATURE: &[u8] = &[0xc2, 0x01, 0x04];

    #[test]
    fn only_bare_signature_packets_count_as_a_detached_signature() {
        assert!(detached_signature(SIGNATURE));
        // Old format, tag 2, one-byte length.
        assert!(detached_signature(&[0x88, 0x01, 0x04]));
        // Two signatures in a row.
        assert!(detached_signature(&[0xc2, 0x01, 0x04, 0x88, 0x01, 0x04]));
        // A literal data packet (tag 11), a one-pass signature (tag 4), a
        // compressed packet (tag 8), armour, a short or empty file: no.
        for bad in [
            &[0xcb, 0x01, 0x00][..],
            &[0xc4, 0x01, 0x00],
            &[0xc8, 0x01, 0x00],
            b"-----BEGIN PGP SIGNATURE-----",
            &[0xc2, 0x05, 0x04],
            &[],
            &[0xc2, 0x01, 0x04, 0xcb, 0x01, 0x00],
        ] {
            assert!(!detached_signature(bad), "{bad:?}");
        }
    }

    /// Downloads served from a map of URL to contents.
    fn served(files: HashMap<String, Vec<u8>>) -> impl Fn(&str, &Path) -> bool {
        move |url: &str, to: &Path| match files.get(url) {
            Some(bytes) => std::fs::write(to, bytes).is_ok(),
            None => false,
        }
    }

    #[test]
    fn an_arch_bundle_holds_packages_and_signatures_and_installs_offline() {
        let catalog = Catalog::builtin(false).unwrap();
        let arch = platform(PackageManager::Arch, "arch");
        let scratch = tempfile::tempdir().unwrap();
        let scratch_cache = scratch.path().join("cache");
        let mut fake = Fake::default();
        fake.commands.insert(
            format!("pacman -Sp --cachedir {} --print-format %f %l htop", scratch_cache.display()),
            "htop-3.5.3-1-x86_64.pkg.tar.zst https://mirror.example/extra/os/x86_64/htop-3.5.3-1-x86_64.pkg.tar.zst\n".into(),
        );
        let ids = vec![
            "htop".to_string(),
            "steam".to_string(),
            "heroic".to_string(),
        ];
        let found = contents(&ids, &catalog, &arch, false, &fake, scratch.path()).unwrap();
        assert_eq!(found.downloads.len(), 2, "the package and its signature");
        assert!(found.entries[0].offline);
        assert!(!found.entries[1].offline);
        assert!(
            found.entries[1]
                .network
                .as_ref()
                .unwrap()
                .contains("script")
        );
        let url = "https://mirror.example/extra/os/x86_64/htop-3.5.3-1-x86_64.pkg.tar.zst";
        let download = served(HashMap::from([
            (url.to_string(), b"package".to_vec()),
            (format!("{url}.sig"), SIGNATURE.to_vec()),
        ]));
        let tools = Tools {
            system: &fake,
            download: &download,
            dnf_download: &|_, _, _| false,
        };
        let program = scratch.path().join("program");
        std::fs::write(&program, b"helixforge").unwrap();
        let bundle = scratch.path().join("bundle");
        let made = create(
            &bundle,
            &found,
            &arch,
            false,
            Some((&program, "1.5.0", true)),
            1,
            &tools,
        )
        .unwrap();
        assert_eq!(made.files.len(), 3);
        assert!(bundle.join(SUMS).is_file());
        // A second bundle into the same folder is refused.
        assert!(matches!(
            create(&bundle, &found, &arch, false, None, 1, &tools),
            Err(BundleError::NotEmpty(_))
        ));
        let read = verify(&bundle, &arch).unwrap();
        assert_eq!(read, made);
        let planned = plan_install(&bundle, &read, &[], &arch, &fake, false).unwrap();
        assert_eq!(planned.steps.len(), 1);
        let shown = planned.steps[0].display();
        assert!(shown.contains("pacman-key --verify"), "{shown}");
        assert!(
            shown.contains("sudo pacman -U --needed htop-3.5.3-1-x86_64.pkg.tar.zst"),
            "{shown}"
        );
        let Step::Shell { program, .. } = &planned.steps[0] else {
            panic!("a shell step");
        };
        // The checksums are the verified manifest's, written into the plan.
        assert!(program.contains(&read.files[0].sha256), "{program}");
        assert!(program.contains("sha256sum --quiet --check -"), "{program}");
        assert!(!program.contains("SHA256SUMS"), "{program}");
        assert!(program.contains("pacman-key --verify"), "{program}");
        assert!(
            planned
                .notes
                .iter()
                .any(|note| note.contains("Steam") && note.contains("network"))
        );
        // Naming only some of the bundle's offline entries is refused.
        assert!(matches!(
            plan_install(&bundle, &read, &["steam".to_string()], &arch, &fake, false),
            Err(BundleError::Partial(_))
        ));
        // Another distribution, or a damaged file, is refused.
        assert!(matches!(
            verify(&bundle, &platform(PackageManager::Debian, "debian")),
            Err(BundleError::Machine(..))
        ));
        std::fs::write(
            bundle.join("packages/htop-3.5.3-1-x86_64.pkg.tar.zst"),
            b"tampered",
        )
        .unwrap();
        assert!(matches!(
            verify(&bundle, &arch),
            Err(BundleError::Damaged(_))
        ));
    }

    /// A bundle of one Arch package, made with stand-in downloads.
    fn arch_bundle(at: &Path) -> (Manifest, LinuxInfo, Fake) {
        let catalog = Catalog::builtin(false).unwrap();
        let arch = platform(PackageManager::Arch, "arch");
        let scratch_cache = at.join("cache");
        let mut fake = Fake::default();
        fake.commands.insert(
            format!(
                "pacman -Sp --cachedir {} --print-format %f %l htop",
                scratch_cache.display()
            ),
            "htop-1-1-x86_64.pkg.tar.zst https://mirror.example/htop-1-1-x86_64.pkg.tar.zst\n"
                .into(),
        );
        let found = contents(&["htop".to_string()], &catalog, &arch, false, &fake, at).unwrap();
        let download = served(HashMap::from([
            (
                "https://mirror.example/htop-1-1-x86_64.pkg.tar.zst".to_string(),
                b"package".to_vec(),
            ),
            (
                "https://mirror.example/htop-1-1-x86_64.pkg.tar.zst.sig".to_string(),
                SIGNATURE.to_vec(),
            ),
        ]));
        let tools = Tools {
            system: &fake,
            download: &download,
            dnf_download: &|_, _, _| false,
        };
        let made = create(&at.join("bundle"), &found, &arch, false, None, 1, &tools).unwrap();
        (made, arch, fake)
    }

    #[test]
    fn a_file_changed_after_the_check_is_never_installed() {
        let scratch = tempfile::tempdir().unwrap();
        let (_, arch, fake) = arch_bundle(scratch.path());
        let bundle = scratch.path().join("bundle");
        let manifest = verify(&bundle, &arch).unwrap();
        let planned = plan_install(&bundle, &manifest, &[], &arch, &fake, false).unwrap();
        let Step::Shell { program, .. } = &planned.steps[0] else {
            panic!("a shell step");
        };
        // Stand-ins: sudo runs its command, pacman-key accepts, pacman records.
        let bin = scratch.path().join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let log = scratch.path().join("pacman.log");
        for (tool, body) in [
            ("sudo", "exec \"$@\"".to_string()),
            ("pacman-key", "exit 0".to_string()),
            ("pacman", format!("echo \"$@\" >> {}", log.display())),
        ] {
            let path = bin.join(tool);
            std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
            std::fs::set_permissions(&path, std::os::unix::fs::PermissionsExt::from_mode(0o755))
                .unwrap();
        }
        let run = || {
            std::process::Command::new("sh")
                .arg("-c")
                .arg(program)
                .env(
                    "PATH",
                    format!("{}:{}", bin.display(), std::env::var("PATH").unwrap()),
                )
                .output()
                .unwrap()
        };
        let output = run();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            std::fs::read_to_string(&log)
                .unwrap()
                .contains("-U --needed")
        );
        std::fs::remove_file(&log).unwrap();
        // Changed after the plan was made, the bundle's own sums along with it.
        std::fs::write(bundle.join("packages/htop-1-1-x86_64.pkg.tar.zst"), b"evil").unwrap();
        std::fs::write(bundle.join(SUMS), "").unwrap();
        let output = run();
        assert!(!output.status.success());
        assert!(
            String::from_utf8_lossy(&output.stderr)
                .contains("the bundle changed after it was checked")
        );
        assert!(!log.exists(), "pacman never ran");
    }

    #[test]
    fn unsigned_or_unprintable_manifests_are_refused() {
        let scratch = tempfile::tempdir().unwrap();
        let (made, arch, _) = arch_bundle(scratch.path());
        let bundle = scratch.path().join("bundle");
        let write = |manifest: &Manifest| {
            std::fs::write(bundle.join(MANIFEST), toml::to_string(manifest).unwrap()).unwrap()
        };
        let mut unsigned = made.clone();
        unsigned.files.retain(|file| !file.path.ends_with(".sig"));
        write(&unsigned);
        let error = verify(&bundle, &arch).unwrap_err().to_string();
        assert!(error.contains("no signature"), "{error}");
        let mut evil = made.clone();
        evil.entries[0].network = Some("\u{1b}]52;c;x\u{7}".into());
        write(&evil);
        let error = verify(&bundle, &arch).unwrap_err().to_string();
        assert!(error.contains("control characters"), "{error}");
        assert!(!error.chars().any(char::is_control));
        write(&made);
        assert!(verify(&bundle, &arch).is_ok());
    }

    #[test]
    fn debian_downloads_are_checked_against_the_repository_index() {
        let catalog = Catalog::builtin(false).unwrap();
        let debian = platform(PackageManager::Debian, "debian");
        let good = hex(&Sha256::digest(b"deb"));
        let mut fake = Fake::default();
        fake.commands.insert(
            "apt-get --print-uris -qq --reinstall install htop".into(),
            format!("'http://deb.example/pool/h/htop_3.2_amd64.deb' htop_3.2_amd64.deb 3 SHA256:{good}\n"),
        );
        let scratch = tempfile::tempdir().unwrap();
        let found = contents(
            &["htop".to_string()],
            &catalog,
            &debian,
            false,
            &fake,
            scratch.path(),
        )
        .unwrap();
        assert_eq!(
            found.downloads[0].checksum.as_deref(),
            Some(format!("sha256:{good}").as_str())
        );
        let honest = served(HashMap::from([(
            "http://deb.example/pool/h/htop_3.2_amd64.deb".to_string(),
            b"deb".to_vec(),
        )]));
        let tools = Tools {
            system: &fake,
            download: &honest,
            dnf_download: &|_, _, _| false,
        };
        let bundle = scratch.path().join("good");
        create(&bundle, &found, &debian, false, None, 1, &tools).unwrap();
        let manifest = verify(&bundle, &debian).unwrap();
        // Unknown to this machine's index: refused, unless the bundle is trusted.
        let refused = plan_install(&bundle, &manifest, &[], &debian, &fake, false).unwrap_err();
        assert!(refused.to_string().contains("--trust-bundle"), "{refused}");
        let planned = plan_install(&bundle, &manifest, &[], &debian, &fake, true).unwrap();
        assert!(
            planned.steps[0]
                .display()
                .contains("sudo apt-get install htop_3.2_amd64.deb")
        );
        // Known to the index with the same checksum: accepted.
        let file = std::fs::canonicalize(bundle.join("packages/htop_3.2_amd64.deb")).unwrap();
        fake.commands.insert(
            format!("dpkg-deb --field {} Package Version", file.display()),
            "Package: htop\nVersion: 3.2\n".into(),
        );
        fake.commands.insert(
            "apt-cache show htop=3.2".into(),
            format!("Package: htop\nVersion: 3.2\nSHA256: {good}\n"),
        );
        assert!(plan_install(&bundle, &manifest, &[], &debian, &fake, false).is_ok());
        // A download that differs from the signed index is deleted, not kept.
        let lying = served(HashMap::from([(
            "http://deb.example/pool/h/htop_3.2_amd64.deb".to_string(),
            b"evil".to_vec(),
        )]));
        let tools = Tools {
            system: &fake,
            download: &lying,
            dnf_download: &|_, _, _| false,
        };
        let bad = scratch.path().join("bad");
        assert!(matches!(
            create(&bad, &found, &debian, false, None, 1, &tools),
            Err(BundleError::Mismatch(_))
        ));
        assert!(!bad.join("packages/htop_3.2_amd64.deb").exists());
    }

    #[test]
    fn manifests_cannot_reach_outside_the_bundle() {
        let arch = platform(PackageManager::Arch, "arch");
        let directory = tempfile::tempdir().unwrap();
        let manifest = Manifest {
            format: FORMAT,
            family: "arch".into(),
            architecture: std::env::consts::ARCH.into(),
            files: vec![BundledFile {
                path: "../../etc/passwd".into(),
                ..BundledFile::default()
            }],
            ..Manifest::default()
        };
        std::fs::write(
            directory.path().join(MANIFEST),
            toml::to_string(&manifest).unwrap(),
        )
        .unwrap();
        assert!(matches!(
            verify(directory.path(), &arch),
            Err(BundleError::Damaged(_))
        ));
        assert!(!plain_file("../x"));
        assert!(!plain_file("a b"));
        assert!(plain_file("libstdc++-16.2.1+r23-1-x86_64.pkg.tar.zst"));
        assert!(plain_file("zlib-1:1.3.2-4-x86_64.pkg.tar.zst"));
        assert!(matches!(
            contents(
                &[],
                &Catalog::builtin(false).unwrap(),
                &platform(PackageManager::Suse, "opensuse"),
                false,
                &Fake::default(),
                directory.path()
            ),
            Err(BundleError::Unsupported(_))
        ));
    }
}
