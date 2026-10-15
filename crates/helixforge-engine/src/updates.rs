//! One overview of pending updates across the tools that keep this machine:
//! the distribution's package manager, an AUR helper, Flatpak and Nix. It
//! reads what each tool already knows; an online check asks the network only
//! through read-only commands. Updating is delegated to the tools themselves.

use std::path::{Path, PathBuf};

use helixforge_core::{PackageManager, printable};
use helixforge_platform::LinuxInfo;

use crate::doctor::System;
use crate::{Plan, Risk, Source, Step};

/// One package waiting for an update.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Pending {
    pub name: String,
    pub from: Option<String>,
    pub to: Option<String>,
}

/// One tool that updates part of the machine.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Channel {
    /// `pacman`, `aur`, `apt`, `dnf`, `zypper`, `flatpak-system`,
    /// `flatpak-user`, `home-manager`, `nixos` or `nix-profile`.
    pub id: String,
    pub name: String,
    /// What is waiting; None when the tool cannot say without updating, or
    /// only online.
    pub pending: Option<Vec<Pending>>,
    /// Where the answer comes from, or what is known instead.
    pub note: String,
    /// What updating runs, in order.
    pub commands: Vec<Vec<String>>,
    pub requires: Vec<String>,
    /// What the update changes beyond packages, said before it runs.
    pub caveat: Option<String>,
}

impl Channel {
    /// How many updates are known to wait.
    pub fn count(&self) -> Option<usize> {
        self.pending.as_ref().map(Vec::len)
    }

    /// A short status: "12 updates", "up to date" or "unknown".
    pub fn summary(&self) -> String {
        match self.count() {
            Some(0) => "up to date".to_string(),
            Some(1) => "1 update".to_string(),
            Some(count) => format!("{count} updates"),
            None => "not counted".to_string(),
        }
    }

    /// The commands as shown, safe to print.
    pub fn command_line(&self) -> String {
        printable(
            &self
                .commands
                .iter()
                .map(|command| command.join(" "))
                .collect::<Vec<_>>()
                .join(" && "),
        )
    }
}

/// Where to look, apart from the commands.
pub struct Places {
    /// `$XDG_CONFIG_HOME`, or `~/.config`.
    pub config: PathBuf,
    pub home: PathBuf,
    /// Seconds since 1970, for how old the Nix inputs are.
    pub now: u64,
}

impl Places {
    pub fn here() -> Self {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/"));
        let config = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
            .unwrap_or_else(|| home.join(".config"));
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| elapsed.as_secs())
            .unwrap_or(0);
        Places { config, home, now }
    }
}

fn words(command: &[&str]) -> Vec<String> {
    command.iter().map(|word| word.to_string()).collect()
}

fn strings(tools: &[&str]) -> Vec<String> {
    words(tools)
}

/// The standard output of a command, whatever its exit status: pacman -Qu
/// and checkupdates exit non-zero when nothing waits. None when it cannot
/// run.
fn read(system: &dyn System, command: &[&str]) -> Option<String> {
    system.run(command).map(|(_, text)| text)
}

