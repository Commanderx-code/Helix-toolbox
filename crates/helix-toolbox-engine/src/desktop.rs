//! The desktop environment manager: which desktop, session and display
//! manager this is, its appearance settings (read with the desktop's own
//! tools), saved looks to put back, and who else sets those settings: Home
//! Manager, its Plasma and dconf modules, Stylix. Reading changes nothing;
//! applying a look is a plan of the desktop's own commands, confirmed first.

use crate::{Plan, Step, doctor::System, quote};
use helix_toolbox_core::{Risk, printable};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

/// The desktop environments Helix Toolbox knows the settings of.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Kde,
    Gnome,
    Xfce,
    Cinnamon,
    Mate,
    Hyprland,
    Sway,
    Other,
}

impl Kind {
    pub fn id(self) -> &'static str {
        match self {
            Kind::Kde => "kde",
            Kind::Gnome => "gnome",
            Kind::Xfce => "xfce",
            Kind::Cinnamon => "cinnamon",
            Kind::Mate => "mate",
            Kind::Hyprland => "hyprland",
            Kind::Sway => "sway",
            Kind::Other => "other",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Kind::Kde => "KDE Plasma",
            Kind::Gnome => "GNOME",
            Kind::Xfce => "Xfce",
            Kind::Cinnamon => "Cinnamon",
            Kind::Mate => "MATE",
            Kind::Hyprland => "Hyprland",
            Kind::Sway => "Sway",
            Kind::Other => "another desktop",
        }
    }

    /// From `XDG_CURRENT_DESKTOP`, which may list several names.
    pub fn from_desktop(value: &str) -> Option<Self> {
        value
            .split(':')
            .find_map(|name| match name.to_ascii_lowercase().as_str() {
                "kde" | "plasma" => Some(Kind::Kde),
                "gnome" | "gnome-classic" | "ubuntu" | "pop" => Some(Kind::Gnome),
                "xfce" => Some(Kind::Xfce),
                "x-cinnamon" | "cinnamon" => Some(Kind::Cinnamon),
                "mate" => Some(Kind::Mate),
                "hyprland" => Some(Kind::Hyprland),
                "sway" => Some(Kind::Sway),
                _ => None,
            })
    }

    /// The process that shows this desktop is running.
    fn process(self) -> &'static str {
        match self {
            Kind::Kde => "plasmashell",
            Kind::Gnome => "gnome-shell",
            Kind::Xfce => "xfce4-session",
            Kind::Cinnamon => "cinnamon",
            Kind::Mate => "mate-session",
            Kind::Hyprland => "Hyprland",
            Kind::Sway => "sway",
            Kind::Other => "",
        }
    }
}

/// One appearance setting: what it is, how it is read and set.
struct Setting {
    key: &'static str,
    label: &'static str,
    read: &'static [&'static str],
    /// The command that sets it, before the value.
    apply: &'static [&'static str],
}

const KDE: &[Setting] = &[
    Setting {
        key: "color-scheme",
        label: "Color scheme",
        read: &[
            "kreadconfig6",
            "--file",
            "kdeglobals",
            "--group",
            "General",
            "--key",
            "ColorScheme",
        ],
        apply: &["plasma-apply-colorscheme"],
    },
    Setting {
        key: "plasma-theme",
        label: "Plasma style",
        read: &[
            "kreadconfig6",
            "--file",
            "plasmarc",
            "--group",
            "Theme",
            "--key",
            "name",
        ],
        apply: &["plasma-apply-desktoptheme"],
    },
    Setting {
        key: "icons",
        label: "Icons",
        read: &[
            "kreadconfig6",
            "--file",
            "kdeglobals",
            "--group",
            "Icons",
            "--key",
            "Theme",
        ],
        apply: &[
            "kwriteconfig6",
            "--file",
            "kdeglobals",
            "--group",
            "Icons",
            "--key",
            "Theme",
        ],
    },
    Setting {
        key: "cursor",
        label: "Cursor",
        read: &[
            "kreadconfig6",
            "--file",
            "kcminputrc",
            "--group",
            "Mouse",
            "--key",
            "cursorTheme",
        ],
        apply: &["plasma-apply-cursortheme"],
    },
    Setting {
        key: "global-theme",
        label: "Global theme",
        read: &[
            "kreadconfig6",
            "--file",
            "kdeglobals",
            "--group",
            "KDE",
            "--key",
            "LookAndFeelPackage",
        ],
        apply: &["plasma-apply-lookandfeel", "--apply"],
    },
    Setting {
        key: "font",
        label: "Font",
        read: &[
            "kreadconfig6",
            "--file",
            "kdeglobals",
            "--group",
            "General",
            "--key",
            "font",
        ],
        apply: &[
            "kwriteconfig6",
            "--file",
            "kdeglobals",
            "--group",
            "General",
            "--key",
            "font",
        ],
    },
];

