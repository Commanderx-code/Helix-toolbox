//! What the scheduled check watches: updates waiting, as the notifications
//! timer always has, and, when chosen, Helix Doctor's health checks and how
//! the machine differs from a saved Drift baseline. Each is told only when
//! something is new since it was last told, and nothing is ever repaired or
//! put back by itself. The choices live in `settings.toml` under `[watch]`;
//! the timer is on while any of them is.

use crate::{
    doctor::{Diagnosis, Health},
    drift::{Difference, Report},
    history,
    notifications::{self, Places, Started},
    settings,
};
use serde::{Serialize, de::DeserializeOwned};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

/// Health checks that come and go with the network, left out: a laptop
/// off its Wi-Fi is not news.
pub const UNWATCHED: [&str; 2] = ["network", "dns"];

/// What the scheduled check looks at.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
pub struct Watch {
    pub updates: bool,
    pub health: bool,
    /// The baseline compared with, by its file name; none when Drift is not
    /// watched.
    pub drift: Option<String>,
}

impl Watch {
    pub fn any(&self) -> bool {
        self.updates || self.health || self.drift.is_some()
    }
}

/// What is watched: nothing while the timer is off; otherwise the choices
/// in `settings.toml`, updates on unless turned off (as before there were
/// other choices).
pub fn read(places: &Places, settings_path: Option<&Path>) -> Watch {
    if !notifications::is_on(places) {
        return Watch::default();
    }
    let table = settings_path
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|text| text.parse::<toml::Table>().ok())
        .and_then(|table| table.get("watch")?.as_table().cloned())
        .unwrap_or_default();
    Watch {
        updates: table
            .get("updates")
            .and_then(toml::Value::as_bool)
            .unwrap_or(true),
        health: table
            .get("health")
            .and_then(toml::Value::as_bool)
            .unwrap_or(false),
        drift: table
            .get("drift")
            .and_then(toml::Value::as_str)
            .filter(|slug| crate::profiles::slug(slug).as_deref() == Some(*slug))
            .map(str::to_string),
    }
}

/// Save the choices and set the timer to match: on while anything is
/// watched, removed when nothing is. Returns how it started, when it was
/// turned on now.
pub fn apply(
    places: &Places,
    settings_path: &Path,
    watch: &Watch,
    program: &Path,
    window: Option<&Path>,
    systemctl: &dyn Fn(&[&str]) -> Option<bool>,
) -> Result<Option<Started>, String> {
    let was_on = notifications::is_on(places);
    let started = match (watch.any(), was_on) {
        (true, false) => Some(notifications::turn_on(places, program, window, systemctl)?),
        (false, true) => {
            notifications::turn_off(places, systemctl)?;
            None
        }
        _ => None,
    };
    settings::update(settings_path, |table| {
        let mut ours = toml::Table::new();
        ours.insert("updates".into(), toml::Value::Boolean(watch.updates));
        ours.insert("health".into(), toml::Value::Boolean(watch.health));
        if let Some(drift) = &watch.drift {
            ours.insert("drift".into(), toml::Value::String(drift.clone()));
        }
        table.insert("watch".into(), toml::Value::Table(ours));
    })
    .map_err(|error| format!("the choices could not be saved: {error}"))?;
    if !watch.any() {
        for told in [health_told(places), drift_told(places)] {
            let _ = std::fs::remove_file(told);
        }
    }
    Ok(started)
}

/// What the health check last told of, beside the updates' told file.
pub fn health_told(places: &Places) -> PathBuf {
    places.told.with_file_name("health-told.json")
}

/// What Drift last told of.
pub fn drift_told(places: &Places) -> PathBuf {
    places.told.with_file_name("drift-told.json")
}

/// A told file's contents, or nothing yet.
pub fn load<T: DeserializeOwned + Default>(path: &Path) -> T {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

pub fn store<T: Serialize>(path: &Path, value: &T) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let text = serde_json::to_string_pretty(value).map_err(std::io::Error::other)?;
    let staging = path.with_extension("json.new");
    std::fs::write(&staging, text)?;
    std::fs::rename(staging, path)
}

/// Each watched check's health now, by its id.
pub fn health_state(found: &[Diagnosis]) -> BTreeMap<String, String> {
    found
        .iter()
        .filter(|diagnosis| !UNWATCHED.contains(&diagnosis.id.as_str()))
        .map(|diagnosis| (diagnosis.id.clone(), diagnosis.health.label().to_string()))
        .collect()
}

/// The problems now that were not problems when last told.
pub fn health_news<'a>(
    before: &BTreeMap<String, String>,
    found: &'a [Diagnosis],
) -> Vec<&'a Diagnosis> {
    let problem = Health::Problem.label();
    found
        .iter()
        .filter(|diagnosis| {
            diagnosis.health == Health::Problem
                && !UNWATCHED.contains(&diagnosis.id.as_str())
                && before.get(&diagnosis.id).map(String::as_str) != Some(problem)
        })
        .collect()
}

/// The notification's title and text for new problems.
pub fn health_message(news: &[&Diagnosis], window: bool) -> (String, String) {
    let title = match news.len() {
        1 => format!("Helix Doctor: {}", news[0].title),
        count => format!("Helix Doctor found {count} new problems"),
    };
    let mut body: Vec<String> = news
        .iter()
        .take(4)
        .map(|diagnosis| format!("{}: {}", diagnosis.title, diagnosis.summary))
        .collect();
    if news.len() > 4 {
        body.push(format!("and {} more.", news.len() - 4));
    }
    body.push(if window {
        "Show them in Helix Toolbox's Helix Doctor; nothing is repaired by itself.".to_string()
    } else {
        "Run helix-toolbox doctor to see them; nothing is repaired by itself.".to_string()
    });
    (title, body.join("\n"))
}

