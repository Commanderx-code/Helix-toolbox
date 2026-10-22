//! Home Manager: where its configuration is, the generations it built and
//! which one is active, how the configuration's Git repository stands, and
//! which configuration files it links into the Nix store. Reading changes
//! nothing; building, switching and going back to an earlier generation are
//! plans, reviewed first.
//!
//! The configuration comes from the `[home_manager]` table of
//! `settings.toml`, else the settings Command Center left behind (read
//! only), else `~/.config/home-manager`.

use crate::{Plan, Step, doctor::System, quote};
use helix_toolbox_core::{Risk, printable};
use serde::Serialize;
use std::path::{Path, PathBuf};

/// Where the configuration's settings came from.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Origin {
    /// The `[home_manager]` table of `settings.toml`.
    Settings,
    /// Command Center's settings, read only.
    CommandCenter,
    /// `~/.config/home-manager`, where Home Manager looks by itself.
    Default,
}

impl Origin {
    pub fn label(self) -> &'static str {
        match self {
            Origin::Settings => "Helix Toolbox's settings",
            Origin::CommandCenter => "Command Center's settings",
            Origin::Default => "~/.config/home-manager",
        }
    }
}

/// A configuration Home Manager builds.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Config {
    /// The folder holding `flake.nix` or `home.nix`.
    pub folder: PathBuf,
    /// Whether it is a flake; otherwise `home.nix` is built.
    pub flake: bool,
    /// The flake's `homeConfigurations` name; Home Manager picks
    /// `user@host` or `user` when there is none.
    pub profile: Option<String>,
    pub origin: Origin,
}

impl Config {
    /// The `--flake` argument, for a flake.
    pub fn flake_reference(&self) -> Option<String> {
        self.flake.then(|| match &self.profile {
            Some(profile) => format!("{}#{profile}", self.folder.display()),
            None => self.folder.display().to_string(),
        })
    }
}

/// What is set up, or why nothing can be built.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Found {
    Ready(Config),
    /// A configuration is named, but something about it is wrong.
    Problem(String),
    /// Nothing names one, and `~/.config/home-manager` holds none.
    Missing,
}

/// One generation Home Manager built.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Generation {
    pub id: u32,
    /// When it was built, `YYYY-MM-DD HH:MM`, as Home Manager prints it.
    pub built: String,
    pub path: PathBuf,
    pub current: bool,
}

/// A configuration file and whether Home Manager links it.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Link {
    pub name: String,
    pub path: PathBuf,
    /// Where the link points, when it is a link.
    pub target: Option<PathBuf>,
    /// A link into the Nix store: change it in the configuration.
    pub managed: bool,
}

/// What Home Manager looks like on this machine.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
pub struct Status {
    pub installed: bool,
    pub version: Option<String>,
    pub config: Option<Config>,
    /// Why the configuration cannot be used, when one is named.
    pub problem: Option<String>,
    /// Newest first.
    pub generations: Vec<Generation>,
    /// The Git repository holding the configuration, when it is in one.
    pub repository: Option<crate::repositories::Summary>,
    pub links: Vec<Link>,
}

impl Status {
    pub fn current(&self) -> Option<&Generation> {
        self.generations
            .iter()
            .find(|generation| generation.current)
    }
}

/// The configuration files whose links are shown: those Helix Toolbox
/// edits, and the shell's and the terminal's.
const LINKED: [(&str, &str); 6] = [
    ("Ghostty", ".config/ghostty/config"),
    ("Fastfetch", ".config/fastfetch/config.jsonc"),
    ("fish", ".config/fish/config.fish"),
    ("Git", ".config/git/config"),
    ("kitty", ".config/kitty/kitty.conf"),
    ("Neovim", ".config/nvim/init.lua"),
];

/// Where Command Center kept its settings.
pub fn command_center_settings(home: &Path) -> PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .unwrap_or_else(|| home.join(".config"));
    base.join("io.helixstack.commandcenter/settings.json")
}

/// `~/` made the home folder.
fn expand(home: &Path, value: &str) -> PathBuf {
    match value.strip_prefix("~/") {
        Some(rest) => home.join(rest),
        None if value == "~" => home.to_path_buf(),
        None => PathBuf::from(value),
    }
}

/// A `homeConfigurations` name Home Manager and the shell take as is.
pub fn valid_profile(profile: &str) -> bool {
    !profile.is_empty()
        && profile.len() <= 128
        && !profile.starts_with('-')
        && profile
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"@._-+".contains(&b))
}

