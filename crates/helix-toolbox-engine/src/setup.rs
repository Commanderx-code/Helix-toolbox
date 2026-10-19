//! The setup wizard: what this machine is (distribution, desktop, graphics,
//! package tools), the workstations it offers (`catalog/setup.toml`), and
//! which of their entries this machine can run. The wizard only queues
//! entries for an ordinary Install Plan; it runs nothing itself.

use crate::{Machine, desktop::Kind, plan_with};
use helix_toolbox_catalog::Catalog;
use helix_toolbox_core::{PackageManager, printable};
use helix_toolbox_platform::LinuxInfo;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// `catalog/setup.toml`.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct Setup {
    #[serde(default, rename = "workstation")]
    pub workstations: Vec<Workstation>,
    #[serde(default, rename = "desktop")]
    pub desktops: Vec<DesktopEntries>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Workstation {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub entries: Vec<String>,
    #[serde(default)]
    pub when: Vec<When>,
}

/// Entries added when every condition given holds.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct When {
    #[serde(default)]
    pub gpu: Option<String>,
    #[serde(default)]
    pub family: Option<String>,
    #[serde(default)]
    pub not_family: Option<String>,
    #[serde(default)]
    pub desktop: Option<String>,
    pub entries: Vec<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct DesktopEntries {
    /// A desktop's id, or `any`.
    pub id: String,
    pub entries: Vec<String>,
}

/// What the wizard learns about the machine.
#[derive(Clone, Debug, Default, Serialize)]
pub struct Survey {
    pub distribution: String,
    /// arch, debian, fedora, suse, …
    pub family: Option<String>,
    pub package_manager: Option<String>,
    /// Package tools besides the native one: paru, yay, flatpak, nix,
    /// home-manager, snap.
    pub tools: Vec<String>,
    pub desktop: Option<Kind>,
    /// "KDE Plasma 6.8.0 on Wayland".
    pub desktop_name: Option<String>,
    pub gpus: Vec<String>,
    pub laptop: bool,
}

/// One suggested entry and whether it can run here.
#[derive(Clone, Debug, Serialize)]
pub struct Suggestion {
    pub id: String,
    pub name: String,
    pub available: bool,
    /// Why it cannot run here, or what is worth knowing.
    pub note: Option<String>,
}

pub fn family_of(manager: Option<PackageManager>) -> Option<&'static str> {
    Some(match manager? {
        PackageManager::Arch => "arch",
        PackageManager::Debian => "debian",
        PackageManager::Fedora => "fedora",
        PackageManager::Suse => "suse",
        PackageManager::Alpine => "alpine",
        PackageManager::Void => "void",
        PackageManager::Solus => "solus",
    })
}

/// Read `catalog/setup.toml` from a catalog's root.
pub fn load(root: &Path) -> Result<Setup, String> {
    let path = root.join("catalog/setup.toml");
    let text =
        std::fs::read_to_string(&path).map_err(|error| format!("{}: {error}", path.display()))?;
    toml::from_str(&text)
        .map_err(|error| format!("{}: {}", path.display(), printable(error.message())))
}

/// Look at the machine. `on_path` answers whether a tool is installed.
pub fn survey(
    platform: &LinuxInfo,
    desktop: &crate::desktop::Desktop,
    gpus: Vec<String>,
    laptop: bool,
    on_path: &dyn Fn(&str) -> bool,
) -> Survey {
    Survey {
        distribution: printable(&platform.pretty_name),
        family: family_of(platform.manager).map(String::from),
        package_manager: platform.manager.map(|manager| manager.label().to_string()),
        tools: ["paru", "yay", "flatpak", "nix", "home-manager", "snap"]
            .into_iter()
            .filter(|tool| on_path(tool))
            .map(String::from)
            .collect(),
        desktop: desktop.kind,
        desktop_name: desktop.name.as_ref().map(|name| match &desktop.session {
            Some(session) => format!(
                "{name} on {}",
                match session.as_str() {
                    "wayland" => "Wayland".to_string(),
                    "x11" => "X11".to_string(),
                    other => other.to_string(),
                }
            ),
            None => name.clone(),
        }),
        gpus,
        laptop,
    }
}

impl When {
    fn holds(&self, survey: &Survey) -> bool {
        let family = survey.family.as_deref();
        self.gpu
            .as_ref()
            .is_none_or(|gpu| survey.gpus.contains(gpu))
            && self
                .family
                .as_deref()
                .is_none_or(|wanted| family == Some(wanted))
            && self
                .not_family
                .as_deref()
                .is_none_or(|unwanted| family != Some(unwanted))
            && self
                .desktop
                .as_deref()
                .is_none_or(|wanted| survey.desktop.map(Kind::id) == Some(wanted))
    }
}

impl Workstation {
    /// Its entries for this machine, in order, once each.
    pub fn entries_for(&self, survey: &Survey) -> Vec<String> {
        let mut ids: Vec<String> = Vec::new();
        let conditional = self
            .when
            .iter()
            .filter(|when| when.holds(survey))
            .flat_map(|when| when.entries.iter());
        for id in self.entries.iter().chain(conditional) {
            if !ids.contains(id) {
                ids.push(id.clone());
            }
        }
        ids
    }
}

