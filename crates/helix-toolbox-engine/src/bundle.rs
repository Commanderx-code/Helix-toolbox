//! Offline installation bundles: a folder holding Helix Toolbox itself (with
//! its catalog inside), the packages chosen entries need for this machine's
//! distribution and architecture, and a manifest of every file's checksum.
//! On a machine of the same distribution without reliable internet, the
//! bundle is checked, then its packages are installed by the package manager
//! from the files, without downloading. Flatpak applications installed here
//! go in as `flatpak create-usb` copies them, with their runtimes, and are
//! installed from that copy, their remote's signatures checked as from the
//! remote itself. What needs the network (AUR builds, scripts, Flatpaks not
//! installed here) is named, never pretended to be offline.

use crate::{Plan, Step, doctor::System, quote};
use helix_toolbox_catalog::Catalog;
use helix_toolbox_core::{Action, Entry, PackageManager, Risk, printable, valid_package_name};
use helix_toolbox_platform::LinuxInfo;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256, Sha512};
use std::path::{Component, Path, PathBuf};

/// The newest manifest format this version reads and writes: 2 holds
/// Flatpaks, and bundles without them are still written as 1.
pub const FORMAT: u32 = 2;
/// The folder `flatpak create-usb` writes its repository into.
pub const FLATPAKS: &str = "flatpak";
/// The folder for the Flatpak remotes' signing keys.
pub const KEYS: &str = "flatpak-keys";
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
    /// Whether the Helix Toolbox in it is a release build.
    #[serde(default)]
    pub release: bool,
    pub distribution: String,
    /// arch, debian, fedora or opensuse.
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
    #[serde(default, rename = "flatpak", skip_serializing_if = "Vec::is_empty")]
    pub flatpaks: Vec<BundledFlatpak>,
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
    /// The Flatpak application it installs from the bundle, instead.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flatpak: Option<String>,
}

/// A Flatpak application in the bundle, as `flatpak create-usb` copied it
/// from this machine's installation with the runtimes it needs.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct BundledFlatpak {
    /// The application's ID.
    pub id: String,
    /// Its ref: `app/ID/ARCH/BRANCH`.
    pub reference: String,
    /// The commit installed here, which the bundle's copy must be.
    pub commit: String,
    /// The remote it came from, the remote's address and collection ID.
    pub remote: String,
    pub url: String,
    pub collection: String,
    /// In the user's own installation rather than the system's.
    #[serde(default)]
    pub user: bool,
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
    /// So does openSUSE's zypper, into a scratch root of its own.
    pub zypper: Vec<String>,
    /// Flatpak applications installed here, to copy.
    pub flatpaks: Vec<BundledFlatpak>,
}

/// Why a bundle cannot be made, read or used.
#[derive(Debug, thiserror::Error)]
pub enum BundleError {
    #[error("bundles are made with pacman, apt, dnf or zypper; {0} is not supported yet")]
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
    #[error("{0} is not a Helix Toolbox bundle: {1}")]
    Format(PathBuf, String),
    #[error("the bundle was made for {0}; this machine is {1}")]
    Machine(String, String),
    #[error("the bundle is damaged: {0}")]
    Damaged(String),
    #[error(
        "the bundle installs as a whole: its packages, shared between {0}, go in together; make a bundle of only the entries wanted"
    )]
    Partial(String),
    #[error(
        "the {0} Flatpak remote has no collection ID, which flatpak create-usb needs to copy its applications; {1}"
    )]
    Collection(String, String),
    #[error("{0}")]
    Untrusted(String),
}

