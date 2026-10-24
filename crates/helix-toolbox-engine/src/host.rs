//! The host: the machine Helix Toolbox reads, as one value. It holds what
//! "this machine" means to the engine's features: what its files and
//! commands say (a `doctor::System`), the user's home, configuration and
//! state folders, the environment and the clock. `Host::here()` is the
//! machine Helix Toolbox runs on; `Host::made_up()` is one a test describes,
//! which reads nothing of the real home, `/sys` or environment.
//!
//! Features keep taking plain parts for their own tests; their `of` and
//! `_on` forms build those parts from a host, so every interface reads the
//! machine the same way.

use crate::{
    backups,
    doctor::{self, System},
    history,
    preview::{self, Pinned},
    recovery,
};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
};

/// The machine Helix Toolbox reads: this one, or one a test made up.
#[derive(Clone)]
pub struct Host {
    system: Arc<dyn doctor::System + Send + Sync>,
    home: PathBuf,
    /// `$XDG_CONFIG_HOME`, or `~/.config`.
    config: Option<PathBuf>,
    /// Helix Toolbox's state folder: the history, logs and recorded boots.
    state: Option<PathBuf>,
    kind: Kind,
}

#[derive(Clone)]
enum Kind {
    Here,
    MadeUp {
        variables: HashMap<String, String>,
        now: u64,
    },
}

/// When a made-up host's clock stands, unless the test sets it: 2026-10-24.
const MADE_UP_NOW: u64 = 1_792_800_000;

impl Host {
    /// The machine Helix Toolbox runs on.
    pub fn here() -> Self {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_default();
        let config = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
            .or_else(|| (!home.as_os_str().is_empty()).then(|| home.join(".config")));
        Host {
            system: Arc::new(doctor::Here),
            home,
            config,
            state: history::state_dir(),
            kind: Kind::Here,
        }
    }

    /// A machine that answers as `fake` says, with its home, no
    /// configuration or state folder until a test gives one, an empty
    /// environment and a clock that stands still.
    pub fn made_up(fake: doctor::Fake) -> Self {
        Host {
            home: fake.home.clone(),
            system: Arc::new(fake),
            config: None,
            state: None,
            kind: Kind::MadeUp {
                variables: HashMap::new(),
                now: MADE_UP_NOW,
            },
        }
    }

    /// The same, with this configuration folder (`$XDG_CONFIG_HOME`).
    pub fn with_config(mut self, folder: impl Into<PathBuf>) -> Self {
        self.config = Some(folder.into());
        self
    }

    /// The same, with this state folder, where the history is.
    pub fn with_state(mut self, folder: impl Into<PathBuf>) -> Self {
        self.state = Some(folder.into());
        self
    }

    /// A made-up host with this environment variable set.
    pub fn with_variable(mut self, name: &str, value: &str) -> Self {
        if let Kind::MadeUp { variables, .. } = &mut self.kind {
            variables.insert(name.to_string(), value.to_string());
        }
        self
    }

    /// A made-up host whose clock says this, in seconds since 1970.
    pub fn at(mut self, seconds: u64) -> Self {
        if let Kind::MadeUp { now, .. } = &mut self.kind {
            *now = seconds;
        }
        self
    }

    /// Whether this is the machine Helix Toolbox runs on.
    pub fn is_here(&self) -> bool {
        matches!(self.kind, Kind::Here)
    }

    pub fn home(&self) -> &Path {
        &self.home
    }

    /// `$XDG_CONFIG_HOME`, or `~/.config`.
    pub fn config(&self) -> Option<&Path> {
        self.config.as_deref()
    }

    /// Helix Toolbox's state folder: the history, logs and recorded boots.
    pub fn state(&self) -> Option<&Path> {
        self.state.as_deref()
    }

    /// Where Drift baselines are saved.
    pub fn baselines(&self) -> Option<PathBuf> {
        self.config
            .as_ref()
            .map(|config| config.join("helix-toolbox/baselines"))
    }

    /// The user's systemd units folder.
    pub fn units(&self) -> Option<PathBuf> {
        match self.kind {
            Kind::Here => crate::restic::units_here(),
            Kind::MadeUp { .. } => self
                .config
                .as_ref()
                .map(|config| config.join("systemd/user")),
        }
    }

    /// An environment variable.
    pub fn variable(&self, name: &str) -> Option<String> {
        match &self.kind {
            Kind::Here => std::env::var(name).ok(),
            Kind::MadeUp { variables, .. } => variables.get(name).cloned(),
        }
    }

    /// Whether the user is here over SSH, where stopping the network or
    /// the login screen costs more.
    pub fn ssh(&self) -> bool {
        self.variable("SSH_CONNECTION").is_some()
    }