impl Setup {
    /// Every workstation's entries, for a custom choice.
    pub fn everything(&self, survey: &Survey) -> Vec<String> {
        let mut ids: Vec<String> = Vec::new();
        for workstation in &self.workstations {
            for id in workstation.entries_for(survey) {
                if !ids.contains(&id) {
                    ids.push(id);
                }
            }
        }
        ids
    }

    /// The entries offered for a desktop: its own, then those for any.
    pub fn for_desktop(&self, kind: Option<Kind>) -> Vec<String> {
        let mut ids: Vec<String> = Vec::new();
        for group in &self.desktops {
            if group.id == "any" || kind.map(Kind::id) == Some(group.id.as_str()) {
                for id in &group.entries {
                    if !ids.contains(id) {
                        ids.push(id.clone());
                    }
                }
            }
        }
        // The desktop's own first.
        ids.sort_by_key(|id| {
            !self.desktops.iter().any(|group| {
                group.id != "any"
                    && kind.map(Kind::id) == Some(group.id.as_str())
                    && group.entries.contains(id)
            })
        });
        ids
    }
}

/// Which suggested entries this machine can run: the catalog has them here
/// (preconditions hide some) and they can be planned for this distribution.
pub fn suggestions(ids: &[String], catalog: &Catalog, platform: &LinuxInfo) -> Vec<Suggestion> {
    let nothing = Machine {
        holds: &|_| false,
        on_path: &|_| false,
    };
    ids.iter()
        .map(|id| match catalog.find(id) {
            None => Suggestion {
                id: id.clone(),
                name: printable(id),
                available: false,
                note: Some("not offered on this machine".to_string()),
            },
            Some(entry) => {
                let planned = plan_with([entry], platform, &|id| catalog.find(id), nothing);
                Suggestion {
                    id: id.clone(),
                    name: entry.name.clone(),
                    available: planned.is_ok(),
                    note: match planned {
                        Ok(_) if !entry.multi_select => {
                            Some("runs on its own, outside the Install Plan".to_string())
                        }
                        Ok(_) => None,
                        Err(_) => Some(format!(
                            "no package for {}",
                            printable(&platform.pretty_name)
                        )),
                    },
                }
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn arch() -> LinuxInfo {
        LinuxInfo {
            id: "arch".into(),
            pretty_name: "Arch Linux".into(),
            id_like: vec![],
            manager: Some(PackageManager::Arch),
        }
    }

    fn fedora() -> LinuxInfo {
        LinuxInfo {
            id: "fedora".into(),
            pretty_name: "Fedora Linux".into(),
            id_like: vec![],
            manager: Some(PackageManager::Fedora),
        }
    }

    #[test]
    fn workstations_fit_the_machine() {
        let catalog = Catalog::builtin(false).unwrap();
        let setup = load(catalog.root()).unwrap();
        let ids: Vec<&str> = setup.workstations.iter().map(|w| w.id.as_str()).collect();
        assert_eq!(ids, ["developer", "gaming", "privacy", "minimal", "custom"]);
        // Every name the file gives is a catalog entry.
        for id in setup.everything(&Survey::default()).iter().chain(
            setup
                .workstations
                .iter()
                .flat_map(|w| w.when.iter().flat_map(|when| when.entries.iter())),
        ) {
            assert!(catalog.find(id).is_some(), "setup.toml names {id}");
        }
        let desktop = crate::desktop::Desktop::default();
        let on_arch = survey(
            &arch(),
            &desktop,
            vec!["nvidia".into(), "intel".into()],
            true,
            &|tool| tool == "paru",
        );
        assert_eq!(on_arch.tools, ["paru"]);
        let gaming = setup
            .workstations
            .iter()
            .find(|w| w.id == "gaming")
            .unwrap();
        assert!(
            gaming
                .entries_for(&on_arch)
                .contains(&"nvidia-drivers-hardware-acceleration".to_string())
        );
        let on_fedora = survey(&fedora(), &desktop, vec!["nvidia".into()], false, &|_| {
            false
        });
        let entries = gaming.entries_for(&on_fedora);
        assert!(entries.contains(&"nvidia-proprietary-drivers".to_string()));
        assert!(!entries.contains(&"nvidia-drivers-hardware-acceleration".to_string()));
        let privacy = setup
            .workstations
            .iter()
            .find(|w| w.id == "privacy")
            .unwrap();
        assert!(
            privacy
                .entries_for(&on_fedora)
                .contains(&"selinux".to_string())
        );
        assert!(
            privacy
                .entries_for(&on_arch)
                .contains(&"apparmor".to_string())
        );
        assert!(
            setup
                .workstations
                .iter()
                .find(|w| w.id == "custom")
                .unwrap()
                .entries_for(&on_arch)
                .is_empty()
        );
        // What this machine can run.
        let found = suggestions(&gaming.entries_for(&on_arch), &catalog, &arch());
        assert!(found.iter().find(|s| s.id == "steam").unwrap().available);
        let kde = setup.for_desktop(Some(Kind::Kde));
        assert_eq!(kde[0], "dotfiles-konsole");
        assert!(kde.contains(&"plymouth-on".to_string()));
        assert!(
            !setup
                .for_desktop(Some(Kind::Gnome))
                .contains(&"dotfiles-konsole".to_string())
        );
    }
}