/// Every channel this machine has. `online` also asks the network, through
/// read-only commands (checkupdates, the AUR helper, Flatpak's remotes).
pub fn overview(
    system: &dyn System,
    platform: &LinuxInfo,
    places: &Places,
    online: bool,
) -> Vec<Channel> {
    let mut channels = Vec::new();
    match platform.manager {
        Some(PackageManager::Arch) if system.on_path("pacman") => {
            let (pending, note) = if online && system.on_path("checkupdates") {
                (
                    read(system, &["checkupdates"]).map(|text| arrows(&text)),
                    "checked online with checkupdates",
                )
            } else {
                (
                    read(system, &["pacman", "-Qu"]).map(|text| arrows(&text)),
                    "as of pacman's last sync",
                )
            };
            channels.push(Channel {
                id: "pacman".into(),
                name: "Arch packages".into(),
                pending,
                note: note.into(),
                commands: vec![words(&["sudo", "pacman", "-Syu"])],
                requires: strings(&["pacman", "sudo"]),
                caveat: Some(
                    "updates the whole system: Arch does not support partial upgrades".into(),
                ),
            });
            if let Some(helper) = ["paru", "yay"]
                .into_iter()
                .find(|tool| system.on_path(tool))
            {
                let pending = online
                    .then(|| read(system, &[helper, "-Qua"]).map(|text| arrows(&text)))
                    .flatten();
                channels.push(Channel {
                    id: "aur".into(),
                    name: format!("AUR ({helper})"),
                    note: if online {
                        format!("checked online with {helper} -Qua")
                    } else {
                        "the AUR is only asked in an online check".into()
                    },
                    pending,
                    commands: vec![words(&[helper, "-Sua"])],
                    requires: strings(&[helper]),
                    caveat: Some(
                        "builds AUR packages from their PKGBUILDs; read them when asked".into(),
                    ),
                });
            }
        }
        Some(PackageManager::Debian) if system.on_path("apt") => {
            channels.push(Channel {
                id: "apt".into(),
                name: "Debian packages".into(),
                pending: read(system, &["apt", "list", "--upgradable"]).map(|text| apt(&text)),
                note: "as of the last apt update, which the update runs first".into(),
                commands: vec![
                    words(&["sudo", "apt-get", "update"]),
                    words(&["sudo", "apt-get", "upgrade"]),
                ],
                requires: strings(&["apt-get", "sudo"]),
                caveat: None,
            });
        }
        Some(PackageManager::Fedora) if system.on_path("dnf") => {
            let command: &[&str] = if online {
                &["dnf", "-q", "list", "--upgrades"]
            } else {
                &["dnf", "-C", "-q", "list", "--upgrades"]
            };
            channels.push(Channel {
                id: "dnf".into(),
                name: "Fedora packages".into(),
                pending: read(system, command).map(|text| dnf(&text)),
                note: if online {
                    "checked online with dnf".into()
                } else {
                    "from dnf's cached metadata".into()
                },
                commands: vec![words(&["sudo", "dnf", "upgrade"])],
                requires: strings(&["dnf", "sudo"]),
                caveat: None,
            });
        }
        Some(PackageManager::Suse) if system.on_path("zypper") => {
            // Tumbleweed moves as a whole, through a distribution upgrade.
            let rolling = platform.id.contains("tumbleweed");
            channels.push(Channel {
                id: "zypper".into(),
                name: "openSUSE packages".into(),
                pending: read(system, &["zypper", "-q", "--no-refresh", "list-updates"])
                    .map(|text| zypper(&text)),
                note: "from zypper's cached repositories".into(),
                commands: vec![words(if rolling {
                    &["sudo", "zypper", "dist-upgrade"]
                } else {
                    &["sudo", "zypper", "update"]
                })],
                requires: strings(&["zypper", "sudo"]),
                caveat: None,
            });
        }
        _ => {}
    }
    if system.on_path("flatpak") {
        for (installation, id, name) in [
            ("--system", "flatpak-system", "Flatpak (system)"),
            ("--user", "flatpak-user", "Flatpak (user)"),
        ] {
            let installed = read(
                system,
                &["flatpak", "list", installation, "--columns=application"],
            )
            .unwrap_or_default();
            if installed.trim().is_empty() {
                continue;
            }
            let mut command = vec![
                "flatpak",
                "remote-ls",
                "--updates",
                installation,
                "--columns=application,version",
            ];
            if !online {
                command.push("--cached");
            }
            let system_wide = installation == "--system";
            channels.push(Channel {
                id: id.into(),
                name: name.into(),
                pending: read(system, &command).map(|text| columns(&text)),
                note: if online {
                    "checked online with flatpak remote-ls".into()
                } else {
                    "from Flatpak's cached remote metadata".into()
                },
                commands: vec![if system_wide {
                    words(&["sudo", "flatpak", "update", "--system"])
                } else {
                    words(&["flatpak", "update", "--user"])
                }],
                requires: if system_wide {
                    strings(&["flatpak", "sudo"])
                } else {
                    strings(&["flatpak"])
                },
                caveat: None,
            });
        }
    }
    if let Some(channel) = home_manager(system, places) {
        channels.push(channel);
    }
    if system.exists(Path::new("/etc/NIXOS")) {
        let flake = Path::new("/etc/nixos/flake.nix");
        let (commands, caveat) = if system.exists(flake) {
            (
                vec![
                    words(&["sudo", "nix", "flake", "update", "--flake", "/etc/nixos"]),
                    words(&["sudo", "nixos-rebuild", "switch", "--flake", "/etc/nixos"]),
                ],
                Some("updates /etc/nixos/flake.lock".to_string()),
            )
        } else {
            (
                vec![words(&["sudo", "nixos-rebuild", "switch", "--upgrade"])],
                None,
            )
        };
        channels.push(Channel {
            id: "nixos".into(),
            name: "NixOS".into(),
            pending: None,
            note: lock_age(system, Path::new("/etc/nixos/flake.lock"), places.now)
                .unwrap_or_else(|| "NixOS counts nothing before it rebuilds".into()),
            commands,
            requires: strings(&["nixos-rebuild", "sudo"]),
            caveat,
        });
    }
    let manifest = places.home.join(".nix-profile/manifest.json");
    if system.on_path("nix")
        && let Some(text) = system.read(&manifest)
        && let Ok(json) = serde_json::from_str::<serde_json::Value>(&text)
    {
        let count = match &json["elements"] {
            serde_json::Value::Object(map) => map.len(),
            serde_json::Value::Array(list) => list.len(),
            _ => 0,
        };
        if count > 0 {
            channels.push(Channel {
                id: "nix-profile".into(),
                name: "Nix profile".into(),
                pending: None,
                note: format!(
                    "{count} package{} installed with nix profile",
                    if count == 1 { "" } else { "s" }
                ),
                commands: vec![words(&["nix", "profile", "upgrade", "--all"])],
                requires: strings(&["nix"]),
                caveat: None,
            });
        }
    }
    // Package names, versions, flake input names and paths come from outside
    // HelixForge: keep terminal control characters out of what is shown.
    channels.into_iter().map(cleaned).collect()
}