/// A difference as it is remembered: what it is about and what it is now.
pub fn drift_key(difference: &Difference) -> String {
    format!(
        "{:?}\t{}\t{}",
        difference.kind,
        difference.subject,
        difference.now.as_deref().unwrap_or_default()
    )
}

/// The differences not told of before, with what to remember.
pub fn drift_news<'a>(
    before: &BTreeSet<String>,
    report: &'a Report,
) -> (Vec<&'a Difference>, BTreeSet<String>) {
    let now: BTreeSet<String> = report.differences.iter().map(drift_key).collect();
    let news = report
        .differences
        .iter()
        .filter(|difference| !before.contains(&drift_key(difference)))
        .collect();
    (news, now)
}

/// The notification's title and text for new differences.
pub fn drift_message(against: &str, news: &[&Difference], window: bool) -> (String, String) {
    let title = match news.len() {
        1 => format!("This machine changed since {against}"),
        count => format!("{count} changes since {against}"),
    };
    let mut body: Vec<String> = news
        .iter()
        .take(4)
        .map(|difference| format!("{}: {}", difference.subject, difference.note))
        .collect();
    if news.len() > 4 {
        body.push(format!("and {} more.", news.len() - 4));
    }
    body.push(if window {
        "Show them in Helix Toolbox's Drift; nothing is put back by itself.".to_string()
    } else {
        "Run helix-toolbox drift check to see them; nothing is put back by itself.".to_string()
    });
    (title, body.join("\n"))
}

/// Where the told files go when there is no timer yet, for tests.
pub fn places_in(directory: &Path) -> Places {
    Places {
        units: directory.join("systemd/user"),
        told: directory.join("notified.txt"),
    }
}

/// The history's folder, beside which the told files are.
pub fn state() -> Option<PathBuf> {
    history::state_dir()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drift::Kind;

    fn diagnosis(id: &str, health: Health) -> Diagnosis {
        Diagnosis {
            id: id.into(),
            title: format!("Check {id}"),
            health,
            summary: format!("{id} is {}", health.label()),
            evidence: Vec::new(),
            advice: None,
            repair: None,
            entry: None,
        }
    }

    #[test]
    fn only_new_problems_are_news_and_the_network_is_left_out() {
        let found = vec![
            diagnosis("services", Health::Problem),
            diagnosis("disk-space", Health::Problem),
            diagnosis("dns", Health::Problem),
            diagnosis("boot", Health::Good),
        ];
        let before: BTreeMap<String, String> = [
            ("services".to_string(), "Problem".to_string()),
            ("disk-space".to_string(), "OK".to_string()),
        ]
        .into();
        let news = health_news(&before, &found);
        assert_eq!(news.len(), 1);
        assert_eq!(news[0].id, "disk-space");
        let state = health_state(&found);
        assert!(!state.contains_key("dns"));
        assert!(health_news(&state, &found).is_empty(), "told once");
        let (title, body) = health_message(&news, true);
        assert_eq!(title, "Helix Doctor: Check disk-space");
        assert!(body.contains("nothing is repaired by itself"));
    }

    #[test]
    fn drift_tells_of_each_difference_once() {
        let difference = |subject: &str, now: &str| Difference {
            kind: Kind::FileChanged,
            subject: subject.into(),
            was: Some("a".into()),
            now: Some(now.into()),
            entry: None,
            note: format!("{subject} changed"),
        };
        let report = Report {
            against: "Fresh install".into(),
            differences: vec![
                difference("~/.bashrc", "b"),
                difference("/etc/pacman.conf", "c"),
            ],
            matched: 3,
            unknown: Vec::new(),
        };
        let (news, seen) = drift_news(&BTreeSet::new(), &report);
        assert_eq!(news.len(), 2);
        let (none, _) = drift_news(&seen, &report);
        assert!(none.is_empty());
        // The same file changed again is news again.
        let again = Report {
            differences: vec![difference("~/.bashrc", "d")],
            ..report.clone()
        };
        assert_eq!(drift_news(&seen, &again).0.len(), 1);
        let (title, body) = drift_message("Fresh install", &news, false);
        assert_eq!(title, "2 changes since Fresh install");
        assert!(body.contains("helix-toolbox drift check"));
    }

    #[test]
    fn the_choices_and_the_timer_go_together() {
        let directory = tempfile::tempdir().unwrap();
        let places = places_in(directory.path());
        let settings = directory.path().join("settings.toml");
        let program = Path::new("/usr/bin/helix-toolbox");
        assert_eq!(
            read(&places, Some(&settings)),
            Watch::default(),
            "off: nothing watched"
        );
        let health = Watch {
            health: true,
            ..Watch::default()
        };
        let started = apply(&places, &settings, &health, program, None, &|_| Some(true)).unwrap();
        assert_eq!(started, Some(Started::Now));
        assert!(notifications::is_on(&places));
        assert_eq!(read(&places, Some(&settings)), health);
        let both = Watch {
            drift: Some("fresh-install".into()),
            ..health.clone()
        };
        assert_eq!(
            apply(&places, &settings, &both, program, None, &|_| Some(true)).unwrap(),
            None
        );
        assert_eq!(
            read(&places, Some(&settings)).drift.as_deref(),
            Some("fresh-install")
        );
        apply(
            &places,
            &settings,
            &Watch::default(),
            program,
            None,
            &|_| Some(true),
        )
        .unwrap();
        assert!(!notifications::is_on(&places));
        // A timer from before these choices tells of updates.
        notifications::turn_on(&places, program, None, &|_| Some(true)).unwrap();
        std::fs::remove_file(&settings).unwrap();
        assert_eq!(
            read(&places, Some(&settings)),
            Watch {
                updates: true,
                ..Watch::default()
            }
        );
    }
}