/// A folder as a configuration: a flake, `home.nix`, or neither.
fn config_in(
    system: &dyn System,
    folder: PathBuf,
    profile: Option<String>,
    origin: Origin,
) -> Found {
    if !folder.is_absolute() || folder.components().any(|part| part.as_os_str() == "..") {
        return Found::Problem(format!(
            "the configuration folder {} is not a plain absolute path",
            folder.display()
        ));
    }
    if let Some(profile) = &profile
        && !valid_profile(profile)
    {
        return Found::Problem(format!(
            "the profile {:?} is not a name Home Manager takes",
            printable(profile)
        ));
    }
    let flake = system.exists(&folder.join("flake.nix"));
    if !flake && !system.exists(&folder.join("home.nix")) {
        return Found::Problem(format!(
            "{} holds neither flake.nix nor home.nix",
            folder.display()
        ));
    }
    Found::Ready(Config {
        folder,
        flake,
        profile: if flake { profile } else { None },
        origin,
    })
}

/// The configuration, from the `[home_manager]` table (when there is one),
/// Command Center's `settings.json` text (when there is one), or the
/// default folder.
pub fn find(
    system: &dyn System,
    home: &Path,
    settings: Option<&toml::Table>,
    command_center: Option<&str>,
) -> Found {
    let text = |table: &toml::Table, key: &str| {
        table
            .get(key)
            .and_then(toml::Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(String::from)
    };
    if let Some(table) = settings
        && let Some(flake) = text(table, "flake")
    {
        // `folder#profile`, or the folder with `profile` beside it.
        let (folder, profile) = match flake.split_once('#') {
            Some((folder, profile)) => (folder.to_string(), Some(profile.to_string())),
            None => (flake, text(table, "profile")),
        };
        return config_in(system, expand(home, &folder), profile, Origin::Settings);
    }
    if let Some(text) = command_center
        && let Ok(value) = serde_json::from_str::<serde_json::Value>(text)
        && let Some(dotfiles) = value
            .pointer("/integrations/dotfilesPath")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
    {
        let dotfiles = expand(home, dotfiles);
        let nested = dotfiles.join("home-manager");
        let folder = if system.exists(&nested.join("flake.nix")) {
            nested
        } else {
            dotfiles
        };
        let profile = value
            .pointer("/integrations/flakeProfile")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(String::from);
        return config_in(system, folder, profile, Origin::CommandCenter);
    }
    let folder = home.join(".config/home-manager");
    if system.exists(&folder.join("flake.nix")) || system.exists(&folder.join("home.nix")) {
        return config_in(system, folder, None, Origin::Default);
    }
    Found::Missing
}

/// The configuration on this machine.
pub fn here(system: &dyn System) -> Found {
    let (_, _, home) = system.user();
    let table = crate::settings::path()
        .and_then(|path| system.read(&path))
        .and_then(|text| text.parse::<toml::Table>().ok())
        .and_then(|table| table.get("home_manager")?.as_table().cloned());
    let command_center = system.read(&command_center_settings(&home));
    find(system, &home, table.as_ref(), command_center.as_deref())
}

/// `home-manager generations`: `2026-10-20 12:01 : id 201 -> /nix/store/…
/// (current)` a line, newest first. Without the `(current)` mark (older
/// releases), the generation `current` points to is the current one.
pub fn parse_generations(text: &str, current: Option<&Path>) -> Vec<Generation> {
    let mut found: Vec<Generation> = text
        .lines()
        .filter_map(|line| {
            let (built, rest) = line.split_once(" : id ")?;
            let (id, rest) = rest.split_once(" -> ")?;
            let id: u32 = id.trim().parse().ok()?;
            let (path, marked) = match rest.trim().strip_suffix("(current)") {
                Some(path) => (path.trim(), true),
                None => (rest.trim(), false),
            };
            let path = PathBuf::from(path);
            if !store_path(&path) {
                return None;
            }
            Some(Generation {
                id,
                built: printable(built.trim()),
                current: marked,
                path,
            })
        })
        .collect();
    if !found.iter().any(|generation| generation.current)
        && let Some(current) = current
        && let Some(generation) = found
            .iter_mut()
            .find(|generation| generation.path == current)
    {
        generation.current = true;
    }
    found
}

/// The configuration files whose links are shown.
pub fn links(system: &dyn System, home: &Path) -> Vec<Link> {
    LINKED
        .iter()
        .filter_map(|(name, file)| {
            let path = home.join(file);
            let target = system.link(&path);
            if target.is_none() && !system.exists(&path) {
                return None;
            }
            // A link to a link into the store is managed too.
            let managed = target.as_ref().is_some_and(|target| {
                target.starts_with("/nix/store")
                    || system
                        .canonical(&path)
                        .is_some_and(|real| real.starts_with("/nix/store"))
            });
            Some(Link {
                name: name.to_string(),
                path,
                target,
                managed,
            })
        })
        .collect()
}

/// The Git repository a folder is in: the folder or one above it holding
/// `.git`, no higher than the home folder.
pub fn repository_of(system: &dyn System, home: &Path, folder: &Path) -> Option<PathBuf> {
    folder
        .ancestors()
        .take_while(|place| place.starts_with(home) && *place != home)
        .find(|place| system.exists(&place.join(".git")))
        .map(Path::to_path_buf)
}

/// Read everything; `git` answers for the configuration's repository.
pub fn status(system: &dyn System, git: &dyn crate::repositories::Git) -> Status {
    let (_, _, home) = system.user();
    let installed = system.on_path("home-manager");
    let version = installed
        .then(|| system.run(&["home-manager", "--version"]))
        .flatten()
        .filter(|(ok, _)| *ok)
        .map(|(_, text)| printable(text.trim()))
        .filter(|text| !text.is_empty());
    let (config, problem) = match here(system) {
        Found::Ready(config) => (Some(config), None),
        Found::Problem(problem) => (None, Some(problem)),
        Found::Missing => (None, None),
    };
    let current = system.canonical(&home.join(".local/state/home-manager/gcroots/current-home"));
    let generations = installed
        .then(|| system.run(&["home-manager", "generations"]))
        .flatten()
        .filter(|(ok, _)| *ok)
        .map(|(_, text)| parse_generations(&text, current.as_deref()))
        .unwrap_or_default();
    let repository = config
        .as_ref()
        .and_then(|config| repository_of(system, &home, &config.folder))
        .map(|repo| crate::repositories::summary(git, &repo));
    Status {
        installed,
        version,
        config,
        problem,
        generations,
        repository,
        links: links(system, &home),
    }
}

/// A word for the shell: as it is when nothing in it is special there.
fn word(text: &str) -> String {
    if !text.is_empty()
        && text
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"@%+=:,./_-".contains(&b))
    {
        text.to_string()
    } else {
        quote(text)
    }
}

