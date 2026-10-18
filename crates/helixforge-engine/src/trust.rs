//! The trust inspector: where an entry's installer comes from, what checks
//! the integrity of what it installs, what it downloads unchecked, what
//! privileges it uses and how far it has been tested. It reads files and
//! asks package managers read-only questions; it changes nothing.
//!
//! Three kinds of assurance are kept apart: the provenance of the HelixForge
//! release the installer ships in, the integrity checks package managers
//! make, and downloads that nothing checks.

use crate::{
    doctor::System,
    history,
    reliability::{Assessment, Evidence},
    updates::Install,
};
use helixforge_core::{Action, Entry, NixAction, PackageManager, printable};
use helixforge_platform::LinuxInfo;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

/// How much a fact assures.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Standing {
    /// Signed build provenance that can be verified.
    Provenance,
    /// A checksum or signature that a tool checks before using it.
    Checked,
    /// Fixed to one revision by its hash.
    Pinned,
    /// Nothing checks it.
    Unverified,
    /// Cannot be told from here.
    Unknown,
}

impl Standing {
    pub fn label(self) -> &'static str {
        match self {
            Standing::Provenance => "Release provenance",
            Standing::Checked => "Integrity checked",
            Standing::Pinned => "Pinned revision",
            Standing::Unverified => "Unverified",
            Standing::Unknown => "Unknown",
        }
    }
}

/// One thing the inspector found.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Fact {
    pub standing: Standing,
    /// What it is about: "HelixForge", "pacman", "Download".
    pub subject: String,
    pub text: String,
}

fn fact(standing: Standing, subject: impl Into<String>, text: impl Into<String>) -> Fact {
    Fact {
        standing,
        subject: subject.into(),
        text: printable(&text.into()),
    }
}

/// What this copy of HelixForge is, for the provenance of what it ships.
#[derive(Clone, Debug)]
pub struct Build {
    pub version: String,
    /// Built by the release workflow (the version was stamped in).
    pub release: bool,
    /// The commit it was built from, when stamped.
    pub commit: Option<String>,
    pub install: Install,
}

/// Everything the inspector found about one entry.
#[derive(Clone, Debug, Serialize)]
pub struct Inspection {
    pub id: String,
    pub name: String,
    /// What runs: "packages", "script", "command", "nix", "native", "profile".
    pub kind: &'static str,
    /// Where the installer comes from.
    pub origin: String,
    /// The installer file, relative to the scripts folder.
    pub file: Option<String>,
    pub sha256: Option<String>,
    /// The installer's license.
    pub license: Option<String>,
    /// The software's license, as its package manager reports it.
    pub software_license: Option<String>,
    /// The release HelixForge build the installer ships in.
    pub provenance: Vec<Fact>,
    /// What checks the packages it installs.
    pub integrity: Vec<Fact>,
    /// What the installer downloads itself.
    pub downloads: Vec<Fact>,
    /// What it does with elevated privileges.
    pub privileges: Vec<String>,
    pub validation: Assessment,
}