const GNOME: &[Setting] = &[
    Setting {
        key: "gtk-theme",
        label: "Applications",
        read: &[
            "gsettings",
            "get",
            "org.gnome.desktop.interface",
            "gtk-theme",
        ],
        apply: &[
            "gsettings",
            "set",
            "org.gnome.desktop.interface",
            "gtk-theme",
        ],
    },
    Setting {
        key: "color-scheme",
        label: "Style",
        read: &[
            "gsettings",
            "get",
            "org.gnome.desktop.interface",
            "color-scheme",
        ],
        apply: &[
            "gsettings",
            "set",
            "org.gnome.desktop.interface",
            "color-scheme",
        ],
    },
    Setting {
        key: "icons",
        label: "Icons",
        read: &[
            "gsettings",
            "get",
            "org.gnome.desktop.interface",
            "icon-theme",
        ],
        apply: &[
            "gsettings",
            "set",
            "org.gnome.desktop.interface",
            "icon-theme",
        ],
    },
    Setting {
        key: "cursor",
        label: "Cursor",
        read: &[
            "gsettings",
            "get",
            "org.gnome.desktop.interface",
            "cursor-theme",
        ],
        apply: &[
            "gsettings",
            "set",
            "org.gnome.desktop.interface",
            "cursor-theme",
        ],
    },
    Setting {
        key: "font",
        label: "Font",
        read: &[
            "gsettings",
            "get",
            "org.gnome.desktop.interface",
            "font-name",
        ],
        apply: &[
            "gsettings",
            "set",
            "org.gnome.desktop.interface",
            "font-name",
        ],
    },
];

const CINNAMON: &[Setting] = &[
    Setting {
        key: "gtk-theme",
        label: "Applications",
        read: &[
            "gsettings",
            "get",
            "org.cinnamon.desktop.interface",
            "gtk-theme",
        ],
        apply: &[
            "gsettings",
            "set",
            "org.cinnamon.desktop.interface",
            "gtk-theme",
        ],
    },
    Setting {
        key: "desktop-theme",
        label: "Desktop",
        read: &["gsettings", "get", "org.cinnamon.theme", "name"],
        apply: &["gsettings", "set", "org.cinnamon.theme", "name"],
    },
    Setting {
        key: "icons",
        label: "Icons",
        read: &[
            "gsettings",
            "get",
            "org.cinnamon.desktop.interface",
            "icon-theme",
        ],
        apply: &[
            "gsettings",
            "set",
            "org.cinnamon.desktop.interface",
            "icon-theme",
        ],
    },
    Setting {
        key: "cursor",
        label: "Cursor",
        read: &[
            "gsettings",
            "get",
            "org.cinnamon.desktop.interface",
            "cursor-theme",
        ],
        apply: &[
            "gsettings",
            "set",
            "org.cinnamon.desktop.interface",
            "cursor-theme",
        ],
    },
];

const MATE: &[Setting] = &[
    Setting {
        key: "gtk-theme",
        label: "Applications",
        read: &["gsettings", "get", "org.mate.interface", "gtk-theme"],
        apply: &["gsettings", "set", "org.mate.interface", "gtk-theme"],
    },
    Setting {
        key: "icons",
        label: "Icons",
        read: &["gsettings", "get", "org.mate.interface", "icon-theme"],
        apply: &["gsettings", "set", "org.mate.interface", "icon-theme"],
    },
    Setting {
        key: "cursor",
        label: "Cursor",
        read: &[
            "gsettings",
            "get",
            "org.mate.peripherals-mouse",
            "cursor-theme",
        ],
        apply: &[
            "gsettings",
            "set",
            "org.mate.peripherals-mouse",
            "cursor-theme",
        ],
    },
];

