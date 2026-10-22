//! The storage and cleanup center: where the space goes, read without
//! changing anything, and what can be cleaned with an estimate of what it
//! frees. Each cleanup is a plan of its own, confirmed first. Snapshots,
//! the kernels the machine can boot and Helix Toolbox's configuration
//! backups are shown and never offered: what can undo a mistake stays.

use crate::{Plan, Step, doctor::System, quote};
use helix_toolbox_core::{PackageManager, Risk, printable, valid_package_name};
use serde::Serialize;
use std::path::{Path, PathBuf};

/// A file system and its space.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Filesystem {
    /// Where it is looked at: "/", or the home folder when it is apart.
    pub path: String,
    pub free: u64,
    pub total: u64,
}

/// Something that can be cleaned.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
pub struct Item {
    /// package-cache, unused-packages, flatpak-unused, journal, trash,
    /// thumbnails, aur-builds.
    pub id: String,
    pub title: String,
    /// What it is and what cleaning it does.
    pub detail: String,
    /// What it takes now, when known.
    pub size: Option<u64>,
    /// What cleaning it frees, when it can be told beforehand.
    pub frees: Option<u64>,
    /// Packages it removes, for unused packages.
    pub packages: Vec<String>,
    /// Why it is not offered here, when it is not.
    pub not_offered: Option<String>,
}