/// Inspect an entry. `root` holds the catalog and scripts this build ships.
pub fn inspect(
    entry: &Entry,
    root: &Path,
    platform: &LinuxInfo,
    build: &Build,
    system: &dyn System,
    evidence: &Evidence,
    history: &[history::Record],
) -> Inspection {
    let mut inspection = Inspection {
        id: entry.id.clone(),
        name: entry.name.clone(),
        kind: "command",
        origin: "the HelixForge catalog".to_string(),
        file: None,
        sha256: None,
        license: Some("MIT (HelixForge)".to_string()),
        software_license: None,
        provenance: provenance(build),
        integrity: Vec::new(),
        downloads: Vec::new(),
        privileges: privileges(entry),
        validation: evidence.assess(entry, history),
    };
    match &entry.action {
        Action::Package { .. } => {
            inspection.kind = "packages";
            inspection.origin =
                "commands HelixForge builds from the package names in its catalog".to_string();
        }
        Action::Script { file, .. } => {
            inspection.kind = "script";
            let scripts = root.join("scripts");
            let relative = file
                .strip_prefix(&scripts)
                .unwrap_or(file)
                .display()
                .to_string();
            inspection.sha256 = sum(file);
            let (origin, license) = script_origin(&relative);
            inspection.origin = origin;
            inspection.license = Some(license);
            inspection.file = Some(printable(&relative));
            let texts = script_texts(file);
            inspection.downloads = downloads(&texts, &scripts);
            inspection.privileges.extend(
                elevated_commands(&texts)
                    .into_iter()
                    .map(|command| format!("runs {command} as root")),
            );
        }
        Action::Command { line } => {
            inspection.kind = "command";
            inspection.origin = format!("a command line in the catalog: {}", printable(line));
        }
        Action::Nix { action } => {
            inspection.kind = "nix";
            let (what, text) = match action {
                NixAction::ProfileInstall { installable } => (
                    installable,
                    "Nix builds or downloads it and checks every store path's hash; a flake's lock file pins its inputs",
                ),
                NixAction::HomeManagerSwitch { flake } => (
                    flake,
                    "Home Manager switches to this flake; Nix checks every store path's hash, and the flake's lock file pins its inputs",
                ),
            };
            inspection.origin = format!("Nix: {}", printable(what));
            inspection
                .integrity
                .push(fact(Standing::Checked, "Nix", text));
        }
        Action::Native { action } => {
            inspection.kind = "native";
            inspection.origin = format!(
                "HelixForge itself (helixforge native {}), reading the system",
                action.id()
            );
        }
        Action::Profile { entries } => {
            inspection.kind = "profile";
            inspection.origin = format!(
                "a profile of {} entries; inspect each one: {}",
                entries.len(),
                printable(&entries.join(", "))
            );
        }
    }
    if let Some(identity) = entry.identity() {
        inspection.integrity.extend(integrity(
            identity.packages.for_manager_any(platform.manager),
            identity.aur,
            identity.flatpak,
            platform,
            system,
        ));
        inspection.software_license = software_license(
            identity.packages.for_manager_any(platform.manager),
            identity.flatpak,
            platform,
            system,
        );
    }
    inspection.integrity.sort_by_key(|fact| fact.standing);
    inspection
        .downloads
        .sort_by_key(|fact| std::cmp::Reverse(fact.standing));
    inspection
}

/// What vouches for the HelixForge build the installer ships in.
fn provenance(build: &Build) -> Vec<Fact> {
    let subject = format!("HelixForge {}", build.version);
    let commit = build
        .commit
        .as_deref()
        .map(|commit| format!(" of commit {}", commit.chars().take(12).collect::<String>()))
        .unwrap_or_default();
    if !build.release {
        return vec![fact(
            Standing::Unknown,
            subject,
            match build.install {
                Install::Cargo => "built on this machine by cargo install; crates.io checked the source's checksums, and the catalog and scripts came with it, but there is no build provenance for this copy".to_string(),
                _ => "built from source, not by the release workflow, so there is no build provenance; the catalog and scripts are those of the checkout".to_string(),
            },
        )];
    }
    let mut facts = vec![fact(
        Standing::Provenance,
        subject.clone(),
        format!(
            "a release build{commit}: the catalog and every installer ship inside it, and each release file has a signed build provenance attestation; gh attestation verify <file> --repo Commanderx-code/Helix-Forge shows which workflow built it"
        ),
    )];
    match build.install {
        Install::Launcher | Install::Download => facts.push(fact(
            Standing::Checked,
            subject,
            "run from where the launcher or start.sh keeps it; both check the download against the release's SHA256SUMS before running it",
        )),
        Install::Package => facts.push(fact(
            Standing::Checked,
            subject,
            "installed from the release's package; its checksum is in the release's SHA256SUMS",
        )),
        _ => {}
    }
    facts
}

/// Where a script under `scripts/` comes from, and its license.
fn script_origin(relative: &str) -> (String, String) {
    if relative.starts_with("installers/commander/") {
        (
            "Commander Toolbox's installers, maintained in the HelixForge repository (github.com/Commanderx-code/Helix-Forge)".to_string(),
            "MIT (HelixForge)".to_string(),
        )
    } else if relative.starts_with("installers/") {
        (
            "Linutil's catalog (github.com/ChrisTitusTech/linutil), carried into the HelixForge repository".to_string(),
            "MIT (Linutil; scripts/installers/LICENSE-linutil)".to_string(),
        )
    } else {
        (
            "the HelixForge repository".to_string(),
            "MIT (HelixForge)".to_string(),
        )
    }
}