const XFCE: &[Setting] = &[
    Setting {
        key: "gtk-theme",
        label: "Style",
        read: &["xfconf-query", "-c", "xsettings", "-p", "/Net/ThemeName"],
        apply: &[
            "xfconf-query",
            "-c",
            "xsettings",
            "-p",
            "/Net/ThemeName",
            "-s",
        ],
    },
    Setting {
        key: "window-theme",
        label: "Window manager",
        read: &["xfconf-query", "-c", "xfwm4", "-p", "/general/theme"],
        apply: &["xfconf-query", "-c", "xfwm4", "-p", "/general/theme", "-s"],
    },
    Setting {
        key: "icons",
        label: "Icons",
        read: &[
            "xfconf-query",
            "-c",
            "xsettings",
            "-p",
            "/Net/IconThemeName",
        ],
        apply: &[
            "xfconf-query",
            "-c",
            "xsettings",
            "-p",
            "/Net/IconThemeName",
            "-s",
        ],
    },
    Setting {
        key: "cursor",
        label: "Cursor",
        read: &[
            "xfconf-query",
            "-c",
            "xsettings",
            "-p",
            "/Gtk/CursorThemeName",
        ],
        apply: &[
            "xfconf-query",
            "-c",
            "xsettings",
            "-p",
            "/Gtk/CursorThemeName",
            "-s",
        ],
    },
    Setting {
        key: "font",
        label: "Font",
        read: &["xfconf-query", "-c", "xsettings", "-p", "/Gtk/FontName"],
        apply: &[
            "xfconf-query",
            "-c",
            "xsettings",
            "-p",
            "/Gtk/FontName",
            "-s",
        ],
    },
];

fn settings_of(kind: Kind) -> &'static [Setting] {
    match kind {
        Kind::Kde => KDE,
        Kind::Gnome => GNOME,
        Kind::Cinnamon => CINNAMON,
        Kind::Mate => MATE,
        Kind::Xfce => XFCE,
        Kind::Hyprland | Kind::Sway | Kind::Other => &[],
    }
}