fn cleaned(mut channel: Channel) -> Channel {
    channel.name = printable(&channel.name);
    channel.note = printable(&channel.note);
    channel.caveat = channel.caveat.as_deref().map(printable);
    for pending in channel.pending.iter_mut().flatten() {
        pending.name = printable(&pending.name);
        pending.from = pending.from.as_deref().map(printable);
        pending.to = pending.to.as_deref().map(printable);
    }
    channel
}

/// Home Manager with a flake in its configuration directory.
fn home_manager(system: &dyn System, places: &Places) -> Option<Channel> {
    if !system.on_path("home-manager") {
        return None;
    }
    let directory = places.config.join("home-manager");
    if !system.exists(&directory.join("flake.nix")) {
        return None;
    }
    // A link to a dotfiles checkout is where the lock file really changes.
    let shown = system
        .canonical(&directory)
        .unwrap_or_else(|| directory.clone());
    let flake = directory.to_string_lossy().into_owned();
    Some(Channel {
        id: "home-manager".into(),
        name: "Home Manager".into(),
        pending: None,
        note: lock_age(system, &directory.join("flake.lock"), places.now)
            .unwrap_or_else(|| "its flake inputs are not locked yet".into()),
        commands: vec![
            words(&["nix", "flake", "update", "--flake", &flake]),
            words(&["home-manager", "switch", "--flake", &flake]),
        ],
        requires: strings(&["nix", "home-manager"]),
        caveat: Some(format!(
            "updates {}; commit it there if you keep it in Git",
            shown.join("flake.lock").display()
        )),
    })
}