fn sum(path: &Path) -> Option<String> {
    let bytes = std::fs::read(path).ok()?;
    Some(
        Sha256::digest(&bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect(),
    )
}

/// The script's text and that of the scripts beside it that it runs, one
/// level deep; the shared helpers are left out.
fn script_texts(file: &Path) -> Vec<String> {
    let mut texts = Vec::new();
    let Ok(text) = std::fs::read_to_string(file) else {
        return texts;
    };
    let directory = file.parent().unwrap_or(Path::new("."));
    let mut seen: Vec<PathBuf> = vec![file.to_path_buf()];
    for word in text
        .split(|character: char| !(character.is_ascii_alphanumeric() || "-_.".contains(character)))
    {
        if !word.ends_with(".sh") || word.starts_with("common") {
            continue;
        }
        let other = directory.join(word);
        if !seen.contains(&other)
            && other.is_file()
            && let Ok(more) = std::fs::read_to_string(&other)
        {
            seen.push(other);
            texts.push(more);
        }
    }
    texts.insert(0, text);
    texts
}

/// The pinned sources `fetch_source` knows, from the shared helpers.
pub(crate) fn pins(scripts: &Path) -> Vec<(String, String, String)> {
    let Ok(text) = std::fs::read_to_string(scripts.join("installers/commander/common.sh")) else {
        return Vec::new();
    };
    text.lines()
        .filter_map(|line| {
            let (name, rest) = line.trim().split_once(") ")?;
            let revision = rest
                .split("revision=")
                .nth(1)?
                .split(|c: char| !c.is_ascii_hexdigit())
                .next()?;
            if revision.len() != 40 || name.contains(['*', '|', ' ']) {
                return None;
            }
            let repository = rest
                .split("source_repo=")
                .nth(1)
                .and_then(|value| value.split(';').next())
                .map(str::to_string)
                .unwrap_or_else(|| format!("Commanderx-code/{name}"));
            Some((name.to_string(), repository, revision.to_string()))
        })
        .collect()
}

/// What a script downloads, and whether anything checks it.
fn downloads(texts: &[String], scripts: &Path) -> Vec<Fact> {
    let mut facts = Vec::new();
    let all = texts.join("\n");
    let pins = pins(scripts);
    for (name, repository, revision) in &pins {
        if all.contains(&format!("fetch_source {name}"))
            || (name == "dotfiles" || name == "Myfish")
                && all.contains("fetch_source \"$source_name\"")
        {
            facts.push(fact(
                Standing::Pinned,
                "Download",
                format!(
                    "github.com/{repository} at commit {}, checked after the download",
                    &revision[..12]
                ),
            ));
        }
    }
    let checks = [
        "sha256sum",
        "sha512sum",
        "gpg --verify",
        "minisign",
        "cosign verify",
    ]
    .into_iter()
    .filter(|tool| all.contains(tool))
    .collect::<Vec<_>>();
    let mut urls: Vec<String> = Vec::new();
    for line in all.lines() {
        let line = line.trim();
        if line.starts_with('#') {
            continue;
        }
        for (start, _) in line.match_indices("http") {
            let rest = &line[start..];
            if !(rest.starts_with("https://") || rest.starts_with("http://")) {
                continue;
            }
            let url: String = rest
                .chars()
                .take_while(|character| {
                    !character.is_whitespace() && !"\"'()<>|;`".contains(*character)
                })
                .collect();
            if url.len() < 12 || urls.contains(&url) {
                continue;
            }
            urls.push(url.clone());
            let piped = line.contains("| sh") || line.contains("| bash") || line.contains("| sudo");
            let (standing, text) = if url.starts_with("http://") {
                (
                    Standing::Unverified,
                    format!("{url} over plain HTTP, which anyone on the network can change"),
                )
            } else if piped {
                (
                    Standing::Unverified,
                    format!("{url}, run straight into a shell without a check"),
                )
            } else if !checks.is_empty() {
                (
                    Standing::Checked,
                    format!(
                        "{url}; the script checks downloads with {}",
                        checks.join(", ")
                    ),
                )
            } else {
                (
                    Standing::Unverified,
                    format!("{url}, over HTTPS but with no checksum or signature check"),
                )
            };
            facts.push(fact(standing, "Download", text));
        }
    }
    if all.contains("git clone") && !all.contains("fetch_source") {
        facts.push(fact(
            Standing::Unverified,
            "Download",
            "clones a Git repository at whatever its branch holds when it runs",
        ));
    }
    facts
}

/// What a script runs through sudo or the escalation tool.
fn elevated_commands(texts: &[String]) -> Vec<String> {
    let mut commands: Vec<String> = Vec::new();
    for text in texts {
        for line in text.lines() {
            if line.trim_start().starts_with('#') {
                continue;
            }
            for marker in ["sudo ", "\"$ESCALATION_TOOL\" ", "$ESCALATION_TOOL "] {
                for (start, _) in line.match_indices(marker) {
                    let word: String = line[start + marker.len()..]
                        .trim_start()
                        .trim_start_matches('"')
                        .chars()
                        .take_while(|character| {
                            character.is_ascii_alphanumeric() || "-_.$".contains(*character)
                        })
                        .collect();
                    let word = match word.as_str() {
                        "$PACKAGER" | "${PACKAGER}" => "the package manager".to_string(),
                        _ => word,
                    };
                    if !word.is_empty() && !word.starts_with('-') && !commands.contains(&word) {
                        commands.push(word);
                    }
                }
            }
        }
    }
    commands.sort();
    commands
}

/// Privileges, from the entry's flags.
fn privileges(entry: &Entry) -> Vec<String> {
    let mut found = Vec::new();
    if helixforge_core::privileged(&entry.flags, &entry.action) {
        found.push("asks for your password (sudo) while it runs".to_string());
    } else {
        found.push("runs as your user, without asking for a password".to_string());
    }
    let flags: Vec<&str> = entry.flags.split_whitespace().collect();
    for (flag, text) in [
        ("D", "changes disks, such as partitions"),
        ("K", "changes the kernel"),
        ("SI", "installs a whole system"),
        ("SS", "starts, stops or enables services"),
        ("PFM", "changes system files"),
        ("FM", "changes your files"),
        ("RP", "removes packages"),
    ] {
        if flags.contains(&flag) {
            found.push(text.to_string());
        }
    }
    for path in &entry.writes {
        found.push(format!("writes {}", printable(path)));
    }
    found
}

/// What checks the packages an entry installs on this machine.
fn integrity(
    native: Option<(PackageManager, &str)>,
    aur: Option<&str>,
    flatpak: Option<&str>,
    platform: &LinuxInfo,
    system: &dyn System,
) -> Vec<Fact> {
    let mut facts = Vec::new();
    if let Some((manager, name)) = native {
        facts.push(match manager {
            PackageManager::Arch => {
                let level = system
                    .read(Path::new("/etc/pacman.conf"))
                    .and_then(|text| sig_level(&text))
                    .unwrap_or_else(|| "Required DatabaseOptional".to_string());
                if level.contains("Never") || level.contains("TrustAll") {
                    fact(
                        Standing::Unverified,
                        format!("pacman ({name})"),
                        format!(
                            "pacman.conf has SigLevel = {level}, so package signatures are not required"
                        ),
                    )
                } else {
                    fact(
                        Standing::Checked,
                        format!("pacman ({name})"),
                        format!(
                            "pacman checks each package's signature against the distribution's keyring (SigLevel = {level})"
                        ),
                    )
                }
            }
            PackageManager::Debian => {
                let trusted = ["/etc/apt/sources.list"]
                    .iter()
                    .map(PathBuf::from)
                    .chain(
                        system
                            .list(Path::new("/etc/apt/sources.list.d"))
                            .into_iter()
                            .map(|name| PathBuf::from("/etc/apt/sources.list.d").join(name)),
                    )
                    .filter_map(|path| system.read(&path))
                    .any(|text| text.contains("trusted=yes") || text.contains("Trusted: yes"));
                if trusted {
                    fact(
                        Standing::Unverified,
                        format!("apt ({name})"),
                        "a repository is marked trusted=yes, so apt does not check its signature",
                    )
                } else {
                    fact(
                        Standing::Checked,
                        format!("apt ({name})"),
                        "apt checks the repository's signed Release file and each package's checksum",
                    )
                }
            }
            PackageManager::Fedora => {
                let off = system
                    .read(Path::new("/etc/dnf/dnf.conf"))
                    .is_some_and(|text| {
                        text.lines().any(|line| {
                            let line = line.replace(' ', "").to_lowercase();
                            line == "gpgcheck=0" || line == "gpgcheck=false"
                        })
                    });
                if off {
                    fact(
                        Standing::Unverified,
                        format!("dnf ({name})"),
                        "dnf.conf turns gpgcheck off, so package signatures are not checked",
                    )
                } else {
                    fact(
                        Standing::Checked,
                        format!("dnf ({name})"),
                        "dnf checks each package's GPG signature (gpgcheck)",
                    )
                }
            }
            PackageManager::Suse => fact(
                Standing::Checked,
                format!("zypper ({name})"),
                "zypper checks the repository's and each package's GPG signature",
            ),
            other => fact(
                Standing::Checked,
                format!("{} ({name})", other.executable()),
                "the distribution's package manager checks its packages' signatures",
            ),
        });
    }
    if let Some(name) = aur.filter(|_| platform.manager == Some(PackageManager::Arch)) {
        facts.push(fact(
            Standing::Unverified,
            format!("AUR ({name})"),
            "if pacman has no package, the AUR's build script is used: users submit them and nobody reviews them; paru or yay shows it before building, and checks the sums it lists for its downloads",
        ));
    }
    if let Some(id) = flatpak {
        facts.push(fact(
            Standing::Checked,
            format!("Flathub ({id})"),
            "Flatpak checks Flathub's signed commits; who publishes the app on Flathub is shown on its Flathub page",
        ));
    }
    facts
}

/// The SigLevel in pacman.conf's [options].
fn sig_level(text: &str) -> Option<String> {
    let mut options = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            options = line == "[options]";
        } else if options
            && let Some(value) = line.strip_prefix("SigLevel")
            && let Some(value) = value.trim_start().strip_prefix('=')
        {
            return Some(value.trim().to_string());
        }
    }
    None
}