/// What is shown and never cleaned here.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
pub struct Kept {
    pub title: String,
    pub detail: String,
    pub size: Option<u64>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct Storage {
    pub filesystems: Vec<Filesystem>,
    pub items: Vec<Item>,
    pub kept: Vec<Kept>,
}

/// A folder's size, its files added up, links not followed.
pub fn folder_size(system: &dyn System, path: &Path) -> Option<u64> {
    if !system.exists(path) {
        return None;
    }
    fn walk(system: &dyn System, path: &Path, depth: usize) -> u64 {
        if system.is_link(path) || depth > 32 {
            return 0;
        }
        if let Some(size) = system.size(path) {
            return size;
        }
        system
            .list(path)
            .iter()
            .map(|name| walk(system, &path.join(name), depth + 1))
            .sum()
    }
    Some(walk(system, path, 0))
}

/// "1.20 GiB", "512 KiB", "3.4 MB" as bytes.
pub fn parse_size(text: &str) -> Option<u64> {
    let text = text.trim();
    let split = text.find(|c: char| c.is_ascii_alphabetic())?;
    let number: f64 = text[..split].trim().replace(',', "").parse().ok()?;
    let unit = text[split..].trim();
    let scale: f64 = match unit {
        "B" => 1.0,
        "KiB" | "K" => 1024.0,
        "MiB" | "M" => 1024.0 * 1024.0,
        "GiB" | "G" => 1024.0 * 1024.0 * 1024.0,
        "TiB" | "T" => 1024.0f64.powi(4),
        "kB" | "KB" => 1e3,
        "MB" => 1e6,
        "GB" => 1e9,
        "TB" => 1e12,
        _ => return None,
    };
    Some((number * scale) as u64)
}

/// A size for people: "1.6 GB", "512 KB", "0 B".
pub fn shown(bytes: u64) -> String {
    let units = [("TB", 1e12), ("GB", 1e9), ("MB", 1e6), ("KB", 1e3)];
    for (unit, scale) in units {
        let value = bytes as f64 / scale;
        if value >= 1.0 {
            return if value >= 100.0 {
                format!("{value:.0} {unit}")
            } else {
                format!("{value:.1} {unit}")
            };
        }
    }
    format!("{bytes} B")
}

/// Kernels are never removed as unused packages.
fn kernel(name: &str) -> bool {
    name.starts_with("linux") || name.starts_with("kernel")
}

fn run(system: &dyn System, command: &[&str]) -> Option<String> {
    system
        .run(command)
        .filter(|(ok, _)| *ok)
        .map(|(_, text)| text)
}

/// Read the machine: its file systems, what can be cleaned, what is kept.
pub fn gather(system: &dyn System, manager: Option<PackageManager>) -> Storage {
    let home = system.user().2;
    let mut storage = Storage::default();
    if let Some((free, total)) = system.space(Path::new("/")) {
        storage.filesystems.push(Filesystem {
            path: "/".to_string(),
            free,
            total,
        });
    }
    if let Some((free, total)) = system.space(&home)
        && !storage
            .filesystems
            .iter()
            .any(|other| other.free == free && other.total == total)
    {
        storage.filesystems.push(Filesystem {
            path: home.display().to_string(),
            free,
            total,
        });
    }
    storage.items.push(package_cache(system, manager));
    storage.items.push(unused_packages(system, manager));
    storage.items.push(flatpak_unused(system));
    let journal = ["/var/log/journal", "/run/log/journal"]
        .iter()
        .filter_map(|path| folder_size(system, Path::new(path)))
        .sum::<u64>();
    storage.items.push(Item {
        id: "journal".into(),
        title: "Old system logs".into(),
        detail: "The journal's entries older than four weeks; the last four weeks stay, for Helix Doctor and for finding what went wrong.".into(),
        size: Some(journal),
        not_offered: (!system.on_path("journalctl"))
            .then(|| "journalctl is not installed".to_string()),
        ..Item::default()
    });
    let trash = home.join(".local/share/Trash");
    let trash_size = folder_size(system, &trash);
    storage.items.push(Item {
        id: "trash".into(),
        title: "The trash".into(),
        detail: "Files you deleted to the trash, deleted for good.".into(),
        size: trash_size,
        frees: trash_size,
        not_offered: trash_size
            .filter(|size| *size > 0)
            .is_none()
            .then(|| "the trash is empty".to_string()),
        ..Item::default()
    });
    let thumbnails = folder_size(system, &home.join(".cache/thumbnails"));
    storage.items.push(Item {
        id: "thumbnails".into(),
        title: "Picture previews".into(),
        detail: "The thumbnails file managers keep; they are made again when a folder is opened."
            .into(),
        size: thumbnails,
        frees: thumbnails,
        not_offered: thumbnails
            .filter(|size| *size > 0)
            .is_none()
            .then(|| "there are none".to_string()),
        ..Item::default()
    });
    let builds: Vec<PathBuf> = [".cache/paru/clone", ".cache/yay"]
        .iter()
        .map(|path| home.join(path))
        .filter(|path| system.exists(path))
        .collect();
    let built: u64 = builds
        .iter()
        .filter_map(|path| folder_size(system, path))
        .sum();
    if manager == Some(PackageManager::Arch) {
        storage.items.push(Item {
            id: "aur-builds".into(),
            title: "AUR build folders".into(),
            detail: "What paru and yay downloaded and built AUR packages in; the installed packages stay.".into(),
            size: Some(built),
            frees: Some(built),
            not_offered: (built == 0).then(|| "there are none".to_string()),
            ..Item::default()
        });
    }
    storage.kept = kept(system, &home);
    storage
}

fn package_cache(system: &dyn System, manager: Option<PackageManager>) -> Item {
    let (folders, detail): (&[&str], &str) = match manager {
        Some(PackageManager::Arch) => (
            &["/var/cache/pacman/pkg"],
            "Downloaded packages pacman keeps: the two newest versions of each installed package stay, to go back to; those of removed packages go.",
        ),
        Some(PackageManager::Debian) => (
            &["/var/cache/apt/archives"],
            "Downloaded packages apt keeps after installing them.",
        ),
        Some(PackageManager::Fedora) => (
            &["/var/cache/libdnf5", "/var/cache/dnf"],
            "Downloaded packages dnf keeps; its repository metadata stays.",
        ),
        Some(PackageManager::Suse) => (
            &["/var/cache/zypp/packages"],
            "Downloaded packages zypper keeps.",
        ),
        _ => (&[], ""),
    };
    let size = folders
        .iter()
        .filter_map(|folder| folder_size(system, Path::new(folder)))
        .reduce(|a, b| a + b);
    let mut item = Item {
        id: "package-cache".into(),
        title: "Package cache".into(),
        detail: detail.to_string(),
        size,
        ..Item::default()
    };
    match manager {
        Some(PackageManager::Arch) if system.on_path("paccache") => {
            // paccache's dry runs say what it would free.
            let saved = |args: &[&str]| {
                run(system, args).and_then(|text| {
                    text.lines().find_map(|line| {
                        let rest = line.split("disk space saved:").nth(1)?;
                        parse_size(rest.trim().trim_end_matches(')'))
                    })
                })
            };
            item.frees = Some(
                saved(&["paccache", "-dk2"]).unwrap_or(0)
                    + saved(&["paccache", "-duk0"]).unwrap_or(0),
            );
        }
        Some(PackageManager::Arch) => {
            item.detail = "Downloaded packages pacman keeps; those of packages no longer installed go (pacman -Sc). paccache, from pacman-contrib, would keep the two newest of each instead.".into();
        }
        Some(PackageManager::Debian | PackageManager::Fedora | PackageManager::Suse) => {
            item.frees = size;
        }
        _ => item.not_offered = Some("not for this package manager yet".into()),
    }
    if item.size == Some(0) && item.not_offered.is_none() {
        item.not_offered = Some("it is empty".into());
    }
    item
}

fn unused_packages(system: &dyn System, manager: Option<PackageManager>) -> Item {
    let mut item = Item {
        id: "unused-packages".into(),
        title: "Unused packages".into(),
        detail: "Packages installed only as what others needed, which nothing needs now. Kernels are never among them.".into(),
        ..Item::default()
    };
    let names: Vec<String> = match manager {
        Some(PackageManager::Arch) => run(system, &["pacman", "-Qdtq"])
            .map(|text| text.lines().map(str::to_string).collect())
            .unwrap_or_default(),
        Some(PackageManager::Debian) => run(system, &["apt-get", "-s", "autoremove"])
            .map(|text| {
                text.lines()
                    .filter_map(|line| line.strip_prefix("Remv ")?.split_whitespace().next())
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default(),
        Some(PackageManager::Fedora) => run(
            system,
            &[
                "dnf",
                "repoquery",
                "--unneeded",
                "--queryformat",
                "%{name}\n",
            ],
        )
        .map(|text| text.lines().map(str::to_string).collect())
        .unwrap_or_default(),
        Some(PackageManager::Suse) => run(
            system,
            &[
                "zypper",
                "--non-interactive",
                "--quiet",
                "packages",
                "--unneeded",
            ],
        )
        .map(|text| {
            text.lines()
                .filter_map(|line| {
                    let columns: Vec<&str> = line.split('|').map(str::trim).collect();
                    (columns.len() >= 5 && columns[0] == "i").then(|| columns[2].to_string())
                })
                .collect()
        })
        .unwrap_or_default(),
        _ => {
            item.not_offered = Some("not for this package manager yet".into());
            return item;
        }
    };
    let mut names: Vec<String> = names
        .into_iter()
        .map(|name| name.trim().to_string())
        .filter(|name| valid_package_name(name) && !kernel(name))
        .collect();
    names.sort();
    names.dedup();
    if names.is_empty() {
        item.not_offered = Some("there are none".into());
        return item;
    }
    let mut query: Vec<&str> = match manager {
        Some(PackageManager::Arch) => vec!["pacman", "-Qi"],
        Some(PackageManager::Debian) => vec!["dpkg-query", "-W", "-f=${Installed-Size}\n"],
        _ => vec!["rpm", "-q", "--queryformat", "%{SIZE}\n"],
    };
    query.extend(names.iter().map(String::as_str));
    item.frees = run(system, &query).map(|text| match manager {
        Some(PackageManager::Arch) => text
            .lines()
            .filter_map(|line| parse_size(line.strip_prefix("Installed Size")?.split(':').nth(1)?))
            .sum(),
        Some(PackageManager::Debian) => text
            .lines()
            .filter_map(|line| line.trim().parse::<u64>().ok())
            .map(|kib| kib * 1024)
            .sum(),
        _ => text
            .lines()
            .filter_map(|line| line.trim().parse::<u64>().ok())
            .sum(),
    });
    item.size = item.frees;
    item.packages = names;
    item
}

fn flatpak_unused(system: &dyn System) -> Item {
    let mut item = Item {
        id: "flatpak-unused".into(),
        title: "Unused Flatpak runtimes".into(),
        detail: "Runtimes and extensions no installed Flatpak application uses; flatpak itself works out which, so what it frees is known only then.".into(),
        ..Item::default()
    };
    if !system.on_path("flatpak") {
        item.not_offered = Some("Flatpak is not installed".into());
        return item;
    }
    let runtimes: usize = ["--system", "--user"]
        .iter()
        .filter_map(|installation| {
            run(
                system,
                &[
                    "flatpak",
                    "list",
                    "--runtime",
                    installation,
                    "--columns=ref",
                ],
            )
        })
        .map(|text| text.lines().filter(|line| !line.trim().is_empty()).count())
        .sum();
    if runtimes == 0 {
        item.not_offered = Some("no runtimes are installed".into());
    }
    item
}

fn kept(system: &dyn System, home: &Path) -> Vec<Kept> {
    let mut kept = Vec::new();
    let modules = ["/usr/lib/modules", "/lib/modules"]
        .iter()
        .map(Path::new)
        .find(|path| system.exists(path));
    if let Some(modules) = modules {
        let kernels: Vec<String> = system
            .list(modules)
            .into_iter()
            .filter(|name| !name.starts_with('.'))
            .collect();
        kept.push(Kept {
            title: "Kernels".into(),
            detail: format!(
                "{} installed ({}): the kernels the machine can boot are never removed here; the package manager replaces them as they update.",
                kernels.len(),
                kernels.join(", ")
            ),
            size: None,
        });
    }
    let snapper = system.exists(Path::new("/etc/snapper/configs/root"));
    let timeshift = system.exists(Path::new("/etc/timeshift/timeshift.json"));
    if snapper || timeshift {
        kept.push(Kept {
            title: "Snapshots".into(),
            detail: format!(
                "{} keeps them, to roll the system back; they are never removed here: {} which go.",
                if snapper { "Snapper" } else { "Timeshift" },
                if snapper {
                    "its cleanup settings decide"
                } else {
                    "it decides"
                }
            ),
            size: None,
        });
    }
    let backups = crate::backups::discover(&crate::backups::Roots::here());
    if !backups.is_empty() {
        kept.push(Kept {
            title: "Configuration backups".into(),
            detail: format!(
                "{} copies Helix Toolbox's entries kept before changing a file; Backups puts one back. They are never removed here.",
                backups.len()
            ),
            size: backups
                .iter()
                .map(|backup| folder_size(system, &backup.backup))
                .sum(),
        });
    }
    let _ = home;
    kept
}

/// A plan that cleans one item, as `gather` found it.
pub fn plan(
    item: &Item,
    manager: Option<PackageManager>,
    system: &dyn System,
) -> Result<Plan, String> {
    if let Some(why) = &item.not_offered {
        return Err(format!("{} is not offered: {why}", item.title));
    }
    let home = system.user().2;
    let quoted = |words: &[&str]| {
        words
            .iter()
            .map(|word| quote(word))
            .collect::<Vec<_>>()
            .join(" ")
    };
    let (commands, privileged, risk): (Vec<Vec<String>>, bool, Risk) = match item.id.as_str() {
        "package-cache" => {
            let commands: Vec<Vec<&str>> = match manager {
                Some(PackageManager::Arch) if system.on_path("paccache") => {
                    vec![
                        vec!["sudo", "paccache", "-rk2"],
                        vec!["sudo", "paccache", "-ruk0"],
                    ]
                }
                Some(PackageManager::Arch) => vec![vec!["sudo", "pacman", "-Sc", "--noconfirm"]],
                Some(PackageManager::Debian) => vec![vec!["sudo", "apt-get", "clean"]],
                Some(PackageManager::Fedora) => vec![vec!["sudo", "dnf", "clean", "packages"]],
                Some(PackageManager::Suse) => vec![vec!["sudo", "zypper", "clean", "--all"]],
                _ => return Err("not for this package manager yet".into()),
            };
            (to_owned(commands), true, Risk::Low)
        }
        "unused-packages" => {
            if !item
                .packages
                .iter()
                .all(|name| valid_package_name(name) && !kernel(name))
            {
                return Err("a package name is not one Helix Toolbox runs".into());
            }
            let names: Vec<&str> = item.packages.iter().map(String::as_str).collect();
            let mut command: Vec<&str> = match manager {
                Some(PackageManager::Arch) => vec!["sudo", "pacman", "-Rns", "--noconfirm"],
                Some(PackageManager::Debian) => vec!["sudo", "apt-get", "remove", "-y"],
                Some(PackageManager::Fedora) => vec!["sudo", "dnf", "remove", "-y"],
                Some(PackageManager::Suse) => vec!["sudo", "zypper", "--non-interactive", "remove"],
                _ => return Err("not for this package manager yet".into()),
            };
            command.extend(names);
            (to_owned(vec![command]), true, Risk::Medium)
        }
        "flatpak-unused" => (
            to_owned(vec![
                vec![
                    "flatpak",
                    "uninstall",
                    "--user",
                    "--unused",
                    "--noninteractive",
                    "-y",
                ],
                vec![
                    "sudo",
                    "flatpak",
                    "uninstall",
                    "--system",
                    "--unused",
                    "--noninteractive",
                    "-y",
                ],
            ]),
            true,
            Risk::Low,
        ),
        "journal" => (
            to_owned(vec![vec!["sudo", "journalctl", "--vacuum-time=4weeks"]]),
            true,
            Risk::Low,
        ),
        "trash" => {
            let trash = home.join(".local/share/Trash");
            let command = if system.on_path("gio") {
                vec![
                    "gio".to_string(),
                    "trash".to_string(),
                    "--empty".to_string(),
                ]
            } else {
                vec![
                    "rm".to_string(),
                    "-rf".to_string(),
                    "--".to_string(),
                    trash.join("files").display().to_string(),
                    trash.join("info").display().to_string(),
                ]
            };
            (vec![command], false, Risk::Medium)
        }
        "thumbnails" => (
            vec![vec![
                "rm".to_string(),
                "-rf".to_string(),
                "--".to_string(),
                home.join(".cache/thumbnails").display().to_string(),
            ]],
            false,
            Risk::Low,
        ),
        "aur-builds" => {
            let mut command = vec!["rm".to_string(), "-rf".to_string(), "--".to_string()];
            command.extend(
                [".cache/paru/clone", ".cache/yay"]
                    .iter()
                    .map(|path| home.join(path))
                    .filter(|path| system.exists(path))
                    .map(|path| path.display().to_string()),
            );
            (vec![command], false, Risk::Low)
        }
        other => {
            return Err(format!(
                "{} is not something Helix Toolbox cleans",
                printable(other)
            ));
        }
    };
    let program = commands
        .iter()
        .map(|words| {
            let words: Vec<&str> = words.iter().map(String::as_str).collect();
            quoted(&words)
        })
        .collect::<Vec<_>>()
        .join(" && ");
    let display = commands
        .iter()
        .map(|words| words.join(" "))
        .collect::<Vec<_>>()
        .join("; then ");
    let mut requires: Vec<String> = commands
        .iter()
        .filter_map(|words| words.iter().find(|word| *word != "sudo").cloned())
        .collect();
    if privileged {
        requires.push("sudo".to_string());
    }
    requires.sort();
    requires.dedup();
    let mut notes = Vec::new();
    if let Some(frees) = item.frees {
        notes.push(format!("It frees about {}.", shown(frees)));
    }
    if item.id == "unused-packages" {
        notes.push(format!("It removes {}.", item.packages.join(", ")));
    }
    Ok(Plan {
        steps: vec![Step::Shell {
            id: format!("clean-{}", item.id),
            name: format!("Clean: {}", item.title),
            program,
            display,
            requires: requires.clone(),
            changes: Vec::new(),
        }],
        notes,
        requirements: requires,
        privileged,
        risk,
        depends: vec![Vec::new()],
        ..Plan::default()
    })
}

fn to_owned(commands: Vec<Vec<&str>>) -> Vec<Vec<String>> {
    commands
        .into_iter()
        .map(|words| words.into_iter().map(str::to_string).collect())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[derive(Default)]
    struct Fake {
        /// Files and their sizes.
        files: HashMap<PathBuf, u64>,
        commands: HashMap<String, String>,
        tools: Vec<&'static str>,
    }

    impl Fake {
        fn file(&mut self, path: &str, size: u64) {
            self.files.insert(PathBuf::from(path), size);
        }
    }

    impl System for Fake {
        fn read(&self, _: &Path) -> Option<String> {
            None
        }
        fn exists(&self, path: &Path) -> bool {
            self.files.keys().any(|file| file.starts_with(path))
        }
        fn list(&self, path: &Path) -> Vec<String> {
            let mut names: Vec<String> = self
                .files
                .keys()
                .filter_map(|file| file.strip_prefix(path).ok())
                .filter_map(|rest| rest.components().next())
                .map(|part| part.as_os_str().to_string_lossy().into_owned())
                .collect();
            names.sort();
            names.dedup();
            names
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
        fn space(&self, path: &Path) -> Option<(u64, u64)> {
            (path == Path::new("/")).then_some((40_000_000_000, 500_000_000_000))
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
        fn on_path(&self, tool: &str) -> bool {
            self.tools.contains(&tool)
        }
        fn processes(&self) -> Vec<String> {
            Vec::new()
        }
        fn size(&self, path: &Path) -> Option<u64> {
            self.files.get(path).copied()
        }
    }

    fn arch() -> Fake {
        let mut fake = Fake {
            tools: vec!["paccache", "journalctl", "gio", "flatpak"],
            ..Fake::default()
        };
        fake.file("/var/cache/pacman/pkg/a-1-x86_64.pkg.tar.zst", 3_000_000);
        fake.file("/var/cache/pacman/pkg/a-2-x86_64.pkg.tar.zst", 3_000_000);
        fake.file("/var/log/journal/abc/system.journal", 800_000_000);
        fake.file("/home/me/.local/share/Trash/files/old.iso", 2_000_000_000);
        fake.file("/home/me/.cache/thumbnails/large/x.png", 1_000);
        fake.file("/home/me/.cache/paru/clone/foo/foo.pkg.tar.zst", 50_000_000);
        fake.file("/usr/lib/modules/6.12.1-arch1-1/vmlinuz", 1);
        fake.file("/usr/lib/modules/6.6.60-1-lts/vmlinuz", 1);
        fake.commands.insert(
            "paccache -dk2".into(),
            "==> finished dry run: 12 candidates (disk space saved: 1.50 GiB)\n".into(),
        );
        fake.commands.insert(
            "paccache -duk0".into(),
            "==> no candidate packages found for pruning\n".into(),
        );
        fake.commands.insert(
            "pacman -Qdtq".into(),
            "libfoo\nlinux-headers\npython-old\n".into(),
        );
        fake.commands.insert(
            "pacman -Qi libfoo python-old".into(),
            "Name            : libfoo\nInstalled Size  : 2.00 MiB\n\nName            : python-old\nInstalled Size  : 512.00 KiB\n".into(),
        );
        fake.commands.insert(
            "flatpak list --runtime --system --columns=ref".into(),
            "org.kde.Platform/x86_64/6.7\n".into(),
        );
        fake
    }

    #[test]
    fn sizes_are_read_as_tools_write_them() {
        assert_eq!(parse_size("1.50 GiB"), Some(1_610_612_736));
        assert_eq!(parse_size("512.00 KiB"), Some(524_288));
        assert_eq!(parse_size("3.4 MB"), Some(3_400_000));
        assert_eq!(parse_size("nonsense"), None);
        assert_eq!(shown(1_610_612_736), "1.6 GB");
        assert_eq!(shown(524_288), "524 KB");
        assert_eq!(shown(12), "12 B");
    }

    #[test]
    fn what_can_be_cleaned_and_what_it_frees() {
        let machine = arch();
        let found = gather(&machine, Some(PackageManager::Arch));
        assert_eq!(found.filesystems[0].path, "/");
        let item = |id: &str| found.items.iter().find(|item| item.id == id).unwrap();
        assert_eq!(item("package-cache").size, Some(6_000_000));
        assert_eq!(item("package-cache").frees, Some(1_610_612_736));
        let unused = item("unused-packages");
        assert_eq!(
            unused.packages,
            ["libfoo", "python-old"],
            "kernels and their headers stay"
        );
        assert_eq!(unused.frees, Some(2 * 1024 * 1024 + 512 * 1024));
        assert_eq!(item("journal").size, Some(800_000_000));
        assert_eq!(item("trash").frees, Some(2_000_000_000));
        assert_eq!(item("aur-builds").frees, Some(50_000_000));
        assert!(item("flatpak-unused").not_offered.is_none());
        assert!(
            found
                .kept
                .iter()
                .any(|kept| kept.title == "Kernels" && kept.detail.starts_with("2 installed"))
        );
    }

    #[test]
    fn each_cleanup_is_a_plan_of_its_own_and_kernels_never_go() {
        let machine = arch();
        let found = gather(&machine, Some(PackageManager::Arch));
        let item = |id: &str| {
            found
                .items
                .iter()
                .find(|item| item.id == id)
                .unwrap()
                .clone()
        };
        let cache = plan(&item("package-cache"), Some(PackageManager::Arch), &machine).unwrap();
        assert_eq!(
            cache.steps[0].display(),
            "sudo paccache -rk2; then sudo paccache -ruk0"
        );
        assert!(
            cache.privileged && cache.notes[0].contains("1.6 GB"),
            "{:?}",
            cache.notes
        );
        let unused = plan(
            &item("unused-packages"),
            Some(PackageManager::Arch),
            &machine,
        )
        .unwrap();
        assert_eq!(
            unused.steps[0].display(),
            "sudo pacman -Rns --noconfirm libfoo python-old"
        );
        assert_eq!(unused.risk, Risk::Medium);
        let mut sneaky = item("unused-packages");
        sneaky.packages.push("linux".into());
        assert!(plan(&sneaky, Some(PackageManager::Arch), &machine).is_err());
        let trash = plan(&item("trash"), None, &machine).unwrap();
        assert_eq!(trash.steps[0].display(), "gio trash --empty");
        assert!(!trash.privileged);
        let builds = plan(&item("aur-builds"), None, &machine).unwrap();
        assert_eq!(
            builds.steps[0].display(),
            "rm -rf -- /home/me/.cache/paru/clone"
        );
        let empty = Fake::default();
        let none = gather(&empty, Some(PackageManager::Arch));
        let trash = none.items.iter().find(|item| item.id == "trash").unwrap();
        assert!(
            plan(trash, None, &empty).is_err(),
            "an empty trash is not offered"
        );
    }

    #[test]
    fn other_package_managers_name_their_unused_packages() {
        let mut debian = Fake::default();
        debian.commands.insert(
            "apt-get -s autoremove".into(),
            "Reading package lists...\nRemv libold1 [1.0-1]\nRemv linux-image-6.1.0-9-amd64 [6.1]\n".into(),
        );
        debian.commands.insert(
            "dpkg-query -W -f=${Installed-Size}\n libold1".into(),
            "300\n".into(),
        );
        let found = gather(&debian, Some(PackageManager::Debian));
        let unused = found
            .items
            .iter()
            .find(|item| item.id == "unused-packages")
            .unwrap();
        assert_eq!(unused.packages, ["libold1"]);
        assert_eq!(unused.frees, Some(300 * 1024));
        let planned = plan(unused, Some(PackageManager::Debian), &debian).unwrap();
        assert_eq!(planned.steps[0].display(), "sudo apt-get remove -y libold1");
        let mut suse = Fake::default();
        suse.commands.insert(
            "zypper --non-interactive --quiet packages --unneeded".into(),
            "S | Repository | Name | Version | Arch\n--+------------+------+---------+------\ni | repo-oss   | libx | 1.0     | x86_64\n".into(),
        );
        let found = gather(&suse, Some(PackageManager::Suse));
        let unused = found
            .items
            .iter()
            .find(|item| item.id == "unused-packages")
            .unwrap();
        assert_eq!(unused.packages, ["libx"]);
    }
}