/// What the machine's desktop is.
#[derive(Clone, Debug, Default, Serialize)]
pub struct Desktop {
    pub kind: Option<Kind>,
    /// "KDE Plasma 6.8.0".
    pub name: Option<String>,
    /// wayland or x11.
    pub session: Option<String>,
    pub display_manager: Option<DisplayManager>,
    /// The appearance settings, in the desktop's order, with their values.
    pub appearance: Vec<Value>,
    /// Who else sets these settings.
    pub owners: Vec<String>,
    /// Whether Helix Toolbox would refuse to change the settings, and why.
    pub blocked: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct Value {
    pub key: String,
    pub label: String,
    pub value: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct DisplayManager {
    /// sddm, gdm, lightdm.
    pub name: String,
    /// Its configuration files that exist here.
    pub files: Vec<String>,
    /// The login screen theme, where it can be read.
    pub theme: Option<String>,
}

/// Read the desktop. `environment` answers for `XDG_*` variables.
pub fn detect(system: &dyn System, environment: &dyn Fn(&str) -> Option<String>) -> Desktop {
    let processes = system.processes();
    let kind = environment("XDG_CURRENT_DESKTOP")
        .and_then(|value| Kind::from_desktop(&value))
        .or_else(|| {
            [
                Kind::Kde,
                Kind::Gnome,
                Kind::Xfce,
                Kind::Cinnamon,
                Kind::Mate,
                Kind::Hyprland,
                Kind::Sway,
            ]
            .into_iter()
            .find(|kind| processes.iter().any(|name| name == kind.process()))
        });
    let version = |command: &[&str]| -> Option<String> {
        let (true, text) = system.run(command)? else {
            return None;
        };
        text.split_whitespace()
            .find(|word| word.chars().next().is_some_and(|c| c.is_ascii_digit()))
            .map(printable)
    };
    let name = kind.map(|kind| {
        let found = match kind {
            Kind::Kde => version(&["plasmashell", "--version"]),
            Kind::Gnome => version(&["gnome-shell", "--version"]),
            Kind::Xfce => version(&["xfce4-session", "--version"]),
            Kind::Cinnamon => version(&["cinnamon", "--version"]),
            Kind::Mate => version(&["mate-session", "--version"]),
            _ => None,
        };
        match found {
            Some(version) => format!("{} {version}", kind.label()),
            None => kind.label().to_string(),
        }
    });
    let appearance = kind
        .map(|kind| {
            settings_of(kind)
                .iter()
                .map(|setting| Value {
                    key: setting.key.to_string(),
                    label: setting.label.to_string(),
                    value: system
                        .run(setting.read)
                        .filter(|(ok, _)| *ok)
                        .map(|(_, text)| unquote(text.trim()))
                        .filter(|text| !text.is_empty()),
                })
                .collect()
        })
        .unwrap_or_default();
    let (owners, blocked) = owners(system, kind);
    Desktop {
        kind,
        name,
        session: environment("XDG_SESSION_TYPE").map(|value| printable(&value)),
        display_manager: display_manager(system),
        appearance,
        owners,
        blocked,
    }
}

/// gsettings prints strings in quotes.
fn unquote(text: &str) -> String {
    printable(
        text.strip_prefix('\'')
            .and_then(|rest| rest.strip_suffix('\''))
            .unwrap_or(text),
    )
}

fn display_manager(system: &dyn System) -> Option<DisplayManager> {
    let target = system.link(Path::new("/etc/systemd/system/display-manager.service"))?;
    let name = target.file_stem()?.to_string_lossy().into_owned();
    let mut files: Vec<String> = match name.as_str() {
        "sddm" => {
            let mut files = vec!["/etc/sddm.conf".to_string()];
            files.extend(
                system
                    .list(Path::new("/etc/sddm.conf.d"))
                    .into_iter()
                    .map(|file| format!("/etc/sddm.conf.d/{file}")),
            );
            files
        }
        "gdm" | "gdm3" => vec![
            "/etc/gdm/custom.conf".into(),
            "/etc/gdm3/custom.conf".into(),
        ],
        "lightdm" => vec![
            "/etc/lightdm/lightdm.conf".into(),
            "/etc/lightdm/lightdm-gtk-greeter.conf".into(),
        ],
        _ => Vec::new(),
    };
    files.retain(|file| system.exists(Path::new(file)));
    // SDDM's theme: the last Current= in the files, read in order.
    let theme = (name == "sddm")
        .then(|| {
            files
                .iter()
                .filter_map(|file| system.read(Path::new(file)))
                .flat_map(|text| {
                    text.lines()
                        .filter_map(|line| line.trim().strip_prefix("Current="))
                        .map(|value| printable(value.trim()))
                        .collect::<Vec<_>>()
                })
                .last()
        })
        .flatten();
    Some(DisplayManager {
        name: printable(&name),
        files: files.iter().map(|file| printable(file)).collect(),
        theme,
    })
}

/// Who else sets the desktop's appearance, and whether that rules out
/// changing it here: a settings file that is a Home Manager link would be
/// written through into the Nix store, or replaced at the next switch.
fn owners(system: &dyn System, kind: Option<Kind>) -> (Vec<String>, Option<String>) {
    let (_, _, home) = system.user();
    let mut owners = Vec::new();
    let mut blocked = None;
    let files: &[&str] = match kind {
        Some(Kind::Kde) => &[
            ".config/kdeglobals",
            ".config/plasmarc",
            ".config/kcminputrc",
        ],
        Some(Kind::Xfce) => &[".config/xfce4/xfconf/xfce-perchannel-xml/xsettings.xml"],
        Some(Kind::Gnome | Kind::Cinnamon | Kind::Mate) => &[".config/dconf/user"],
        _ => &[],
    };
    for file in files {
        let path = home.join(file);
        if let Some(target) = system.link(&path)
            && target.starts_with("/nix/store")
        {
            blocked = Some(format!(
                "~/{file} is a Home Manager link into the Nix store; change the look in your Home Manager configuration instead"
            ));
        }
    }
    let state = home.join(".local/state/home-manager/gcroots/current-home");
    if let Some(generation) = system.canonical(&state)
        && let Some(activate) = system.read(&generation.join("activate"))
    {
        let script = activate.to_lowercase();
        if script.contains("stylix") {
            owners.push(
                "Stylix themes this desktop through Home Manager at every switch".to_string(),
            );
        }
        match kind {
            Some(Kind::Kde) if script.contains("plasma") => owners.push(
                "Home Manager runs a Plasma configuration step at every switch (plasma-manager or your own), which may set these again".to_string(),
            ),
            Some(Kind::Gnome | Kind::Cinnamon | Kind::Mate) if script.contains("dconf") => owners.push(
                "Home Manager loads dconf settings at every switch, which may set these again".to_string(),
            ),
            _ => {}
        }
    }
    (owners, blocked)
}

/// A saved look: a desktop's appearance settings, to put back later.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct Look {
    pub name: String,
    pub desktop: String,
    #[serde(default)]
    pub settings: BTreeMap<String, String>,
}

/// Why a look cannot be used.
#[derive(Debug, thiserror::Error)]
pub enum LookError {
    #[error("cannot read {0}: {1}")]
    Read(PathBuf, String),
    #[error("{0} is not a Helix Toolbox look: {1}")]
    Format(PathBuf, String),
    #[error(
        "the look has a name or value Helix Toolbox does not set: control characters, a leading dash or too long"
    )]
    Invalid,
    #[error("the name {0:?} gives no usable file name; use letters or digits")]
    Name(String),
    #[error("cannot write {0}: {1}")]
    Write(PathBuf, String),
}