/// The command line, as plans show it and run it.
fn shown(words: &[String]) -> String {
    printable(
        &words
            .iter()
            .map(|text| word(text))
            .collect::<Vec<_>>()
            .join(" "),
    )
}

fn plan_of(id: &str, name: &str, words: Vec<String>, notes: Vec<String>, risk: Risk) -> Plan {
    let line = shown(&words);
    Plan {
        steps: vec![Step::Shell {
            id: id.to_string(),
            name: name.to_string(),
            program: line.clone(),
            display: line,
            requires: vec![words[0].clone()],
            changes: Vec::new(),
        }],
        notes,
        requirements: vec![words[0].clone()],
        privileged: false,
        risk,
        depends: vec![Vec::new()],
        ..Plan::default()
    }
}

/// `home-manager <verb>` of the configuration.
fn command(config: &Config, verb: &str) -> Vec<String> {
    let mut words = vec!["home-manager".to_string(), verb.to_string()];
    match config.flake_reference() {
        Some(reference) => {
            words.push("--flake".to_string());
            words.push(reference);
        }
        None => {
            words.push("-f".to_string());
            words.push(config.folder.join("home.nix").display().to_string());
        }
    }
    words
}

/// Build the configuration without activating it: the result is a `result`
/// link in the folder it runs in, the configuration's.
pub fn plan_build(config: &Config) -> Plan {
    let mut planned = plan_of(
        "home-manager-build",
        "Build the Home Manager configuration",
        command(config, "build"),
        vec![
            "It builds the configuration and leaves a result link; nothing it manages changes."
                .to_string(),
        ],
        Risk::Low,
    );
    if let Some(Step::Shell {
        program, display, ..
    }) = planned.steps.first_mut()
    {
        *program = format!(
            "cd {} && {program}",
            word(&config.folder.display().to_string())
        );
        *display = printable(program);
    }
    planned
}

/// Build and activate the configuration.
pub fn plan_switch(config: &Config) -> Plan {
    plan_of(
        "home-manager-switch",
        "Switch to the Home Manager configuration",
        command(config, "switch"),
        vec![
            "The files, packages and user services it manages change to what the configuration says; the generation before stays, to go back to.".to_string(),
        ],
        Risk::Medium,
    )
}