    /// The desktop the session says it is (`XDG_CURRENT_DESKTOP`).
    pub fn desktop(&self) -> Option<String> {
        self.variable("XDG_CURRENT_DESKTOP")
    }

    /// Now, in seconds since 1970.
    pub fn now(&self) -> u64 {
        match &self.kind {
            Kind::Here => history::now(),
            Kind::MadeUp { now, .. } => *now,
        }
    }

    /// The runs recorded here, newest first, at most `limit`.
    pub fn history(&self, limit: usize) -> Vec<history::Record> {
        self.state
            .as_deref()
            .and_then(|state| history::read_in(state, limit).ok())
            .unwrap_or_default()
    }

    /// Where a command is on the PATH; on a made-up host, under
    /// `/usr/bin` when the fake lists it.
    pub fn which(&self, tool: &str) -> Option<PathBuf> {
        match self.kind {
            Kind::Here => crate::inventory::which(tool),
            Kind::MadeUp { .. } => self.on_path(tool).then(|| Path::new("/usr/bin").join(tool)),
        }
    }

    /// Run one of the installers' renderers from `scripts`, which only
    /// print; a made-up host runs none.
    pub fn render(&self, scripts: &Path, arguments: &[String]) -> Result<String, String> {
        match self.kind {
            Kind::Here => preview::render_with(scripts, arguments),
            Kind::MadeUp { .. } => Err("a made-up machine runs no renderer".to_string()),
        }
    }

    /// Where a pinned source of `scripts` is downloaded here: on a made-up
    /// host, under its home's `.local/share`.
    pub fn pinned(&self, scripts: &Path, name: &str) -> Pinned {
        match self.kind {
            Kind::Here => preview::pinned_source(scripts, name),
            Kind::MadeUp { .. } => preview::pinned_in(
                scripts,
                &self.home.join(".local/share/helix-toolbox/sources"),
                name,
            ),
        }
    }

    /// The restic repository Helix Toolbox backs up to, by name: listing
    /// its snapshots may ask for a password, so the newest is not read.
    pub fn restic(&self) -> Option<recovery::Repository> {
        match self.kind {
            Kind::Here => crate::restic::repository_here(),
            Kind::MadeUp { .. } => None,
        }
    }

    /// Where configuration backups are looked for: on a made-up host, its
    /// configuration folder, its home and the folders under it only.
    pub fn backup_roots(&self) -> backups::Roots {
        match self.kind {
            Kind::Here => backups::Roots::here(),
            Kind::MadeUp { .. } => backups::Roots {
                user: [
                    self.config
                        .clone()
                        .unwrap_or_else(|| self.home.join(".config")),
                    self.home.join(".local/share"),
                    self.home.join(".local/bin"),
                ]
                .into(),
                home: Some(self.home.clone()),
                system: Vec::new(),
                beside: Vec::new(),
            },
        }
    }

    /// The snapshot tool set up here, if any.
    pub fn snapshots(&self) -> Option<backups::Snapshots> {
        recovery::snapshot_tool(self)
    }

    /// The configuration backups found here, newest first.
    pub fn backups(&self) -> Vec<backups::Backup> {
        backups::discover(&self.backup_roots())
    }

    /// The saved Drift baselines, newest first.
    pub fn saved_baselines(&self) -> Vec<(String, crate::drift::Baseline)> {
        self.baselines()
            .map(|directory| crate::drift::load_all(&directory).0)
            .unwrap_or_default()
    }
}

/// A host answers as its machine's files and commands do, so it goes
/// wherever a `doctor::System` does.
impl doctor::System for Host {
    fn read(&self, path: &Path) -> Option<String> {
        self.system.read(path)
    }

    fn exists(&self, path: &Path) -> bool {
        self.system.exists(path)
    }

    fn list(&self, path: &Path) -> Vec<String> {
        self.system.list(path)
    }

    fn link(&self, path: &Path) -> Option<PathBuf> {
        self.system.link(path)
    }

    fn run(&self, command: &[&str]) -> Option<(bool, String)> {
        self.system.run(command)
    }

    fn resolve(&self, host: &str) -> Result<Vec<String>, String> {
        self.system.resolve(host)
    }

    fn space(&self, path: &Path) -> Option<(u64, u64)> {
        self.system.space(path)
    }

    fn owner(&self, path: &Path) -> Option<u32> {
        self.system.owner(path)
    }

    fn is_link(&self, path: &Path) -> bool {
        self.system.is_link(path)
    }

    fn canonical(&self, path: &Path) -> Option<PathBuf> {
        self.system.canonical(path)
    }

