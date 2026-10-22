//! What docs/STABILITY.md promises for 1.x, checked against the program:
//! the `--json` shapes only gain fields (each command's saved in
//! tests/contracts/json), and the files an earlier 1.x wrote (in
//! tests/contracts/files, as 1.0, 1.1, 1.4 and 1.19 wrote them) are read by this
//! one. HELIX_TOOLBOX_BLESS_CONTRACTS=1 saves the shape of a command that
//! has none yet; a saved shape is never rewritten by the test.

use serde_json::{Map, Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Run the program over the fixture catalog, in a home of its own.
fn run(home: &Path, args: &[&str]) -> (bool, String) {
    let output = Command::new(env!("CARGO_BIN_EXE_helix-toolbox"))
        .arg("--catalog-dir")
        .arg(root().join("tests/fixtures"))
        .args(args)
        .env("HOME", home)
        .env("XDG_CONFIG_HOME", home.join("config"))
        .env("XDG_STATE_HOME", home.join("state"))
        .env("XDG_DATA_HOME", home.join("data"))
        .env("XDG_CACHE_HOME", home.join("cache"))
        .env_remove("HELIX_TOOLBOX_LOG")
        .env_remove("RESTIC_REPOSITORY")
        .env_remove("RESTIC_PASSWORD_FILE")
        .env_remove("RESTIC_PASSWORD_COMMAND")
        .env_remove("RESTIC_PASSWORD")
        .env_remove("RESTIC_WALLET")
        .env_remove("RESTIC_WALLET_ENTRY")
        .output()
        .expect("the program runs");
    (
        output.status.success(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
    )
}

fn json(home: &Path, args: &[&str]) -> Value {
    let mut all = vec!["--json"];
    all.extend(args);
    let (_, text) = run(home, &all);
    serde_json::from_str(&text).unwrap_or_else(|error| panic!("{args:?}: {error}\n{text}"))
}

/// A value's shape: each object's keys with their shapes, an array's first
/// item, and the kind of anything else.
fn shape(value: &Value) -> Value {
    match value {
        Value::Object(fields) => Value::Object(
            fields
                .iter()
                .map(|(key, value)| (key.clone(), shape(value)))
                .collect::<Map<_, _>>(),
        ),
        Value::Array(items) => Value::Array(items.first().map(shape).into_iter().collect()),
        Value::String(_) => json!("string"),
        Value::Number(_) => json!("number"),
        Value::Bool(_) => json!("boolean"),
        Value::Null => json!("null"),
    }
}

/// What a saved shape has that this one lacks or gives another kind. A
/// null, or an empty list, on either side matches anything: those vary
/// with the machine.
fn lost(saved: &Value, now: &Value, at: &str, into: &mut Vec<String>) {
    match (saved, now) {
        (Value::String(kind), _) if kind == "null" => {}
        (_, Value::String(kind)) if kind == "null" => {}
        (Value::Object(saved), Value::Object(now)) => {
            for (key, value) in saved {
                match now.get(key) {
                    Some(found) => lost(value, found, &format!("{at}.{key}"), into),
                    None => into.push(format!("{at}.{key} is gone")),
                }
            }
        }
        (Value::Array(saved), Value::Array(now)) => {
            if let (Some(saved), Some(now)) = (saved.first(), now.first()) {
                lost(saved, now, &format!("{at}[]"), into);
            }
        }
        (saved, now) if saved != now => {
            into.push(format!("{at} was {saved}, is {now}"));
        }
        _ => {}
    }
}

#[test]
fn json_shapes_only_gain_fields() {
    let home = tempfile::tempdir().unwrap();
    let directory = root().join("tests/contracts/json");
    let migration = root().join("tests/contracts/files/migration-1.19.toml");
    let migration = migration.to_str().unwrap();
    // A Ghostty configuration with a backup kept beside it.
    let ghostty = home.path().join("config/ghostty");
    fs::create_dir_all(&ghostty).unwrap();
    fs::write(ghostty.join("config"), "font-size = 12\n").unwrap();
    fs::write(
        ghostty.join("config.helix-toolbox-backup-20261001000000-1"),
        "font-size = 11\n",
    )
    .unwrap();
    let commands: [(&str, &[&str]); 24] = [
        ("system", &["system"]),
        ("list", &["list"]),
        ("search", &["search", "marker"]),
        ("info", &["info", "marker"]),
        ("plan", &["plan", "marker"]),
        ("history", &["history"]),
        ("profile-list", &["profile", "list"]),
        ("drift-list", &["drift", "list"]),
        ("reliability", &["reliability"]),
        ("verify", &["verify", "marker"]),
        ("report", &["report", "--no-checks"]),
        ("storage", &["storage"]),
        ("startup", &["startup"]),
        ("performance", &["performance"]),
        ("performance-boots", &["performance", "boots"]),
        ("recovery", &["recovery"]),
        ("migrate-export", &["migrate", "export"]),
        ("migration", &["migrate", "show", migration]),
        ("restic", &["restic"]),
        ("restic-baselines", &["restic", "baseline"]),
        ("config", &["config", "ghostty"]),
        ("config-history", &["config", "ghostty", "history"]),
        ("home-manager", &["home-manager"]),
        ("home-manager-generations", &["home-manager", "generations"]),
    ];
    // A Home Manager configuration where Home Manager looks by itself, so
    // its shape is in the saved one.
    fs::create_dir_all(home.path().join(".config/home-manager")).unwrap();
    fs::write(home.path().join(".config/home-manager/flake.nix"), "{ }\n").unwrap();
    let mut problems = Vec::new();
    for (name, args) in commands {
        let now = shape(&json(home.path(), args));
        assert_eq!(now["schema"], json!("number"), "{name} has no schema");
        let path = directory.join(format!("{name}.json"));
        match fs::read_to_string(&path) {
            Ok(text) => {
                let saved: Value = serde_json::from_str(&text).unwrap();
                lost(&saved, &now, name, &mut problems);
            }
            Err(_) if std::env::var_os("HELIX_TOOLBOX_BLESS_CONTRACTS").is_some() => {
                fs::create_dir_all(&directory).unwrap();
                fs::write(&path, serde_json::to_string_pretty(&now).unwrap() + "\n").unwrap();
            }
            Err(_) => problems.push(format!(
                "{name} has no saved shape: HELIX_TOOLBOX_BLESS_CONTRACTS=1 cargo test --test contracts saves it"
            )),
        }
    }
    assert!(
        problems.is_empty(),
        "JSON that 1.x promised changed:\n{}",
        problems.join("\n")
    );
}

/// The repositories commands, over one small repository of the test's own
/// under the folder its settings name.
#[test]
fn repository_json_shapes_only_gain_fields() {
    let home = tempfile::tempdir().unwrap();
    let repo = home.path().join("projects/sample");
    fs::create_dir_all(&repo).unwrap();
    let git = |args: &[&str]| {
        let status = Command::new("git")
            .arg("-C")
            .arg(&repo)
            .args(args)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_AUTHOR_NAME", "Test")
            .env("GIT_AUTHOR_EMAIL", "test@example.com")
            .env("GIT_COMMITTER_NAME", "Test")
            .env("GIT_COMMITTER_EMAIL", "test@example.com")
            .output()
            .expect("git runs");
        assert!(status.status.success(), "git {args:?}");
    };
    git(&["init", "-q", "-b", "main"]);
    fs::write(repo.join("Cargo.toml"), "[package]\nname = \"sample\"\n").unwrap();
    git(&["add", "Cargo.toml"]);
    git(&["commit", "-q", "-m", "First"]);
    fs::write(repo.join("Cargo.toml"), "[package]\nname = \"changed\"\n").unwrap();
    git(&["stash", "push", "-q", "-m", "Kept"]);
    fs::write(repo.join("new.txt"), "new\n").unwrap();
    let config = home.path().join("config/helix-toolbox");
    fs::create_dir_all(&config).unwrap();
    fs::write(
        config.join("settings.toml"),
        "[repositories]\nroots = [\"~/projects\"]\n",
    )
    .unwrap();
    let directory = root().join("tests/contracts/json");
    let mut problems = Vec::new();
    for (name, args) in [
        ("repositories", &["repos"][..]),
        ("repository", &["repos", "show", "sample"]),
        ("repos-diff", &["repos", "diff", "sample", "new.txt"]),
        ("repos-tasks", &["repos", "tasks", "sample"]),
    ] {
        let now = shape(&json(home.path(), args));
        let path = directory.join(format!("{name}.json"));
        match fs::read_to_string(&path) {
            Ok(text) => lost(
                &serde_json::from_str(&text).unwrap(),
                &now,
                name,
                &mut problems,
            ),
            Err(_) if std::env::var_os("HELIX_TOOLBOX_BLESS_CONTRACTS").is_some() => {
                fs::write(&path, serde_json::to_string_pretty(&now).unwrap() + "\n").unwrap();
            }
            Err(_) => problems.push(format!("{name} has no saved shape")),
        }
    }
    assert!(
        problems.is_empty(),
        "JSON that 1.x promised changed:\n{}",
        problems.join("\n")
    );
}

/// A home holding the files earlier releases wrote.
fn old_home() -> tempfile::TempDir {
    let home = tempfile::tempdir().unwrap();
    let files = root().join("tests/contracts/files");
    let config = home.path().join("config/helix-toolbox");
    let state = home.path().join("state/helix-toolbox");
    for (from, to) in [
        ("settings-1.0.toml", config.join("settings.toml")),
        ("favourites-1.4.toml", config.join("favourites.toml")),
        ("profile-1.0.toml", config.join("profiles/dev-box.toml")),
        (
            "baseline-1.1.toml",
            config.join("baselines/fresh-install.toml"),
        ),
        ("look-1.4.toml", config.join("looks/evening.toml")),
        ("history-1.0.jsonl", state.join("history.jsonl")),
    ] {
        fs::create_dir_all(to.parent().unwrap()).unwrap();
        fs::copy(files.join(from), &to).unwrap();
    }
    home
}

#[test]
fn files_an_earlier_release_wrote_are_read() {
    let home = old_home();
    // settings.toml from 1.0: the old name of the transparent theme.
    let report = json(home.path(), &["report", "--no-checks"]);
    let settings = &report["data"]["settings"];
    assert_eq!(settings["theme"], json!("transparent"), "{report}");
    assert_eq!(settings["icons"], json!(false));
    assert_eq!(settings["keep_going"], json!(true));
    assert_eq!(settings["snapshots"], json!(false));
    // history.jsonl from 1.0, without the log field.
    let history = json(home.path(), &["history"]);
    let runs = history["data"]["runs"].as_array().unwrap();
    assert_eq!(runs.len(), 2, "{history}");
    assert!(runs.iter().any(|run| run["entries"] == json!(["marker"])));
    // A profile from 1.0.
    let profiles = json(home.path(), &["profile", "list"]);
    let found = &profiles["data"]["profiles"];
    assert_eq!(found[0]["name"], json!("Dev Box"), "{profiles}");
    assert_eq!(found[0]["entries"], json!(["marker", "report"]));
    // A baseline from 1.1.
    let baselines = json(home.path(), &["drift", "list"]);
    assert_eq!(
        baselines["data"]["baselines"][0]["name"],
        json!("Fresh install"),
        "{baselines}"
    );
    // A look from 1.4, and favorites under their name before 1.5.9.
    let look = helix_toolbox_engine::desktop::read(
        &home.path().join("config/helix-toolbox/looks/evening.toml"),
    )
    .unwrap();
    assert_eq!(look.desktop, "kde");
    let favorites = helix_toolbox_engine::settings::load_favorites(
        &home.path().join("config/helix-toolbox/favorites.toml"),
    );
    assert_eq!(favorites, ["marker"]);
}

#[test]
fn settings_this_release_does_not_know_are_kept() {
    let home = old_home();
    let path = home.path().join("config/helix-toolbox/settings.toml");
    let mut text = fs::read_to_string(&path).unwrap();
    text.push_str("from_a_later_release = \"kept\"\n");
    fs::write(&path, &text).unwrap();
    // Saving the run choices rewrites the file; what it does not know stays.
    helix_toolbox_engine::settings::save_run_choices(
        &path,
        helix_toolbox_engine::settings::RunChoices {
            keep_going: false,
            snapshots: true,
        },
    )
    .unwrap();
    let after = fs::read_to_string(&path).unwrap();
    assert!(after.contains("from_a_later_release = \"kept\""), "{after}");
    assert!(after.contains("theme = \"default\""), "{after}");
}