/// Activate an earlier generation again, through its own `activate`
/// script, as Home Manager's manual says to go back.
pub fn plan_activate(system: &dyn System, generation: &Generation) -> Result<Plan, String> {
    if generation.current {
        return Err(format!("generation {} is the active one", generation.id));
    }
    let activate = generation.path.join("activate");
    // Where the script really is must be in the store too: a link out of
    // it would run something Home Manager did not build.
    let escapes = system
        .canonical(&activate)
        .is_some_and(|real| !real.starts_with("/nix/store"));
    if !store_path(&generation.path) || escapes || !system.exists(&activate) {
        return Err(format!(
            "generation {} has no activate script at {}; it may have been collected",
            generation.id,
            activate.display()
        ));
    }
    let line = word(&activate.display().to_string());
    Ok(Plan {
        steps: vec![Step::Shell {
            id: "home-manager-activate".to_string(),
            name: format!("Go back to Home Manager generation {}", generation.id),
            program: line.clone(),
            display: printable(&line),
            requires: Vec::new(),
            changes: Vec::new(),
        }],
        notes: vec![format!(
            "The files, packages and user services Home Manager manages go back to generation {}, built {}; a later switch builds the configuration again.",
            generation.id, generation.built
        )],
        privileged: false,
        risk: Risk::Medium,
        depends: vec![Vec::new()],
        ..Plan::default()
    })
}

/// A path of the Nix store itself, `/nix/store/<name>`: no `..` or `.` in
/// it and nothing below the store path, so it cannot point elsewhere.
fn store_path(path: &Path) -> bool {
    use std::path::Component;
    let parts: Vec<Component> = path.components().collect();
    matches!(
        parts.as_slice(),
        [Component::RootDir, Component::Normal(nix), Component::Normal(store), Component::Normal(name)]
            if *nix == "nix" && *store == "store" && !name.to_string_lossy().starts_with('.')
    ) && !path.to_string_lossy().contains("/./")
}