    fn user(&self) -> (u32, u32, PathBuf) {
        self.system.user()
    }

    fn on_path(&self, tool: &str) -> bool {
        self.system.on_path(tool)
    }

    fn can_look(&self, path: &Path) -> bool {
        self.system.can_look(path)
    }

    fn is_dir(&self, path: &Path) -> bool {
        self.system.is_dir(path)
    }

    fn size(&self, path: &Path) -> Option<u64> {
        self.system.size(path)
    }

    fn processes(&self) -> Vec<String> {
        self.system.processes()
    }

    fn node(&self, path: &Path) -> Option<doctor::Node> {
        self.system.node(path)
    }

    fn names(&self, path: &Path) -> Option<Vec<std::ffi::OsString>> {
        self.system.names(path)
    }

    fn open(&self, path: &Path) -> Option<doctor::Opened<'_>> {
        self.system.open(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_made_up_host_answers_only_what_it_was_told() {
        let home = tempfile::tempdir().unwrap();
        let state = tempfile::tempdir().unwrap();
        history::record_in(
            state.path(),
            &[("marker".to_string(), "Marker".to_string())],
            true,
            "exit 0",
            None,
        )
        .unwrap();
        let fake = doctor::Fake {
            home: home.path().to_path_buf(),
            tools: vec!["restic".into()],
            ..doctor::Fake::default()
        };
        let host = Host::made_up(fake)
            .with_state(state.path())
            .with_variable("SSH_CONNECTION", "192.0.2.1 50000 192.0.2.2 22")
            .at(1_000);
        assert!(!host.is_here());
        assert_eq!(host.home(), home.path());
        assert!(host.ssh());
        assert_eq!(host.desktop(), None);
        assert_eq!(host.now(), 1_000);
        assert_eq!(host.which("restic"), Some(PathBuf::from("/usr/bin/restic")));
        assert_eq!(host.which("git"), None);
        assert_eq!(host.history(10).len(), 1);
        assert!(host.render(Path::new("scripts"), &["grub".into()]).is_err());
        assert_eq!(host.restic(), None);
        assert_eq!(host.baselines(), None);
        assert_eq!(host.snapshots(), None);
        assert!(host.backups().is_empty());
        assert!(host.backup_roots().system.is_empty());
        // Without a state folder there is no history to read.
        let bare = Host::made_up(doctor::Fake::default());
        assert!(bare.history(10).is_empty());
        assert_eq!(bare.units(), None);
    }

    #[test]
    fn a_made_up_host_finds_the_snapshot_tool_its_fake_has() {
        let mut fake = doctor::Fake {
            tools: vec!["snapper".into()],
            ..doctor::Fake::default()
        };
        fake.files
            .insert("/etc/snapper/configs/root".into(), String::new());
        assert_eq!(
            Host::made_up(fake).snapshots(),
            Some(backups::Snapshots::Snapper)
        );
    }

    #[test]
    fn a_made_up_hosts_configuration_folder_is_searched_for_backups() {
        let home = tempfile::tempdir().unwrap();
        let config = tempfile::tempdir().unwrap();
        std::fs::write(
            config
                .path()
                .join("kitty.conf.helix-toolbox-backup-20261015164340"),
            "font_size 12\n",
        )
        .unwrap();
        let fake = doctor::Fake {
            home: home.path().to_path_buf(),
            ..doctor::Fake::default()
        };
        let host = Host::made_up(fake).with_config(config.path());
        assert!(
            host.backup_roots()
                .user
                .contains(&config.path().to_path_buf())
        );
        let found = host.backups();
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].original, config.path().join("kitty.conf"));
    }

    #[test]
    fn the_checks_kept_for_after_the_reboot_are_read_from_the_state_folder() {
        // Like the history: Helix Toolbox's own state folder, not the
        // machine's files, so a made-up host's fake need not hold it.
        let state = tempfile::tempdir().unwrap();
        let kept = [crate::verification::Waiting {
            entry: "marker".into(),
            name: "Marker".into(),
            ran: 1_000,
            checks: Vec::new(),
        }];
        std::fs::write(
            state.path().join(crate::verification::AFTER_REBOOT),
            serde_json::to_string(&kept).unwrap(),
        )
        .unwrap();
        let host = Host::made_up(doctor::Fake::default()).with_state(state.path());
        let found = doctor::examine_on(&host, None);
        let after = found
            .iter()
            .find(|diagnosis| diagnosis.id == "after-reboot")
            .expect("the kept checks were not read");
        assert_eq!(after.summary, "waiting for the next reboot: Marker");
    }
}