/// `$XDG_CONFIG_HOME/helix-toolbox/looks`.
pub fn dir() -> Option<PathBuf> {
    crate::profiles::dir().and_then(|profiles| Some(profiles.parent()?.join("looks")))
}

/// The current appearance as a look.
pub fn capture(name: &str, desktop: &Desktop) -> Option<Look> {
    let kind = desktop.kind?;
    let settings: BTreeMap<String, String> = desktop
        .appearance
        .iter()
        .filter_map(|value| Some((value.key.clone(), value.value.clone()?)))
        .collect();
    (!settings.is_empty()).then(|| Look {
        name: name.trim().to_string(),
        desktop: kind.id().to_string(),
        settings,
    })
}

/// A value Helix Toolbox passes to a desktop's tool: plain text that cannot be
/// taken for an option.
fn valid_value(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 200
        && !value.starts_with('-')
        && !value.chars().any(char::is_control)
}

pub fn check(look: &Look) -> Result<(), LookError> {
    let plain = look.name.len() <= 100 && !look.name.chars().any(char::is_control);
    let keys = look.settings.keys().all(|key| {
        key.len() <= 32
            && key
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte == b'-')
    });
    if plain
        && keys
        && look.settings.values().all(|value| valid_value(value))
        && helix_toolbox_core::valid_id(&look.desktop)
    {
        Ok(())
    } else {
        Err(LookError::Invalid)
    }
}

pub fn read(path: &Path) -> Result<Look, LookError> {
    let text = std::fs::read_to_string(path)
        .map_err(|error| LookError::Read(path.to_path_buf(), error.to_string()))?;
    let look: Look = toml::from_str(&text)
        .map_err(|error| LookError::Format(path.to_path_buf(), printable(error.message())))?;
    check(&look)?;
    Ok(look)
}

pub fn save(directory: &Path, look: &Look) -> Result<PathBuf, LookError> {
    check(look)?;
    let slug =
        crate::profiles::slug(&look.name).ok_or_else(|| LookError::Name(look.name.clone()))?;
    let path = directory.join(format!("{slug}.toml"));
    let failed = |error: String| LookError::Write(path.clone(), error);
    std::fs::create_dir_all(directory).map_err(|error| failed(error.to_string()))?;
    let text = toml::to_string(look).map_err(|error| failed(error.to_string()))?;
    let staging = path.with_extension("toml.new");
    std::fs::write(
        &staging,
        format!("# A Helix Toolbox desktop look: helix-toolbox desktop apply <name>\n{text}"),
    )
    .map_err(|error| failed(error.to_string()))?;
    std::fs::rename(&staging, &path).map_err(|error| failed(error.to_string()))?;
    Ok(path)
}

/// The saved looks, by name, and the files that could not be read.
pub fn load_all(directory: &Path) -> (Vec<(String, Look)>, Vec<String>) {
    let mut found = Vec::new();
    let mut problems = Vec::new();
    let Ok(listing) = std::fs::read_dir(directory) else {
        return (found, problems);
    };
    let mut paths: Vec<PathBuf> = listing
        .flatten()
        .map(|item| item.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "toml")
        })
        .collect();
    paths.sort();
    for path in paths {
        let Some(slug) = path
            .file_stem()
            .and_then(|stem| crate::profiles::slug(&stem.to_string_lossy()))
        else {
            continue;
        };
        match read(&path) {
            Ok(look) => found.push((slug, look)),
            Err(error) => problems.push(error.to_string()),
        }
    }
    (found, problems)
}