/// A generation by its id.
pub fn find_generation(all: &[Generation], id: &str) -> Result<Generation, String> {
    let wanted: u32 = id
        .trim()
        .parse()
        .map_err(|_| format!("{:?} is not a generation number", printable(id)))?;
    all.iter()
        .find(|generation| generation.id == wanted)
        .cloned()
        .ok_or_else(|| format!("Home Manager has no generation {wanted}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[derive(Default)]
    struct Fake {
        files: HashMap<PathBuf, String>,
        links: HashMap<PathBuf, PathBuf>,
        commands: HashMap<String, String>,
        tools: Vec<&'static str>,
    }

    impl System for Fake {
        fn read(&self, path: &Path) -> Option<String> {
            self.files.get(path).cloned()
        }
        fn exists(&self, path: &Path) -> bool {
            self.files.contains_key(path)
                || self.links.contains_key(path)
                || self.files.keys().any(|file| file.starts_with(path))
        }
        fn list(&self, _: &Path) -> Vec<String> {
            Vec::new()
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
            Err("no network".into())
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
        fn on_path(&self, tool: &str) -> bool {
            self.tools.contains(&tool)
        }
        fn processes(&self) -> Vec<String> {
            Vec::new()
        }
    }

    const GENERATIONS: &str = "2026-10-20 12:01 : id 201 -> /nix/store/aaa-home-manager-generation (current)\n2026-10-16 13:23 : id 200 -> /nix/store/bbb-home-manager-generation\nnot a generation\n2026-10-15 03:41 : id 199 -> /tmp/elsewhere\n";

    #[test]
    fn the_generations_are_read_with_the_current_one() {
        let found = parse_generations(GENERATIONS, None);
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].id, 201);
        assert!(found[0].current && !found[1].current);
        assert_eq!(found[1].built, "2026-10-16 13:23");
        // Older releases print no mark: the gcroot says which.
        let unmarked = GENERATIONS.replace(" (current)", "");
        let found = parse_generations(
            &unmarked,
            Some(Path::new("/nix/store/bbb-home-manager-generation")),
        );
        assert!(!found[0].current && found[1].current);
        assert!(find_generation(&found, "200").is_ok());
        assert!(find_generation(&found, "7").is_err());
        assert!(find_generation(&found, "x").is_err());
    }

    #[test]
    fn the_configuration_is_found_from_the_settings_then_command_center_then_the_default() {
        let mut fake = Fake::default();
        let home = Path::new("/home/me");
        fake.files.insert(
            "/home/me/dotfiles/home-manager/flake.nix".into(),
            String::new(),
        );
        fake.files.insert(
            "/home/me/.config/home-manager/home.nix".into(),
            String::new(),
        );
        let table: toml::Table = "flake = \"~/dotfiles/home-manager#me\"\n".parse().unwrap();
        let Found::Ready(config) = find(&fake, home, Some(&table), None) else {
            panic!("no configuration from the settings");
        };
        assert_eq!(config.origin, Origin::Settings);
        assert_eq!(
            config.flake_reference().as_deref(),
            Some("/home/me/dotfiles/home-manager#me")
        );
        let command_center =
            r#"{"integrations": {"dotfilesPath": "~/dotfiles", "flakeProfile": "commander"}}"#;
        let Found::Ready(config) = find(&fake, home, None, Some(command_center)) else {
            panic!("no configuration from Command Center");
        };
        assert_eq!(config.origin, Origin::CommandCenter);
        assert_eq!(config.folder, Path::new("/home/me/dotfiles/home-manager"));
        assert_eq!(config.profile.as_deref(), Some("commander"));
        let Found::Ready(config) = find(&fake, home, None, None) else {
            panic!("no default configuration");
        };
        assert!(!config.flake && config.origin == Origin::Default);
        assert_eq!(
            command(&config, "build"),
            [
                "home-manager",
                "build",
                "-f",
                "/home/me/.config/home-manager/home.nix"
            ]
        );
        for bad in [
            "flake = \"~/dotfiles/home-manager#-rf\"\n",
            "flake = \"~/../etc\"\n",
            "flake = \"~/nothing-here\"\n",
        ] {
            let table: toml::Table = bad.parse().unwrap();
            assert!(
                matches!(find(&fake, home, Some(&table), None), Found::Problem(_)),
                "{bad}"
            );
        }
        fake.files
            .remove(Path::new("/home/me/.config/home-manager/home.nix"));
        assert_eq!(find(&fake, home, None, None), Found::Missing);
    }

    #[test]
    fn plans_build_switch_and_go_back() {
        let mut fake = Fake::default();
        let config = Config {
            folder: "/home/me/my config".into(),
            flake: true,
            profile: Some("me".into()),
            origin: Origin::Settings,
        };
        let Step::Shell { program, .. } = &plan_build(&config).steps[0] else {
            panic!("not a shell step");
        };
        assert_eq!(
            program,
            "cd '/home/me/my config' && home-manager build --flake '/home/me/my config#me'"
        );
        assert_eq!(plan_switch(&config).risk, Risk::Medium);
        let found = parse_generations(GENERATIONS, None);
        assert!(plan_activate(&fake, &found[0]).is_err(), "the active one");
        assert!(plan_activate(&fake, &found[1]).is_err(), "collected");
        // A path that only starts like the store's is no generation.
        for path in [
            "/nix/store/../../tmp/evil",
            "/nix/store/abc-generation/../../../tmp",
            "/nix/store/./abc",
            "/nix/storefront/abc",
            "/nix/store",
        ] {
            assert!(!store_path(Path::new(path)), "{path}");
        }
        assert!(store_path(Path::new(
            "/nix/store/abc-home-manager-generation"
        )));
        let tricked = parse_generations(
            "2026-10-20 12:01 : id 9 -> /nix/store/../../tmp/evil\n",
            None,
        );
        assert!(tricked.is_empty());
        fake.files.insert(
            "/nix/store/bbb-home-manager-generation/activate".into(),
            String::new(),
        );
        let planned = plan_activate(&fake, &found[1]).unwrap();
        let Step::Shell { program, name, .. } = &planned.steps[0] else {
            panic!("not a shell step");
        };
        assert_eq!(program, "/nix/store/bbb-home-manager-generation/activate");
        assert_eq!(name, "Go back to Home Manager generation 200");
    }

    #[test]
    fn links_into_the_store_are_managed() {
        let mut fake = Fake::default();
        fake.links.insert(
            "/home/me/.config/ghostty/config".into(),
            "/nix/store/xyz-home-manager-files/.config/ghostty/config".into(),
        );
        fake.files.insert(
            "/home/me/.config/fastfetch/config.jsonc".into(),
            "{}".into(),
        );
        let found = links(&fake, Path::new("/home/me"));
        assert_eq!(found.len(), 2);
        assert!(found[0].managed && found[0].name == "Ghostty");
        assert!(!found[1].managed && found[1].target.is_none());
    }

    #[test]
    fn the_repository_is_the_nearest_folder_with_git_under_home() {
        let mut fake = Fake::default();
        fake.files
            .insert("/home/me/dotfiles/.git/HEAD".into(), String::new());
        assert_eq!(
            repository_of(
                &fake,
                Path::new("/home/me"),
                Path::new("/home/me/dotfiles/home-manager")
            ),
            Some(PathBuf::from("/home/me/dotfiles"))
        );
        assert_eq!(
            repository_of(
                &fake,
                Path::new("/home/me"),
                Path::new("/home/me/.config/home-manager")
            ),
            None
        );
    }
}