/// The family a bundle is made for.
fn family(manager: Option<PackageManager>) -> Result<(PackageManager, &'static str), BundleError> {
    match manager {
        Some(PackageManager::Arch) => Ok((PackageManager::Arch, "arch")),
        Some(PackageManager::Debian) => Ok((PackageManager::Debian, "debian")),
        Some(PackageManager::Fedora) => Ok((PackageManager::Fedora, "fedora")),
        Some(PackageManager::Suse) => Ok((PackageManager::Suse, "opensuse")),
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
    let mut installed: Option<Vec<BundledFlatpak>> = None;
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
                flatpak: None,
            });
            continue;
        };
        if let Action::Profile { entries } = &entry.action {
            queue.extend(entries.iter().rev().cloned());
            continue;
        }
        let (offline, mut network) = classify(entry, manager);
        if offline.is_none()
            && let Action::Package {
                flatpak: Some(id), ..
            } = &entry.action
        {
            let installed = installed.get_or_insert_with(|| installed_flatpaks(system));
            if let Some(flatpak) = installed.iter().find(|flatpak| flatpak.id == *id) {
                if flatpak.collection.is_empty() {
                    return Err(BundleError::Collection(
                        printable(&flatpak.remote),
                        collection_hint(&flatpak.remote, flatpak.user),
                    ));
                }
                found.entries.push(BundledEntry {
                    id: entry.id.clone(),
                    name: entry.name.clone(),
                    offline: true,
                    packages: Vec::new(),
                    network: None,
                    flatpak: Some(id.clone()),
                });
                if !found
                    .flatpaks
                    .iter()
                    .any(|other| other.reference == flatpak.reference)
                {
                    found.flatpaks.push(flatpak.clone());
                }
                continue;
            }
            network =
                network.map(|why| format!("{why}; its Flatpak can be bundled once installed here"));
        }
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
            flatpak: None,
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
        PackageManager::Suse => found.zypper = names,
        _ => found.dnf = names,
    }
    for download in &found.downloads {
        if !plain_file(&download.file) {
            return Err(BundleError::Resolve(format!(
                "the package manager named a file Helix Toolbox does not accept: {}",
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

/// The Flatpak applications installed here, in the system installation or
/// the user's, with their remotes; `collection` is empty when the remote
/// has none.
fn installed_flatpaks(system: &dyn System) -> Vec<BundledFlatpak> {
    let Some((true, text)) = system.run(&[
        "flatpak",
        "list",
        "--app",
        "--columns=application,ref,origin,installation",
    ]) else {
        return Vec::new();
    };
    let mut remotes: Vec<(bool, Vec<Remote>)> = Vec::new();
    let mut found = Vec::new();
    for line in text.lines() {
        let parts: Vec<&str> = line.trim_end().split('\t').collect();
        let [id, reference, remote, installation] = parts[..] else {
            continue;
        };
        let user = match installation {
            "user" => true,
            "system" => false,
            _ => continue,
        };
        let reference = format!("app/{reference}");
        if !plain_flatpak_ref(&reference, id) || !flatpak_name(remote) {
            continue;
        }
        if !remotes.iter().any(|(scope, _)| *scope == user) {
            remotes.push((user, flatpak_remotes(system, user)));
        }
        let Some((_, url, collection)) = remotes
            .iter()
            .find(|(scope, _)| *scope == user)
            .and_then(|(_, known)| known.iter().find(|(name, _, _)| name == remote))
        else {
            continue;
        };
        let Some((true, commit)) =
            system.run(&["flatpak", "info", scope(user), "--show-commit", &reference])
        else {
            continue;
        };
        found.push(BundledFlatpak {
            id: id.to_string(),
            reference,
            commit: commit.trim().to_string(),
            remote: remote.to_string(),
            url: url.clone(),
            collection: collection.clone(),
            user,
        });
    }
    found
}

/// A Flatpak remote: name, address and collection ID (empty when it has
/// none).
type Remote = (String, String, String);

/// A machine's Flatpak remotes.
fn flatpak_remotes(system: &dyn System, user: bool) -> Vec<Remote> {
    let Some((true, text)) = system.run(&[
        "flatpak",
        "remotes",
        scope(user),
        "--columns=name,url,collection",
    ]) else {
        return Vec::new();
    };
    text.lines()
        .filter_map(|line| {
            let parts: Vec<&str> = line.trim_end().split('\t').collect();
            let [name, url, collection] = parts[..] else {
                return None;
            };
            let collection = if collection == "-" { "" } else { collection };
            Some((name.to_string(), url.to_string(), collection.to_string()))
        })
        .collect()
}

fn scope(user: bool) -> &'static str {
    if user { "--user" } else { "--system" }
}

/// The collection IDs publishers give their remotes, by address: the only
/// ones a bundle may give a remote that has none, unless it is trusted.
const PUBLISHED_COLLECTIONS: [(&str, &str); 2] = [
    ("https://dl.flathub.org/repo/", "org.flathub.Stable"),
    ("https://dl.flathub.org/beta-repo/", "org.flathub.Beta"),
];

fn published_collection(address: &str) -> Option<&'static str> {
    let address = address.trim_end_matches('/');
    PUBLISHED_COLLECTIONS
        .iter()
        .find(|(published, _)| published.trim_end_matches('/') == address)
        .map(|(_, collection)| *collection)
}

/// How to give a remote its collection ID.
fn collection_hint(remote: &str, user: bool) -> String {
    let command = if user {
        "flatpak --user remote-modify"
    } else {
        "sudo flatpak remote-modify"
    };
    match remote {
        "flathub" => format!(
            "Flathub's is org.flathub.Stable: {command} --collection-id=org.flathub.Stable flathub"
        ),
        "flathub-beta" => format!(
            "Flathub Beta's is org.flathub.Beta: {command} --collection-id=org.flathub.Beta flathub-beta"
        ),
        _ => format!(
            "set the one its publisher gives with {command} --collection-id=ID {}",
            printable(remote)
        ),
    }
}

/// A Flatpak ID, remote, collection, architecture or branch: plain characters.
fn flatpak_name(text: &str) -> bool {
    !text.is_empty()
        && text.len() <= 255
        && !text.starts_with(['.', '-'])
        && text
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
}

/// `app/ID/ARCH/BRANCH`, for this ID.
fn plain_flatpak_ref(reference: &str, id: &str) -> bool {
    let parts: Vec<&str> = reference.split('/').collect();
    parts.len() == 4
        && parts[0] == "app"
        && parts[1] == id
        && parts.iter().all(|part| flatpak_name(part))
}

/// Where a remote's key goes in the bundle.
fn key_path(remote: &str, user: bool) -> String {
    format!("{KEYS}/{}{remote}.gpg", if user { "user-" } else { "" })
}

/// The commit a Flatpak repository holds for a ref, where `create-usb`
/// writes it: under the remote's collection ID.
fn mirrored_commit(repository: &Path, flatpak: &BundledFlatpak) -> Option<String> {
    let path = repository
        .join("refs/mirrors")
        .join(&flatpak.collection)
        .join(&flatpak.reference);
    std::fs::read_to_string(path)
        .ok()
        .map(|text| text.trim().to_string())
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

/// Whether a file in a bundle's packages/ folder is named as this family's
/// packages are: an Arch package or its detached signature, a Debian
/// package, an RPM. Nothing else is handed to the package manager.
fn package_file(family: &str, name: &str) -> bool {
    match family {
        "arch" => {
            let package = name.strip_suffix(".sig").unwrap_or(name);
            package
                .rsplit_once(".pkg.tar")
                .is_some_and(|(stem, compression)| {
                    !stem.is_empty()
                        && matches!(
                            compression,
                            "" | ".zst" | ".xz" | ".gz" | ".bz2" | ".lz4" | ".lzo" | ".lrz" | ".Z"
                        )
                })
        }
        "debian" => name.len() > ".deb".len() && name.ends_with(".deb"),
        "fedora" | "opensuse" => {
            name.len() > ".rpm".len() && name.ends_with(".rpm") && !name.ends_with(".src.rpm")
        }
        _ => false,
    }
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

/// A package manager downloading packages, with their dependencies (all
/// of them, when asked), into a folder.
type Downloader<'a> = dyn Fn(&[String], bool, &Path) -> bool + 'a;

/// What making a bundle asks the machine; tests answer instead.
pub struct Tools<'a> {
    pub system: &'a dyn System,
    /// Download a URL to a file; false when it failed.
    pub download: &'a dyn Fn(&str, &Path) -> bool,
    /// Have dnf download packages with their dependencies into a folder.
    pub dnf_download: &'a dyn Fn(&[String], bool, &Path) -> bool,
    /// The same with zypper, which needs no root for it.
    pub zypper_download: &'a dyn Fn(&[String], bool, &Path) -> bool,
    /// Have `flatpak create-usb` copy refs, with the runtimes they need,
    /// from the system installation (or the user's) into the bundle's
    /// FLATPAKS folder.
    pub create_usb: &'a dyn Fn(&[String], bool, &Path) -> bool,
    /// Where a remote's signing key is kept, in the system installation or
    /// the user's.
    pub flatpak_key: &'a dyn Fn(&str, bool) -> PathBuf,
}

/// A new, private scratch folder for pacman's empty database, which
/// `contents` takes: made here and now, never one that already existed, so
/// nobody else's files are used.
pub fn scratch() -> std::io::Result<PathBuf> {
    use std::os::unix::fs::DirBuilderExt;
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.subsec_nanos())
        .unwrap_or(0);
    let base = std::env::temp_dir().join(format!(
        "helix-toolbox-bundle-{}-{nanos}",
        std::process::id()
    ));
    std::fs::DirBuilder::new().mode(0o700).create(&base)?;
    Ok(base)
}

/// A plan that makes a bundle in `directory` with the command line at
/// `program` (`helix-toolbox bundle create`), whose downloads show in the
/// run's terminal. It installs nothing here. `directory` is absolute, so it
/// is never read as an option.
pub fn plan_create(program: &Path, directory: &Path, ids: &[String], complete: bool) -> Plan {
    let mut words = vec![
        program.display().to_string(),
        "bundle".to_string(),
        "create".to_string(),
        directory.display().to_string(),
    ];
    words.extend(ids.iter().cloned());
    if complete {
        words.push("--complete".to_string());
    }
    Plan {
        steps: vec![Step::Shell {
            id: "bundle-create".to_string(),
            name: format!(
                "Make a bundle in {}",
                printable(&directory.display().to_string())
            ),
            program: words
                .iter()
                .map(|word| quote(word))
                .collect::<Vec<_>>()
                .join(" "),
            display: printable(&words.join(" ")),
            requires: Vec::new(),
            changes: Vec::new(),
        }],
        notes: vec![
            "It downloads the packages into the folder and installs nothing on this machine."
                .to_string(),
        ],
        risk: Risk::Low,
        depends: vec![Vec::new()],
        ..Plan::default()
    }
}

/// Make a bundle in `directory`, which must be new or empty: the packages,
/// a copy of `helix-toolbox`, the manifest and the checksums.
#[allow(clippy::too_many_arguments)]
pub fn create(
    directory: &Path,
    contents: &Contents,
    platform: &LinuxInfo,
    complete: bool,
    program_copy: Option<(&Path, &str, bool)>,
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
    let downloaders: [(&[String], &Downloader, &str); 2] = [
        (&contents.dnf, tools.dnf_download, "dnf download"),
        (
            &contents.zypper,
            tools.zypper_download,
            "zypper install --download-only",
        ),
    ];
    for (names, download, label) in downloaders {
        if names.is_empty() {
            continue;
        }
        if !download(names, complete, &packages) {
            return Err(BundleError::Download(format!(
                "the packages, through {label}"
            )));
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
                source: Some(label.to_string()),
                assurance: Some(
                    "signed: its signature is checked against the installing machine's rpm keyring before it installs"
                        .to_string(),
                ),
            });
        }
    }
    for user in [false, true] {
        let refs: Vec<String> = contents
            .flatpaks
            .iter()
            .filter(|flatpak| flatpak.user == user)
            .map(|flatpak| flatpak.reference.clone())
            .collect();
        if !refs.is_empty() && !(tools.create_usb)(&refs, user, directory) {
            return Err(BundleError::Download(
                "the Flatpaks, through flatpak create-usb".to_string(),
            ));
        }
    }
    let repository = directory.join(FLATPAKS);
    for flatpak in &contents.flatpaks {
        if mirrored_commit(&repository, flatpak).as_deref() != Some(flatpak.commit.as_str()) {
            return Err(BundleError::Mismatch(format!(
                "{}, as flatpak create-usb copied it,",
                printable(&flatpak.reference)
            )));
        }
    }
    // The remotes' signing keys, for a machine that lacks the remote.
    let mut keys: Vec<String> = Vec::new();
    for flatpak in &contents.flatpaks {
        let path = key_path(&flatpak.remote, flatpak.user);
        if keys.contains(&path) {
            continue;
        }
        let source = (tools.flatpak_key)(&flatpak.remote, flatpak.user);
        let target = directory.join(&path);
        std::fs::create_dir_all(directory.join(KEYS))
            .map_err(|error| BundleError::Io(error.to_string()))?;
        std::fs::copy(&source, &target).map_err(|error| {
            BundleError::Io(format!("cannot copy {}: {error}", source.display()))
        })?;
        let (sha256, bytes) = sha256_file(&target)?;
        files.push(BundledFile {
            path: path.clone(),
            sha256,
            bytes,
            source: Some(printable(&source.display().to_string())),
            assurance: Some(format!(
                "the {} remote's signing key, from the machine that made the bundle; used only where the remote is missing, with --trust-bundle",
                printable(&flatpak.remote)
            )),
        });
        keys.push(path);
    }
    let (version, release) = match program_copy {
        Some((program, version, release)) => {
            let target = directory.join("helix-toolbox");
            std::fs::copy(program, &target).map_err(|error| BundleError::Io(error.to_string()))?;
            let (sha256, bytes) = sha256_file(&target)?;
            files.push(BundledFile {
                path: "helix-toolbox".to_string(),
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
        format: if contents.flatpaks.is_empty() {
            1
        } else {
            FORMAT
        },
        created: now,
        helixforge: version,
        release,
        distribution: printable(&platform.pretty_name),
        family: family_name.to_string(),
        architecture: std::env::consts::ARCH.to_string(),
        complete,
        entries: contents.entries.clone(),
        files,
        flatpaks: contents.flatpaks.clone(),
    };
    let text = toml::to_string(&manifest).map_err(|error| BundleError::Io(error.to_string()))?;
    std::fs::write(
        directory.join(MANIFEST),
        format!(
            "# A Helix Toolbox offline bundle: ./helix-toolbox bundle install <this folder>\n{text}"
        ),
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
            format!("format {} is from a later Helix Toolbox", manifest.format),
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
        // Bundles made before 1.7 carry the program as `helixforge`.
        let place = file.path == "helix-toolbox"
            || file.path == "helixforge"
            || file.path.starts_with("packages/")
            || file.path.starts_with(&format!("{KEYS}/"));
        if !inside || !name_ok || !place {
            return Err(BundleError::Damaged(format!(
                "it names {}",
                printable(&file.path)
            )));
        }
        // Everything under packages/ goes to the package manager, so it
        // holds only what this family's packages are named.
        if file.path.starts_with("packages/")
            && !relative
                .file_name()
                .is_some_and(|name| package_file(family_name, &name.to_string_lossy()))
        {
            return Err(BundleError::Damaged(format!(
                "{} is not a {family_name} package",
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
    let repository = directory.join(FLATPAKS);
    for flatpak in &manifest.flatpaks {
        if mirrored_commit(&repository, flatpak).as_deref() != Some(flatpak.commit.as_str()) {
            return Err(BundleError::Damaged(format!(
                "its Flatpak repository does not hold {} at the commit it records",
                printable(&flatpak.reference)
            )));
        }
        if !manifest
            .files
            .iter()
            .any(|file| file.path == key_path(&flatpak.remote, flatpak.user))
        {
            return Err(BundleError::Damaged(format!(
                "the {} remote's key is missing",
                printable(&flatpak.remote)
            )));
        }
    }
    for entry in &manifest.entries {
        if !helix_toolbox_core::valid_id(&entry.id)
            || !entry.packages.iter().all(|name| valid_package_name(name))
            || entry
                .flatpak
                .as_deref()
                .is_some_and(|id| !manifest.flatpaks.iter().any(|flatpak| flatpak.id == id))
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
        texts.extend(entry.flatpak.as_deref());
        texts.extend(entry.packages.iter().map(String::as_str));
    }
    for file in &manifest.files {
        texts.push(&file.path);
        texts.extend(file.source.as_deref());
        texts.extend(file.assurance.as_deref());
    }
    let flatpaks = manifest.flatpaks.iter().all(|flatpak| {
        plain_flatpak_ref(&flatpak.reference, &flatpak.id)
            && flatpak_name(&flatpak.remote)
            && flatpak_name(&flatpak.collection)
            && flatpak.commit.len() == 64
            && flatpak.commit.bytes().all(|byte| byte.is_ascii_hexdigit())
            && plain(&flatpak.url)
            && !flatpak.url.contains(char::is_whitespace)
    });
    texts.into_iter().all(plain)
        && flatpaks
        && manifest.flatpaks.len() <= 1_000
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
/// nothing changed in the bundle afterward is installed. Then each
/// package's signature is checked (pacman's with pacman-key; an RPM's must
/// be by a key in this machine's rpm keyring), and the package manager
/// installs them from the files.
///
/// Debian packages carry no signature of their own: each must be known to
/// this machine's repository index (signed, and checked by apt) with the
/// same checksum. `trust` takes the bundle's word for RPMs without such a
/// signature and Debian packages the index does not know.
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
    let mut requirements: Vec<String> = Vec::new();
    if !packages.is_empty() {
        let (step, requires) =
            native_step(&directory, manager, &packages, &offline, system, trust)?;
        plan.steps.push(step);
        requirements.extend(requires);
    }
    if !manifest.flatpaks.is_empty() {
        let (step, requires, privileged) = flatpak_step(&directory, manifest, system, trust)?;
        plan.steps.push(step);
        requirements.extend(requires);
        plan.privileged |= privileged;
    }
    requirements.sort();
    requirements.dedup();
    plan.requirements = requirements;
    plan.privileged |= !packages.is_empty();
    plan.risk = Risk::Medium;
    plan.depends = vec![Vec::new(); plan.steps.len()];
    Ok(plan)
}

/// The step that installs a bundle's packages from its files.
fn native_step(
    directory: &Path,
    manager: PackageManager,
    packages: &[&BundledFile],
    offline: &[&&BundledEntry],
    system: &dyn System,
    trust: bool,
) -> Result<(Step, Vec<String>), BundleError> {
    let name = |file: &BundledFile| {
        Path::new(&file.path)
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default()
    };
    // What the package manager is handed: every package file but Arch's
    // signatures, which pacman-key checks.
    let installable: Vec<&&BundledFile> = packages
        .iter()
        .filter(|file| !file.path.ends_with(".sig"))
        .collect();
    let rpm = matches!(manager, PackageManager::Fedora | PackageManager::Suse);
    if manager == PackageManager::Debian && !trust {
        let mut unknown = Vec::new();
        for file in &installable {
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
    if rpm && !trust {
        let unsigned: Vec<String> = installable
            .iter()
            .filter(|file| !signed_for_rpm(&directory.join(&file.path), system))
            .map(|file| name(file))
            .collect();
        if !unsigned.is_empty() {
            return Err(BundleError::Damaged(format!(
                "{} carry no signature by a key in this machine's rpm keyring, so nothing but the bundle vouches for them; install with --trust-bundle only if you made the bundle yourself",
                unsigned.join(", ")
            )));
        }
    }
    let staged = |file: &BundledFile| format!("\"$hf_staging\"/{}", quote(&name(file)));
    let mut lines = vec![
        "hf_staging=$(mktemp -d) || exit 1".to_string(),
        "trap 'rm -rf \"$hf_staging\"' EXIT".to_string(),
    ];
    for file in packages {
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
        "( cd \"$hf_staging\" && printf '%s\\n' {} | sha256sum --quiet --check - ) || {{ echo 'Helix Toolbox: the bundle changed after it was checked; nothing was installed' >&2; exit 1; }}",
        sums.join(" ")
    ));
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
        PackageManager::Fedora | PackageManager::Suse => {
            let files: Vec<String> = installable.iter().map(|file| staged(file)).collect();
            if trust {
                // The bundle's word, as asked: its digests still have to hold.
                lines.push(format!("rpm -K {} || exit 1", files.join(" ")));
            } else {
                // rpm -K alone passes a package with no signature at all:
                // each copy must show a signature by a key in the keyring,
                // and no check that failed or could not be made.
                lines.push(format!(
                    "for hf_package in {}; do hf_checked=$(LC_ALL=C {RPM_SIGNATURE} -- \"$hf_package\") && printf '%s\\n' \"$hf_checked\" | grep -Eiq '{RPM_SIGNED}' && ! printf '%s\\n' \"$hf_checked\" | grep -Eq '{RPM_FAILED}' || {{ echo \"Helix Toolbox: ${{hf_package##*/}} is not signed by a key this machine trusts; nothing was installed\" >&2; exit 1; }}; done",
                    files.join(" ")
                ));
                requires.push("grep".to_string());
            }
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
        // The remote repositories set aside, as dnf's are.
        PackageManager::Suse => ("sudo zypper --no-refresh --no-remote install", "zypper"),
        _ if trust => ("sudo dnf install --disablerepo='*'", "dnf"),
        _ => (
            "sudo dnf install --disablerepo='*' --setopt=localpkg_gpgcheck=1",
            "dnf",
        ),
    };
    if manager == PackageManager::Suse {
        // zypper reads its answer only from a terminal; with none attached,
        // the confirmation already given stands, and its defaults refuse
        // anything unsigned.
        lines
            .push("if [ -t 0 ]; then hf_answer=; else hf_answer=--non-interactive; fi".to_string());
        lines.push(format!(
            "sudo zypper $hf_answer --no-refresh --no-remote install {}",
            files.join(" ")
        ));
    } else {
        lines.push(format!("{install} {}", files.join(" ")));
    }
    requires.push(tool.to_string());
    let names: Vec<String> = offline
        .iter()
        .filter(|entry| entry.flatpak.is_none())
        .map(|entry| printable(&entry.name))
        .collect();
    let shown: Vec<String> = installable.iter().map(|file| name(file)).collect();
    let step = Step::Shell {
        id: "bundle-install".to_string(),
        name: format!("Install {} from the bundle", names.join(", ")),
        program: lines.join("\n"),
        display: printable(&format!(
            "copy the packages from {} to a private folder, check them against the verified checksums{}, then {install} {}",
            directory.display(),
            match manager {
                PackageManager::Arch => " and their signatures (pacman-key --verify)",
                PackageManager::Fedora | PackageManager::Suse if trust => {
                    " and their digests (rpm -K)"
                }
                PackageManager::Fedora | PackageManager::Suse => {
                    " and their signatures against this machine's rpm keyring (rpm -Kv)"
                }
                _ => "",
            },
            shown.join(" ")
        )),
        requires: requires.clone(),
        changes: Vec::new(),
    };
    Ok((step, requires))
}

/// The step that installs a bundle's Flatpaks from its copy of their
/// repository. flatpak checks each commit's signature against the remote's
/// key on this machine. A remote this machine lacks is added with the
/// bundle's copy of its key, which only `trust` allows, as nothing here
/// vouches for that key; one without a collection ID is given the bundle's.
fn flatpak_step(
    directory: &Path,
    manifest: &Manifest,
    system: &dyn System,
    trust: bool,
) -> Result<(Step, Vec<String>, bool), BundleError> {
    let repository = quote(&directory.join(FLATPAKS).display().to_string());
    let mut lines: Vec<String> = Vec::new();
    let mut shown: Vec<String> = Vec::new();
    let mut requires = vec!["flatpak".to_string()];
    let mut privileged = false;
    for user in [false, true] {
        let group: Vec<&BundledFlatpak> = manifest
            .flatpaks
            .iter()
            .filter(|flatpak| flatpak.user == user)
            .collect();
        if group.is_empty() {
            continue;
        }
        let sudo = if user { "" } else { "sudo " };
        privileged |= !user;
        let here = flatpak_remotes(system, user);
        let mut remotes: Vec<&BundledFlatpak> = Vec::new();
        for flatpak in &group {
            if !remotes.iter().any(|seen| seen.remote == flatpak.remote) {
                remotes.push(flatpak);
            }
        }
        for flatpak in remotes {
            let remote = &flatpak.remote;
            let collection = &flatpak.collection;
            match here.iter().find(|(name, _, _)| name == remote) {
                None if !trust => {
                    return Err(BundleError::Untrusted(format!(
                        "this machine has no {remote} Flatpak remote, so only the bundle's copy of its key would vouch for the Flatpaks; add the remote first, or install with --trust-bundle only if you made the bundle yourself"
                    )));
                }
                None => {
                    let key = manifest
                        .files
                        .iter()
                        .find(|file| file.path == key_path(remote, user))
                        .ok_or_else(|| {
                            BundleError::Damaged(format!("the {remote} remote's key is missing"))
                        })?;
                    // The checksum verified here, not the bundle's own file.
                    lines.push(format!(
                        "hf_key=$(mktemp) && cp -- {} \"$hf_key\" && printf '%s  %s\\n' {} \"$hf_key\" | sha256sum --quiet --check - || {{ echo 'Helix Toolbox: the bundle changed after it was checked; nothing was installed' >&2; exit 1; }}",
                        quote(&directory.join(&key.path).display().to_string()),
                        key.sha256
                    ));
                    lines.push(format!(
                        "{sudo}flatpak remote-add {} --gpg-import=\"$hf_key\" --collection-id={collection} {remote} {} || exit 1\nrm -f \"$hf_key\"",
                        scope(user),
                        quote(&flatpak.url)
                    ));
                    shown.push(format!(
                        "add the {remote} remote ({}) with the bundle's copy of its key",
                        flatpak.url
                    ));
                    requires.extend(["mktemp".to_string(), "sha256sum".to_string()]);
                }
                Some((_, address, here))
                    if here.is_empty()
                        && !trust
                        && published_collection(address) != Some(collection.as_str()) =>
                {
                    return Err(BundleError::Untrusted(format!(
                        "this machine's {remote} Flatpak remote has no collection ID, and {} is not the one published for {}; give the remote its own, or install with --trust-bundle only if you made the bundle yourself",
                        printable(collection),
                        printable(address)
                    )));
                }
                Some((_, _, here)) if here.is_empty() => {
                    lines.push(format!(
                        "{sudo}flatpak remote-modify {} --collection-id={collection} {remote} || exit 1",
                        scope(user)
                    ));
                    shown.push(format!(
                        "give the {remote} remote its collection ID, {collection}"
                    ));
                }
                Some((_, _, here)) if here != collection => {
                    return Err(BundleError::Untrusted(format!(
                        "this machine's {remote} Flatpak remote has the collection ID {}, and the bundle's Flatpaks {collection}",
                        printable(here)
                    )));
                }
                Some(_) => {}
            }
            let ids: Vec<&str> = group
                .iter()
                .filter(|other| other.remote == *remote)
                .map(|other| other.id.as_str())
                .collect();
            let install = format!(
                "{sudo}flatpak install {} --noninteractive --sideload-repo={repository} {remote} {}",
                scope(user),
                ids.join(" ")
            );
            lines.push(format!("{install} || exit 1"));
            shown.push(install);
        }
    }
    if privileged {
        requires.push("sudo".to_string());
    }
    let names: Vec<String> = manifest
        .entries
        .iter()
        .filter(|entry| entry.flatpak.is_some())
        .map(|entry| printable(&entry.name))
        .collect();
    let step = Step::Shell {
        id: "bundle-flatpak".to_string(),
        name: format!("Install {} from the bundle's Flatpaks", names.join(", ")),
        program: lines.join("\n"),
        display: printable(&format!(
            "{}, from the bundle's copy of the repository; flatpak checks each commit's signature against the remote's key",
            shown.join("; then ")
        )),
        requires: requires.clone(),
        changes: Vec::new(),
    };
    Ok((step, requires, privileged))
}

/// rpm asked to check a package's signatures, not only its digests: with
/// `_pkgverify_level` at `signature`, a package with none fails too.
const RPM_SIGNATURE: &str = "rpm --define '_pkgverify_level signature' -Kv";
/// The line rpm -Kv prints for a signature a key in the keyring verifies,
/// in rpm 4's words ("key ID") and rpm 6's ("key fingerprint").
const RPM_SIGNED: &str = "signature, key (id|fingerprint):? *[0-9a-f]+: OK$";
/// What rpm -Kv prints for a check that failed or could not be made.
const RPM_FAILED: &str = ": (BAD|NOKEY|NOTTRUSTED|NOTFOUND)$";

/// Whether rpm says this package file carries a signature by a key in this
/// machine's rpm keyring, and that no check of it failed.
fn signed_for_rpm(file: &Path, system: &dyn System) -> bool {
    let path = file.display().to_string();
    let Some((true, shown)) = system.run(&[
        "rpm",
        "--define",
        "_pkgverify_level signature",
        "-Kv",
        "--",
        &path,
    ]) else {
        return false;
    };
    rpm_signed(&shown)
}

/// rpm -Kv's report on a package: a signature by a key it holds, and no
/// check bad, missing or untrusted.
fn rpm_signed(shown: &str) -> bool {
    let mut signed = false;
    for line in shown.lines() {
        let Some((check, status)) = line.trim().rsplit_once(": ") else {
            continue;
        };
        match status {
            "OK" => {
                let check = check.to_ascii_lowercase();
                signed |= check.contains("signature, key id")
                    || check.contains("signature, key fingerprint");
            }
            "BAD" | "NOKEY" | "NOTTRUSTED" | "NOTFOUND" => return false,
            _ => {}
        }
    }
    signed
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
        // compressed packet (tag 8), armor, a short or empty file: no.
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
            zypper_download: &|_, _, _| false,
            create_usb: &|_, _, _| false,
            flatpak_key: &|_, _| PathBuf::new(),
        };
        let program = scratch.path().join("program");
        std::fs::write(&program, b"helix-toolbox").unwrap();
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
            zypper_download: &|_, _, _| false,
            create_usb: &|_, _, _| false,
            flatpak_key: &|_, _| PathBuf::new(),
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
            zypper_download: &|_, _, _| false,
            create_usb: &|_, _, _| false,
            flatpak_key: &|_, _| PathBuf::new(),
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
            zypper_download: &|_, _, _| false,
            create_usb: &|_, _, _| false,
            flatpak_key: &|_, _| PathBuf::new(),
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
                &platform(PackageManager::Alpine, "alpine"),
                false,
                &Fake::default(),
                directory.path()
            ),
            Err(BundleError::Unsupported(_))
        ));
    }

    /// What `flatpak` says on a machine with Flycast installed from Flathub.
    fn flatpak_machine(collection: &str) -> Fake {
        let mut fake = Fake::default();
        fake.commands.insert(
            "flatpak list --app --columns=application,ref,origin,installation".into(),
            "org.flycast.Flycast\torg.flycast.Flycast/x86_64/stable\tflathub\tsystem\n".into(),
        );
        fake.commands.insert(
            "flatpak remotes --system --columns=name,url,collection".into(),
            format!("flathub\thttps://dl.flathub.org/repo/\t{collection}\n"),
        );
        fake.commands.insert(
            "flatpak info --system --show-commit app/org.flycast.Flycast/x86_64/stable".into(),
            format!("{}\n", "c".repeat(64)),
        );
        fake
    }

    #[test]
    fn installed_flatpaks_are_copied_and_installed_from_the_copy() {
        let catalog = Catalog::builtin(false).unwrap();
        let arch = platform(PackageManager::Arch, "arch");
        let scratch = tempfile::tempdir().unwrap();
        // Without a collection ID, create-usb cannot copy it: say how to set one.
        let error = contents(
            &["flycast".to_string()],
            &catalog,
            &arch,
            false,
            &flatpak_machine("-"),
            scratch.path(),
        )
        .unwrap_err()
        .to_string();
        assert!(
            error.contains("sudo flatpak remote-modify --collection-id=org.flathub.Stable flathub"),
            "{error}"
        );
        let fake = flatpak_machine("org.flathub.Stable");
        let found = contents(
            &["flycast".to_string(), "zoom".to_string()],
            &catalog,
            &arch,
            false,
            &fake,
            scratch.path(),
        )
        .unwrap();
        assert_eq!(found.flatpaks.len(), 1);
        assert_eq!(
            found.flatpaks[0].reference,
            "app/org.flycast.Flycast/x86_64/stable"
        );
        assert!(found.entries[0].offline);
        assert_eq!(
            found.entries[0].flatpak.as_deref(),
            Some("org.flycast.Flycast")
        );
        // Zoom is not installed here: it needs the network, and says how not to.
        assert!(!found.entries[1].offline);
        assert!(
            found.entries[1]
                .network
                .as_ref()
                .unwrap()
                .contains("its Flatpak can be bundled once installed here")
        );
        let key = scratch.path().join("flathub.trustedkeys.gpg");
        std::fs::write(&key, b"key").unwrap();
        let commit = "c".repeat(64);
        let copied = |refs: &[String], user: bool, bundle: &Path| {
            assert!(!user);
            for reference in refs {
                let path = bundle
                    .join(FLATPAKS)
                    .join("refs/mirrors/org.flathub.Stable")
                    .join(reference);
                std::fs::create_dir_all(path.parent().unwrap()).unwrap();
                std::fs::write(path, format!("{commit}\n")).unwrap();
            }
            true
        };
        let tools = Tools {
            system: &fake,
            download: &|_, _| false,
            dnf_download: &|_, _, _| false,
            zypper_download: &|_, _, _| false,
            create_usb: &copied,
            flatpak_key: &|remote, _| {
                assert_eq!(remote, "flathub");
                key.clone()
            },
        };
        let bundle = scratch.path().join("bundle");
        let made = create(&bundle, &found, &arch, false, None, 1, &tools).unwrap();
        assert_eq!(made.format, 2);
        assert_eq!(made.files.len(), 1, "the remote's key");
        assert_eq!(made.files[0].path, "flatpak-keys/flathub.gpg");
        let read = verify(&bundle, &arch).unwrap();
        assert_eq!(read, made);

        // A machine with the remote: installed from the copy, nothing added.
        let planned = plan_install(&bundle, &read, &[], &arch, &fake, false).unwrap();
        assert_eq!(planned.steps.len(), 1);
        let Step::Shell { program, id, .. } = &planned.steps[0] else {
            panic!("a shell step");
        };
        assert_eq!(id, "bundle-flatpak");
        let repository = std::fs::canonicalize(&bundle).unwrap().join("flatpak");
        assert!(
            program.contains(&format!(
                "sudo flatpak install --system --noninteractive --sideload-repo='{}' flathub org.flycast.Flycast",
                repository.display()
            )),
            "{program}"
        );
        assert!(!program.contains("remote-add"), "{program}");
        assert!(planned.privileged);
        assert!(planned.requirements.contains(&"flatpak".to_string()));

        // Its remote without a collection ID: given the bundle's, which is
        // the one Flathub publishes for its address.
        let planned =
            plan_install(&bundle, &read, &[], &arch, &flatpak_machine("-"), false).unwrap();
        assert!(
            planned.steps[0]
                .display()
                .contains("give the flathub remote its collection ID, org.flathub.Stable")
        );

        // At another address, no collection ID is given on the bundle's word.
        let mut elsewhere = flatpak_machine("-");
        elsewhere.commands.insert(
            "flatpak remotes --system --columns=name,url,collection".into(),
            "flathub\thttps://mirror.example/flathub/\t-\n".into(),
        );
        assert!(matches!(
            plan_install(&bundle, &read, &[], &arch, &elsewhere, false),
            Err(BundleError::Untrusted(_))
        ));
        assert!(plan_install(&bundle, &read, &[], &arch, &elsewhere, true).is_ok());

        // No remote: only the bundle's key would vouch, so only on trust.
        let bare = Fake::default();
        assert!(matches!(
            plan_install(&bundle, &read, &[], &arch, &bare, false),
            Err(BundleError::Untrusted(_))
        ));
        let planned = plan_install(&bundle, &read, &[], &arch, &bare, true).unwrap();
        let Step::Shell { program, .. } = &planned.steps[0] else {
            panic!("a shell step");
        };
        assert!(program.contains(&read.files[0].sha256), "{program}");
        assert!(
            program.contains("sudo flatpak remote-add --system --gpg-import=\"$hf_key\" --collection-id=org.flathub.Stable flathub 'https://dl.flathub.org/repo/'"),
            "{program}"
        );

        // The copy at another commit, or no copy at all, is refused.
        let mirrored = bundle
            .join("flatpak/refs/mirrors/org.flathub.Stable/app/org.flycast.Flycast/x86_64/stable");
        std::fs::write(&mirrored, "d".repeat(64)).unwrap();
        assert!(matches!(
            verify(&bundle, &arch),
            Err(BundleError::Damaged(_))
        ));
        std::fs::remove_file(&mirrored).unwrap();
        assert!(matches!(
            verify(&bundle, &arch),
            Err(BundleError::Damaged(_))
        ));
    }

    #[test]
    fn flatpak_records_are_plain() {
        let flatpak = BundledFlatpak {
            id: "org.example.App".into(),
            reference: "app/org.example.App/x86_64/stable".into(),
            commit: "a".repeat(64),
            remote: "flathub".into(),
            url: "https://dl.flathub.org/repo/".into(),
            collection: "org.flathub.Stable".into(),
            user: false,
        };
        let mut manifest = Manifest {
            flatpaks: vec![flatpak.clone()],
            ..Manifest::default()
        };
        assert!(plain_manifest(&manifest));
        for bad in [
            BundledFlatpak {
                reference: "app/other.App/x86_64/stable".into(),
                ..flatpak.clone()
            },
            BundledFlatpak {
                reference: "runtime/org.example.App/x86_64/stable".into(),
                ..flatpak.clone()
            },
            BundledFlatpak {
                remote: "flathub; rm -rf ~".into(),
                ..flatpak.clone()
            },
            BundledFlatpak {
                collection: "../x".into(),
                ..flatpak.clone()
            },
            BundledFlatpak {
                commit: "short".into(),
                ..flatpak.clone()
            },
            BundledFlatpak {
                url: "https://x/ y".into(),
                ..flatpak.clone()
            },
        ] {
            manifest.flatpaks = vec![bad.clone()];
            assert!(!plain_manifest(&manifest), "{bad:?}");
        }
    }

    #[test]
    fn zypper_downloads_without_root_and_installs_from_the_files() {
        let catalog = Catalog::builtin(false).unwrap();
        let suse = platform(PackageManager::Suse, "opensuse-tumbleweed");
        let scratch = tempfile::tempdir().unwrap();
        let found = contents(
            &["htop".to_string()],
            &catalog,
            &suse,
            true,
            &Fake::default(),
            scratch.path(),
        )
        .unwrap();
        assert_eq!(found.zypper, ["htop"]);
        assert!(found.downloads.is_empty() && found.dnf.is_empty());
        let downloaded = |names: &[String], complete: bool, into: &Path| {
            assert_eq!(names, ["htop"]);
            assert!(complete);
            std::fs::write(into.join("htop-3.5.3-1.1.x86_64.rpm"), b"rpm").is_ok()
                && std::fs::write(into.join("libhwloc15-2.13.0-1.3.x86_64.rpm"), b"rpm").is_ok()
        };
        let tools = Tools {
            system: &Fake::default(),
            download: &|_, _| false,
            dnf_download: &|_, _, _| panic!("not dnf"),
            zypper_download: &downloaded,
            create_usb: &|_, _, _| false,
            flatpak_key: &|_, _| PathBuf::new(),
        };
        let bundle = scratch.path().join("bundle");
        let made = create(&bundle, &found, &suse, true, None, 1, &tools).unwrap();
        assert_eq!(made.family, "opensuse");
        assert_eq!(made.files.len(), 2);
        assert_eq!(
            made.files[0].source.as_deref(),
            Some("zypper install --download-only")
        );
        let read = verify(&bundle, &suse).unwrap();
        // Nothing in this machine's rpm keyring vouches for them: refused.
        let refused =
            plan_install(&bundle, &read, &[], &suse, &Fake::default(), false).unwrap_err();
        assert!(refused.to_string().contains("--trust-bundle"), "{refused}");
        let mut signed = Fake::default();
        for name in [
            "htop-3.5.3-1.1.x86_64.rpm",
            "libhwloc15-2.13.0-1.3.x86_64.rpm",
        ] {
            let file = std::fs::canonicalize(bundle.join("packages").join(name)).unwrap();
            signed.commands.insert(
                format!(
                    "rpm --define _pkgverify_level signature -Kv -- {}",
                    file.display()
                ),
                format!(
                    "{}:\n    Header V4 RSA/SHA512 Signature, key ID 29b700a4: OK\n    Header SHA256 digest: OK\n    Payload SHA256 digest: OK\n",
                    file.display()
                ),
            );
        }
        let planned = plan_install(&bundle, &read, &[], &suse, &signed, false).unwrap();
        let Step::Shell { program, .. } = &planned.steps[0] else {
            panic!("a shell step");
        };
        // Each staged copy's signature is checked again before it installs.
        assert!(
            program.contains(&format!("{RPM_SIGNATURE} -- \"$hf_package\"")),
            "{program}"
        );
        assert!(
            program.contains(RPM_SIGNED) && program.contains(RPM_FAILED),
            "{program}"
        );
        assert!(planned.steps[0].display().contains("rpm keyring"));
        assert!(
            program.contains("sudo zypper $hf_answer --no-refresh --no-remote install"),
            "{program}"
        );
        assert!(
            program.contains("if [ -t 0 ]; then hf_answer=; else hf_answer=--non-interactive; fi")
        );
        assert!(planned.requirements.contains(&"zypper".to_string()));
        // A Fedora bundle is not an openSUSE one, though both are RPMs.
        assert!(matches!(
            verify(&bundle, &platform(PackageManager::Fedora, "fedora")),
            Err(BundleError::Machine(..))
        ));
    }

    #[test]
    fn only_the_familys_package_files_are_handed_to_its_package_manager() {
        assert!(package_file("arch", "htop-3.4.1-1-x86_64.pkg.tar.zst"));
        assert!(package_file("arch", "htop-3.4.1-1-x86_64.pkg.tar.zst.sig"));
        assert!(package_file("arch", "zlib-1:1.3.2-4-x86_64.pkg.tar.xz"));
        assert!(!package_file("arch", ".pkg.tar.zst"));
        assert!(!package_file("arch", "htop.pkg.tar.exe"));
        assert!(!package_file("arch", "htop_3.3_amd64.deb"));
        assert!(package_file("debian", "htop_3.3_amd64.deb"));
        assert!(!package_file("debian", "htop_3.3_amd64.ddeb"));
        assert!(!package_file("debian", "htop_3.3_amd64.changes"));
        assert!(!package_file("debian", ".deb"));
        assert!(!package_file("debian", "htop_3.3_amd64.deb.sig"));
        assert!(package_file("fedora", "htop-3.3.0-3.fc44.x86_64.rpm"));
        assert!(package_file("opensuse", "htop-3.5.3-1.1.x86_64.rpm"));
        assert!(!package_file("fedora", "htop-3.3.0-3.fc44.src.rpm"));
        assert!(!package_file("fedora", "htop_3.3_amd64.deb"));
        assert!(!package_file("alpine", "htop-3.3.0-r0.apk"));
    }

    /// A bundle of one file under packages/, its manifest made to match it.
    fn bundle_of(at: &Path, family: &str, name: &str, bytes: &[u8]) -> Manifest {
        std::fs::create_dir_all(at.join("packages")).unwrap();
        std::fs::write(at.join("packages").join(name), bytes).unwrap();
        let manifest = Manifest {
            format: FORMAT,
            family: family.into(),
            architecture: std::env::consts::ARCH.into(),
            entries: vec![BundledEntry {
                id: "htop".into(),
                name: "htop".into(),
                offline: true,
                packages: vec!["htop".into()],
                ..BundledEntry::default()
            }],
            files: vec![BundledFile {
                path: format!("packages/{name}"),
                sha256: hex(&Sha256::digest(bytes)),
                bytes: bytes.len() as u64,
                ..BundledFile::default()
            }],
            ..Manifest::default()
        };
        std::fs::write(at.join(MANIFEST), toml::to_string(&manifest).unwrap()).unwrap();
        manifest
    }

    #[test]
    fn a_file_apt_would_install_but_not_named_as_a_deb_is_refused() {
        let debian = platform(PackageManager::Debian, "debian");
        let directory = tempfile::tempdir().unwrap();
        bundle_of(directory.path(), "debian", "htop_3.3_amd64.ddeb", b"deb");
        let refused = verify(directory.path(), &debian).unwrap_err();
        assert!(
            refused
                .to_string()
                .contains("htop_3.3_amd64.ddeb is not a debian package"),
            "{refused}"
        );
        // Named as a Debian package, it must still be known to the signed
        // index, every one that apt is handed, unless the bundle is trusted.
        let named = directory.path().join("named");
        let manifest = bundle_of(&named, "debian", "htop_3.3_amd64.deb", b"deb");
        verify(&named, &debian).unwrap();
        let refused =
            plan_install(&named, &manifest, &[], &debian, &Fake::default(), false).unwrap_err();
        assert!(
            refused.to_string().contains("htop_3.3_amd64.deb"),
            "{refused}"
        );
        assert!(plan_install(&named, &manifest, &[], &debian, &Fake::default(), true).is_ok());
    }

    #[test]
    fn rpm_bundles_need_a_signature_by_a_key_in_the_keyring() {
        // rpm 4's words, and rpm 6's.
        assert!(rpm_signed(
            "/b/htop.rpm:\n    Header V4 RSA/SHA256 Signature, key ID a15b79cc: OK\n    Header SHA256 digest: OK\n    Payload SHA256 digest: OK\n    V4 RSA/SHA256 Signature, key ID a15b79cc: OK\n"
        ));
        assert!(rpm_signed(
            "/b/htop.rpm:\n    Header OpenPGP V4 RSA/SHA256 signature, key fingerprint: 36f612dcf27f7d1a48a835e4dbfcf71c6d9f90a6: OK\n    Header SHA256 digest: OK\n"
        ));
        // No signature at all: rpm -K alone passes this.
        assert!(!rpm_signed(
            "/b/htop.rpm:\n    Header SHA256 digest: OK\n    Header SHA1 digest: OK\n    Payload SHA256 digest: OK\n    MD5 digest: OK\n"
        ));
        // A key the keyring lacks, or one check that failed.
        assert!(!rpm_signed(
            "/b/htop.rpm:\n    Header V4 RSA/SHA256 Signature, key ID a15b79cc: NOKEY\n    Header SHA256 digest: OK\n"
        ));
        assert!(!rpm_signed(
            "/b/htop.rpm:\n    Header V4 RSA/SHA256 Signature, key ID a15b79cc: OK\n    Payload SHA256 digest: BAD\n"
        ));
        let fedora = platform(PackageManager::Fedora, "fedora");
        let directory = tempfile::tempdir().unwrap();
        let manifest = bundle_of(
            directory.path(),
            "fedora",
            "foo-1.0-1.x86_64.rpm",
            b"unsigned",
        );
        let read = verify(directory.path(), &fedora).unwrap();
        assert_eq!(read, manifest);
        // Unsigned: refused, unless the bundle is trusted.
        let refused = plan_install(
            directory.path(),
            &read,
            &[],
            &fedora,
            &Fake::default(),
            false,
        )
        .unwrap_err();
        assert!(
            refused
                .to_string()
                .contains("foo-1.0-1.x86_64.rpm carry no signature"),
            "{refused}"
        );
        let trusted = plan_install(
            directory.path(),
            &read,
            &[],
            &fedora,
            &Fake::default(),
            true,
        )
        .unwrap();
        let Step::Shell { program, .. } = &trusted.steps[0] else {
            panic!("a shell step");
        };
        assert!(program.contains("rpm -K \"$hf_staging\"/'foo-1.0-1.x86_64.rpm' || exit 1"));
        assert!(!program.contains("localpkg_gpgcheck"), "{program}");
        assert!(
            trusted.steps[0]
                .display()
                .contains("their digests (rpm -K)")
        );
        // Signed by a key in the keyring: dnf checks it again, and so does
        // the step, on the copy it installs.
        let file =
            std::fs::canonicalize(directory.path().join("packages/foo-1.0-1.x86_64.rpm")).unwrap();
        let mut signed = Fake::default();
        signed.commands.insert(
            format!(
                "rpm --define _pkgverify_level signature -Kv -- {}",
                file.display()
            ),
            "    Header V4 RSA/SHA256 Signature, key ID a15b79cc: OK\n".into(),
        );
        let planned = plan_install(directory.path(), &read, &[], &fedora, &signed, false).unwrap();
        let Step::Shell { program, .. } = &planned.steps[0] else {
            panic!("a shell step");
        };
        assert!(
            program.contains("sudo dnf install --disablerepo='*' --setopt=localpkg_gpgcheck=1"),
            "{program}"
        );
        assert!(program.contains(RPM_SIGNATURE), "{program}");
    }

    #[test]
    fn a_bundle_is_made_by_the_command_line_with_quoted_words() {
        let plan = plan_create(
            Path::new("/usr/bin/helix-toolbox"),
            Path::new("/home/me/my bundle"),
            &["ripgrep".to_string(), "fzf".to_string()],
            true,
        );
        let Step::Shell {
            program, display, ..
        } = &plan.steps[0]
        else {
            panic!("not a shell step");
        };
        assert_eq!(
            program,
            "'/usr/bin/helix-toolbox' 'bundle' 'create' '/home/me/my bundle' 'ripgrep' 'fzf' '--complete'"
        );
        assert_eq!(
            display,
            "/usr/bin/helix-toolbox bundle create /home/me/my bundle ripgrep fzf --complete"
        );
        assert!(!plan.privileged);
        assert_eq!(plan.risk, Risk::Low);
        let folder = scratch().unwrap();
        assert!(folder.is_dir());
        std::fs::remove_dir(&folder).unwrap();
    }
}