/// A plan that puts a look back with the desktop's own tools, the settings
/// that differ only. Refused for another desktop, or where Home Manager owns
/// the settings files.
pub fn plan_apply(look: &Look, desktop: &Desktop) -> Result<Plan, String> {
    check(look).map_err(|error| error.to_string())?;
    let Some(kind) = desktop.kind else {
        return Err("no desktop environment is running here".to_string());
    };
    if look.desktop != kind.id() {
        return Err(format!(
            "the look was saved on {}; this is {}",
            printable(&look.desktop),
            kind.label()
        ));
    }
    if let Some(why) = &desktop.blocked {
        return Err(why.clone());
    }
    let mut lines = Vec::new();
    let mut commands = Vec::new();
    let mut shown = Vec::new();
    let mut requires: Vec<String> = Vec::new();
    for setting in settings_of(kind) {
        let Some(value) = look.settings.get(setting.key) else {
            continue;
        };
        let now = desktop
            .appearance
            .iter()
            .find(|current| current.key == setting.key)
            .and_then(|current| current.value.as_deref());
        if now == Some(value.as_str()) {
            continue;
        }
        let mut words: Vec<String> = setting.apply.iter().map(|word| word.to_string()).collect();
        // gsettings takes a GVariant: a string in quotes.
        words.push(if setting.apply[0] == "gsettings" {
            format!("'{}'", value.replace('\'', ""))
        } else {
            value.clone()
        });
        lines.push(
            words
                .iter()
                .map(|word| quote(word))
                .collect::<Vec<_>>()
                .join(" "),
        );
        commands.push(printable(&words.join(" ")));
        shown.push(printable(&format!(
            "{}: {} → {value}",
            setting.label,
            now.unwrap_or("unset")
        )));
        if !requires.contains(&words[0]) {
            requires.push(words[0].clone());
        }
    }
    let mut plan = Plan::default();
    if lines.is_empty() {
        plan.notes.push(format!(
            "{}: the desktop already looks like this",
            printable(&look.name)
        ));
        return Ok(plan);
    }
    requires.sort();
    plan.steps.push(Step::Shell {
        id: "desktop-look".to_string(),
        name: format!("Apply the look {}", printable(&look.name)),
        program: lines.join(" &&\n"),
        display: commands.join(" && "),
        requires: requires.clone(),
        changes: Vec::new(),
    });
    plan.notes.extend(shown);
    plan.notes
        .extend(desktop.owners.iter().map(|owner| format!("Note: {owner}")));
    plan.requirements = requires;
    plan.risk = Risk::Low;
    plan.depends = vec![Vec::new()];
    Ok(plan)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[derive(Default)]
    struct Fake {
        commands: HashMap<String, String>,
        links: HashMap<PathBuf, PathBuf>,
        files: HashMap<PathBuf, String>,
        processes: Vec<String>,
    }

    impl System for Fake {
        fn read(&self, path: &Path) -> Option<String> {
            self.files.get(path).cloned()
        }
        fn exists(&self, path: &Path) -> bool {
            self.files.contains_key(path)
        }
        fn list(&self, path: &Path) -> Vec<String> {
            self.files
                .keys()
                .filter(|known| known.parent() == Some(path))
                .filter_map(|known| known.file_name())
                .map(|name| name.to_string_lossy().into_owned())
                .collect()
        }
        fn link(&self, path: &Path) -> Option<PathBuf> {
            self.links.get(path).cloned()
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
        fn is_link(&self, path: &Path) -> bool {
            self.links.contains_key(path)
        }
        fn canonical(&self, path: &Path) -> Option<PathBuf> {
            self.links.get(path).cloned()
        }
        fn user(&self) -> (u32, u32, PathBuf) {
            (1000, 1000, PathBuf::from("/home/me"))
        }
        fn on_path(&self, _: &str) -> bool {
            true
        }
        fn processes(&self) -> Vec<String> {
            self.processes.clone()
        }
    }

    fn plasma() -> Fake {
        let mut fake = Fake::default();
        fake.commands
            .insert("plasmashell --version".into(), "plasmashell 6.8.0\n".into());
        fake.commands.insert(
            "kreadconfig6 --file kdeglobals --group General --key ColorScheme".into(),
            "BreezeDark\n".into(),
        );
        fake.commands.insert(
            "kreadconfig6 --file kdeglobals --group Icons --key Theme".into(),
            "breeze-dark\n".into(),
        );
        fake.links.insert(
            PathBuf::from("/etc/systemd/system/display-manager.service"),
            PathBuf::from("/usr/lib/systemd/system/sddm.service"),
        );
        fake.files
            .insert(PathBuf::from("/etc/sddm.conf"), "".into());
        fake.files.insert(
            PathBuf::from("/etc/sddm.conf.d/zz-commander-theme.conf"),
            "[Theme]\nCurrent=silent\n".into(),
        );
        fake
    }

    fn environment(name: &str) -> Option<String> {
        match name {
            "XDG_CURRENT_DESKTOP" => Some("KDE".into()),
            "XDG_SESSION_TYPE" => Some("wayland".into()),
            _ => None,
        }
    }

    #[test]
    fn plasma_is_read_with_its_own_tools() {
        let found = detect(&plasma(), &environment);
        assert_eq!(found.kind, Some(Kind::Kde));
        assert_eq!(found.name.as_deref(), Some("KDE Plasma 6.8.0"));
        assert_eq!(found.session.as_deref(), Some("wayland"));
        let dm = found.display_manager.as_ref().unwrap();
        assert_eq!(dm.name, "sddm");
        assert_eq!(dm.theme.as_deref(), Some("silent"));
        assert_eq!(found.appearance[0].value.as_deref(), Some("BreezeDark"));
        assert!(found.appearance.iter().any(|value| value.value.is_none()));
        assert!(found.owners.is_empty());
        assert!(found.blocked.is_none());
        // Without the variable, the running shell tells.
        let mut fake = plasma();
        fake.processes = vec!["plasmashell".into()];
        assert_eq!(detect(&fake, &|_| None).kind, Some(Kind::Kde));
        assert_eq!(Kind::from_desktop("ubuntu:GNOME"), Some(Kind::Gnome));
    }

    #[test]
    fn a_look_is_saved_and_put_back_with_only_what_differs() {
        let desktop = detect(&plasma(), &environment);
        let mut look = capture("Dark", &desktop).unwrap();
        assert_eq!(look.desktop, "kde");
        assert_eq!(look.settings.len(), 2);
        let directory = tempfile::tempdir().unwrap();
        let path = save(directory.path(), &look).unwrap();
        assert_eq!(read(&path).unwrap(), look);
        assert_eq!(load_all(directory.path()).0.len(), 1);
        // The same look changes nothing.
        let planned = plan_apply(&look, &desktop).unwrap();
        assert!(planned.steps.is_empty());
        look.settings.insert("color-scheme".into(), "Sweet".into());
        let planned = plan_apply(&look, &desktop).unwrap();
        assert_eq!(planned.steps.len(), 1);
        assert_eq!(planned.steps[0].display(), "plasma-apply-colorscheme Sweet");
        assert_eq!(planned.notes[0], "Color scheme: BreezeDark → Sweet");
        assert!(
            planned
                .requirements
                .contains(&"plasma-apply-colorscheme".to_string())
        );
        // Another desktop's look, or an option smuggled in, is refused.
        let gnome = Look {
            desktop: "gnome".into(),
            ..look.clone()
        };
        assert!(
            plan_apply(&gnome, &desktop)
                .unwrap_err()
                .contains("saved on gnome; this is KDE Plasma")
        );
        look.settings.insert("icons".into(), "--help".into());
        assert!(plan_apply(&look, &desktop).is_err());
    }

    #[test]
    fn home_manager_owners_are_named_and_its_links_block_changes() {
        let mut fake = plasma();
        fake.links.insert(
            PathBuf::from("/home/me/.local/state/home-manager/gcroots/current-home"),
            PathBuf::from("/nix/store/abc-home-manager-generation"),
        );
        fake.files.insert(
            PathBuf::from("/nix/store/abc-home-manager-generation/activate"),
            "_iNote \"Activating %s\" \"configure-plasma\"\n".into(),
        );
        let found = detect(&fake, &environment);
        assert_eq!(found.owners.len(), 1);
        assert!(found.owners[0].contains("Plasma"));
        assert!(found.blocked.is_none());
        fake.links.insert(
            PathBuf::from("/home/me/.config/kdeglobals"),
            PathBuf::from("/nix/store/xyz-home-manager-files/.config/kdeglobals"),
        );
        let found = detect(&fake, &environment);
        assert!(found.blocked.as_ref().unwrap().contains("kdeglobals"));
        let look = Look {
            name: "x".into(),
            desktop: "kde".into(),
            settings: [("color-scheme".to_string(), "Sweet".to_string())].into(),
        };
        assert!(
            plan_apply(&look, &found)
                .unwrap_err()
                .contains("Home Manager")
        );
    }
}