/// The software's license from its package manager's metadata, read-only.
fn software_license(
    native: Option<(PackageManager, &str)>,
    flatpak: Option<&str>,
    _platform: &LinuxInfo,
    system: &dyn System,
) -> Option<String> {
    let field = |text: &str, key: &str| {
        text.lines().find_map(|line| {
            let (name, value) = line.split_once(':')?;
            (name.trim() == key).then(|| printable(value.trim()))
        })
    };
    if let Some((manager, name)) = native {
        let found = match manager {
            PackageManager::Arch => system
                .run(&["pacman", "-Qi", name])
                .filter(|(ok, _)| *ok)
                .or_else(|| system.run(&["pacman", "-Si", name]).filter(|(ok, _)| *ok))
                .and_then(|(_, text)| field(&text, "Licenses")),
            PackageManager::Fedora | PackageManager::Suse => system
                .run(&["rpm", "-q", "--qf", "%{LICENSE}", name])
                .filter(|(ok, _)| *ok)
                .map(|(_, text)| printable(text.trim())),
            PackageManager::Debian => {
                let copyright = PathBuf::from(format!("/usr/share/doc/{name}/copyright"));
                system
                    .exists(&copyright)
                    .then(|| format!("in {}", copyright.display()))
            }
            _ => None,
        };
        if found.as_deref().is_some_and(|text| !text.is_empty()) {
            return found;
        }
    }
    let id = flatpak?;
    system
        .run(&["flatpak", "info", id])
        .filter(|(ok, _)| *ok)
        .and_then(|(_, text)| field(&text, "License"))
}