/// How old the locked nixpkgs (or else the oldest input) is.
fn lock_age(system: &dyn System, lock: &Path, now: u64) -> Option<String> {
    let json: serde_json::Value = serde_json::from_str(&system.read(lock)?).ok()?;
    let nodes = json["nodes"].as_object()?;
    let when = |node: &serde_json::Value| node["locked"]["lastModified"].as_u64();
    let (name, time) = match nodes.get("nixpkgs").and_then(when) {
        Some(time) => ("nixpkgs", time),
        None => nodes
            .iter()
            .filter_map(|(name, node)| Some((name.as_str(), when(node)?)))
            .min_by_key(|(_, time)| *time)?,
    };
    let days = now.saturating_sub(time) / 86_400;
    Some(format!(
        "{name} locked at {}, {}",
        date(time),
        match days {
            0 => "today".to_string(),
            1 => "1 day ago".to_string(),
            days => format!("{days} days ago"),
        }
    ))
}

/// A UTC date, YYYY-MM-DD.
fn date(seconds: u64) -> String {
    // Days to a civil date (Howard Hinnant's algorithm).
    let days = (seconds / 86_400) as i64 + 719_468;
    let era = days.div_euclid(146_097);
    let day_of_era = days.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

/// `name old -> new` lines: pacman -Qu, checkupdates, paru and yay -Qua.
pub fn arrows(text: &str) -> Vec<Pending> {
    text.lines()
        .filter(|line| !line.contains("[ignored]"))
        .filter_map(|line| {
            let mut words = line.split_whitespace();
            let name = words.next()?;
            let from = words.next()?;
            (words.next()? == "->").then_some(())?;
            Some(Pending {
                name: name.to_string(),
                from: Some(from.to_string()),
                to: Some(words.next()?.to_string()),
            })
        })
        .collect()
}

/// `name/suite new arch [upgradable from: old]` lines from apt list.
pub fn apt(text: &str) -> Vec<Pending> {
    text.lines()
        .filter_map(|line| {
            let (head, from) = line.split_once("[upgradable from:")?;
            let mut words = head.split_whitespace();
            let name = words.next()?.split('/').next()?;
            Some(Pending {
                name: name.to_string(),
                from: Some(from.trim().trim_end_matches(']').trim().to_string()),
                to: words.next().map(String::from),
            })
        })
        .collect()
}

/// `name.arch version repository` lines from dnf list --upgrades.
pub fn dnf(text: &str) -> Vec<Pending> {
    let mut found: Vec<Pending> = crate::inventory::parse_dnf_upgrades(text)
        .into_iter()
        .map(|(name, to)| Pending {
            name,
            from: None,
            to: Some(to),
        })
        .collect();
    found.sort_by(|a, b| a.name.cmp(&b.name));
    found
}

/// `v | repository | name | current | available | arch` rows from zypper.
pub fn zypper(text: &str) -> Vec<Pending> {
    text.lines()
        .filter_map(|line| {
            let cells: Vec<&str> = line.split('|').map(str::trim).collect();
            (cells.len() >= 5 && cells[0] == "v").then(|| Pending {
                name: cells[2].to_string(),
                from: Some(cells[3].to_string()),
                to: Some(cells[4].to_string()),
            })
        })
        .collect()
}

/// Tab-separated `application version` columns from flatpak remote-ls.
pub fn columns(text: &str) -> Vec<Pending> {
    text.lines()
        .filter_map(|line| {
            let mut cells = line.split('\t').map(str::trim);
            let name = cells.next().filter(|name| !name.is_empty())?;
            Some(Pending {
                name: name.to_string(),
                from: None,
                to: cells
                    .next()
                    .filter(|version| !version.is_empty())
                    .map(String::from),
            })
        })
        .collect()
}

/// A plan updating the channels in order, each through its own tool, one
/// after another; a later channel waits for the native packages.
pub fn plan(channels: &[&Channel]) -> Plan {
    let mut plan = Plan::default();
    for channel in channels {
        plan.privileged |= channel.requires.iter().any(|tool| tool == "sudo");
        plan.risk = plan.risk.max(match channel.id.as_str() {
            "pacman" | "apt" | "dnf" | "zypper" | "nixos" => Risk::High,
            _ => Risk::Medium,
        });
        plan.requirements.extend(channel.requires.iter().cloned());
        if let Some(caveat) = &channel.caveat {
            plan.notes.push(format!("{}: {caveat}", channel.name));
        }
        plan.steps.push(Step::Update {
            id: format!("update-{}", channel.id),
            name: format!("Update {}", channel.name),
            source: Source {
                label: channel.name.clone(),
                commands: channel.commands.clone(),
                requires: channel.requires.clone(),
                probe: None,
            },
            targets: Vec::new(),
        });
    }
    // The AUR builds against the repositories' packages: only after them.
    let native = plan.steps.iter().position(|step| {
        matches!(
            step.id(),
            "update-pacman" | "update-apt" | "update-dnf" | "update-zypper"
        )
    });
    plan.depends = plan
        .steps
        .iter()
        .map(|step| match native {
            Some(index) if step.id() == "update-aur" => vec![index],
            _ => Vec::new(),
        })
        .collect();
    plan.requirements.sort();
    plan.requirements.dedup();
    plan
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doctor::Fake;

    fn arch() -> LinuxInfo {
        LinuxInfo::from_os_release("ID=garuda\nID_LIKE=arch\n").unwrap()
    }

    fn places() -> Places {
        Places {
            config: PathBuf::from("/home/me/.config"),
            home: PathBuf::from("/home/me"),
            // 2026-10-15
            now: 1_792_022_400,
        }
    }

    fn machine() -> Fake {
        let mut fake = Fake {
            tools: [
                "pacman",
                "checkupdates",
                "paru",
                "flatpak",
                "home-manager",
                "nix",
            ]
            .map(String::from)
            .to_vec(),
            ..Fake::default()
        };
        fake.commands.insert(
            "pacman -Qu".into(),
            (
                true,
                "ripgrep 15.2.0-1 -> 15.3.0-1\nlinux 6.1-1 -> 6.2-1 [ignored]\n".into(),
            ),
        );
        fake.commands.insert(
            "checkupdates".into(),
            (
                true,
                "ripgrep 15.2.0-1 -> 15.3.0-1\nmesa 1-1 -> 2-1\n".into(),
            ),
        );
        // paru exits 1 when nothing waits.
        fake.commands
            .insert("paru -Qua".into(), (false, String::new()));
        fake.commands.insert(
            "flatpak list --system --columns=application".into(),
            (true, "org.zapzap.ZapZap\n".into()),
        );
        fake.commands.insert(
            "flatpak list --user --columns=application".into(),
            (true, String::new()),
        );
        fake.commands.insert(
            "flatpak remote-ls --updates --system --columns=application,version --cached".into(),
            (true, "org.zapzap.ZapZap\t7.6\n".into()),
        );
        fake.files.insert(
            PathBuf::from("/home/me/.config/home-manager/flake.nix"),
            "{}".into(),
        );
        fake.files.insert(
            PathBuf::from("/home/me/.config/home-manager/flake.lock"),
            r#"{"nodes":{"nixpkgs":{"locked":{"lastModified":1791331200}},"systems":{"locked":{"lastModified":1681000000}},"root":{}}}"#.into(),
        );
        fake.canonical.insert(
            PathBuf::from("/home/me/.config/home-manager"),
            PathBuf::from("/home/me/dotfiles/home-manager"),
        );
        fake
    }

    #[test]
    fn offline_the_overview_reads_what_the_tools_know() {
        let channels = overview(&machine(), &arch(), &places(), false);
        let ids: Vec<&str> = channels.iter().map(|channel| channel.id.as_str()).collect();
        assert_eq!(ids, ["pacman", "aur", "flatpak-system", "home-manager"]);
        assert_eq!(
            channels[0].summary(),
            "1 update",
            "ignored packages are left out"
        );
        assert_eq!(
            channels[0].pending.as_ref().unwrap()[0],
            Pending {
                name: "ripgrep".into(),
                from: Some("15.2.0-1".into()),
                to: Some("15.3.0-1".into()),
            }
        );
        assert_eq!(channels[1].pending, None, "the AUR only online");
        assert_eq!(channels[2].summary(), "1 update");
        let home = &channels[3];
        assert_eq!(home.note, "nixpkgs locked at 2026-10-07, 8 days ago");
        assert_eq!(
            home.command_line(),
            "nix flake update --flake /home/me/.config/home-manager && home-manager switch --flake /home/me/.config/home-manager"
        );
        assert!(
            home.caveat
                .as_deref()
                .unwrap()
                .contains("/home/me/dotfiles/home-manager/flake.lock")
        );
    }

    #[test]
    fn online_the_overview_asks_read_only_commands() {
        let channels = overview(&machine(), &arch(), &places(), true);
        assert_eq!(channels[0].summary(), "2 updates");
        assert!(channels[0].note.contains("checkupdates"));
        assert_eq!(channels[1].summary(), "up to date");
        // The online Flatpak command was not given, so it is unknown.
        assert_eq!(channels[2].pending, None);
    }

    #[test]
    fn control_characters_from_the_tools_are_not_shown() {
        let mut fake = machine();
        fake.commands.insert(
            "pacman -Qu".into(),
            (true, "evil\x1b]0;x\x07 1 -> 2\x1b[2J\n".into()),
        );
        fake.files.insert(
            PathBuf::from("/home/me/.config/home-manager/flake.lock"),
            r#"{"nodes":{"bad\u001b[31m":{"locked":{"lastModified":1791331200}},"root":{}}}"#
                .into(),
        );
        let channels = overview(&fake, &arch(), &places(), false);
        let shown = format!("{channels:?}");
        assert!(
            !shown.contains('\x1b') && !shown.contains('\x07'),
            "{shown}"
        );
        let pending = &channels[0].pending.as_ref().unwrap()[0];
        assert_eq!(pending.name, "evil?]0;x?");
        assert_eq!(pending.to.as_deref(), Some("2?[2J"));
        assert!(channels[3].note.starts_with("bad?[31m locked at"));
        // The plan's commands are shown cleaned, though they run as given.
        let mut odd = channels[3].clone();
        odd.commands = vec![vec!["nix".into(), "/home/\x1b[2Jme".into()]];
        let plan = plan(&[&odd]);
        assert_eq!(plan.steps[0].display(), "nix /home/?[2Jme");
    }

    #[test]
    fn a_machine_without_the_tools_has_no_channels() {
        let fake = Fake::default();
        assert!(overview(&fake, &arch(), &places(), false).is_empty());
    }

    #[test]
    fn the_parsers_read_each_tool() {
        let apt = apt("Listing...\nbash/stable 5.2.15-2+b3 amd64 [upgradable from: 5.2.15-2+b2]\n");
        assert_eq!(apt[0].name, "bash");
        assert_eq!(apt[0].from.as_deref(), Some("5.2.15-2+b2"));
        assert_eq!(apt[0].to.as_deref(), Some("5.2.15-2+b3"));
        let zypper = zypper(
            "S | Repository | Name | Current Version | Available Version | Arch\n--+---\nv | Main | vim | 9.0-1 | 9.1-1 | x86_64\n",
        );
        assert_eq!(zypper.len(), 1);
        assert_eq!(zypper[0].to.as_deref(), Some("9.1-1"));
        let dnf = dnf("Available Upgrades\ngit.x86_64 2.47.0-1.fc41 updates\n");
        assert_eq!(dnf[0].name, "git");
        assert_eq!(date(0), "1970-01-01");
        assert_eq!(date(1_792_022_400), "2026-10-15");
    }

    #[test]
    fn the_plan_runs_each_tool_in_order_and_the_aur_after_pacman() {
        let channels = overview(&machine(), &arch(), &places(), false);
        let chosen: Vec<&Channel> = channels.iter().collect();
        let plan = plan(&chosen);
        let ids: Vec<&str> = plan.steps.iter().map(|step| step.id()).collect();
        assert_eq!(
            ids,
            [
                "update-pacman",
                "update-aur",
                "update-flatpak-system",
                "update-home-manager"
            ]
        );
        assert_eq!(plan.depends[1], [0]);
        assert!(plan.privileged);
        assert_eq!(plan.risk, Risk::High);
        assert!(
            plan.notes
                .iter()
                .any(|note| note.contains("partial upgrades"))
        );
        assert_eq!(plan.steps[0].display(), "sudo pacman -Syu");
        assert_eq!(plan.steps[1].display(), "paru -Sua");
    }
}