/// The family's package name with its manager.
trait ForManagerAny {
    fn for_manager_any(&self, manager: Option<PackageManager>) -> Option<(PackageManager, &str)>;
}

impl ForManagerAny for helixforge_core::PackageNames {
    fn for_manager_any(&self, manager: Option<PackageManager>) -> Option<(PackageManager, &str)> {
        let manager = manager?;
        self.for_manager(manager).map(|name| (manager, name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use helixforge_catalog::Catalog;
    use std::collections::HashMap;

    /// A machine with fixed files and command answers.
    #[derive(Default)]
    struct Fake {
        files: HashMap<PathBuf, String>,
        commands: HashMap<String, String>,
    }

    impl System for Fake {
        fn read(&self, path: &Path) -> Option<String> {
            self.files.get(path).cloned()
        }
        fn exists(&self, path: &Path) -> bool {
            self.files.contains_key(path)
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
            false
        }
        fn processes(&self) -> Vec<String> {
            Vec::new()
        }
    }

    fn arch() -> LinuxInfo {
        LinuxInfo {
            id: "arch".into(),
            pretty_name: "Arch Linux".into(),
            id_like: vec![],
            manager: Some(PackageManager::Arch),
        }
    }

    fn release() -> Build {
        Build {
            version: "1.2.0".into(),
            release: true,
            commit: Some("0123456789abcdef0123456789abcdef01234567".into()),
            install: Install::Package,
        }
    }

    #[test]
    fn a_package_entry_shows_each_sources_checks() {
        let catalog = Catalog::builtin(false).unwrap();
        let mut fake = Fake::default();
        fake.files.insert(
            PathBuf::from("/etc/pacman.conf"),
            "[options]\nSigLevel = Required DatabaseOptional\n[core]\n".into(),
        );
        fake.commands.insert(
            "pacman -Qi ripgrep".into(),
            "Name            : ripgrep\nLicenses        : MIT  Unlicense\n".into(),
        );
        let found = inspect(
            catalog.find("ripgrep").unwrap(),
            catalog.root(),
            &arch(),
            &release(),
            &fake,
            &Evidence::default(),
            &[],
        );
        assert_eq!(found.kind, "packages");
        assert_eq!(found.software_license.as_deref(), Some("MIT  Unlicense"));
        assert_eq!(found.integrity[0].standing, Standing::Checked);
        assert!(found.integrity[0].text.contains("SigLevel = Required"));
        assert_eq!(found.provenance[0].standing, Standing::Provenance);
        assert!(found.provenance[0].text.contains("0123456789ab"));
        assert!(found.privileges[0].contains("password"));

        // A system that turns signatures off is told apart.
        fake.files.insert(
            PathBuf::from("/etc/pacman.conf"),
            "[options]\nSigLevel = Never\n".into(),
        );
        let found = inspect(
            catalog.find("ripgrep").unwrap(),
            catalog.root(),
            &arch(),
            &release(),
            &fake,
            &Evidence::default(),
            &[],
        );
        assert_eq!(found.integrity[0].standing, Standing::Unverified);
    }

    #[test]
    fn scripts_show_their_origin_downloads_and_root_commands() {
        let catalog = Catalog::builtin(false).unwrap();
        let fake = Fake::default();
        let inspect_id = |id: &str, build: &Build| {
            inspect(
                catalog.find(id).unwrap(),
                catalog.root(),
                &arch(),
                build,
                &fake,
                &Evidence::default(),
                &[],
            )
        };
        let sddm = inspect_id("sddm", &release());
        assert_eq!(sddm.kind, "script");
        assert!(sddm.origin.contains("Commander"));
        assert_eq!(sddm.sha256.as_ref().map(String::len), Some(64));
        assert!(
            sddm.downloads
                .iter()
                .any(|fact| fact.standing == Standing::Pinned
                    && fact.text.contains("uiriansan/SilentSDDM")),
            "{:?}",
            sddm.downloads
        );
        // The dotfiles entries reach their pinned source through dotfiles.sh.
        let ghostty = inspect_id("dotfiles-ghostty", &release());
        assert!(
            ghostty
                .downloads
                .iter()
                .any(|fact| fact.text.contains("Commanderx-code/dotfiles")),
            "{:?}",
            ghostty.downloads
        );
        // A piped install script is unverified, and listed first.
        let dank = inspect_id("dank-shell", &release());
        assert!(dank.origin.contains("Linutil"));
        assert_eq!(dank.downloads[0].standing, Standing::Unverified);
        assert!(dank.downloads[0].text.contains("install.danklinux.com"));
        assert!(dank.downloads[0].text.contains("shell"));
        // A local build has no provenance to offer.
        let local = Build {
            release: false,
            commit: None,
            install: Install::Other,
            ..release()
        };
        let resolve = inspect_id("resolve-native", &local);
        assert_eq!(resolve.provenance[0].standing, Standing::Unknown);
        assert!(!resolve.privileges.is_empty());
    }

    #[test]
    fn elevated_commands_are_read_from_the_script() {
        let texts = vec![
            "sudo pacman -S x\n\"$ESCALATION_TOOL\" systemctl enable sddm\n# sudo rm -rf /\n$ESCALATION_TOOL \"$PACKAGER\" install y\n".to_string(),
        ];
        assert_eq!(
            elevated_commands(&texts),
            ["pacman", "systemctl", "the package manager"]
        );
    }
}
