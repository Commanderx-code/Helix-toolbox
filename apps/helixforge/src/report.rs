//! `helixforge report`: what a bug report needs to know about this copy of
//! HelixForge and this machine, to paste into an issue. It reads only, and
//! leaves out what is personal: the home directory is shown as `~`, there is
//! no host name, and the history gives entry names and results, not logs.

use std::path::Path;

use helixforge_engine::{doctor, history, inventory::which, updates};
use helixforge_platform::LinuxInfo;
use serde_json::{Value, json};

use crate::tui::settings;

/// How this copy was installed, from where it runs.
fn installed_from(exe: &Path, temp: &Path) -> &'static str {
    if exe.starts_with("/usr/bin") || exe.starts_with("/usr/sbin") {
        "package"
    } else if exe.components().any(|part| part.as_os_str() == "target") {
        "source build"
    } else if exe.starts_with(temp) {
        "start.sh download"
    } else {
        "other"
    }
}

/// Everything the report says, as one value; the text is made from it.
pub fn gather(platform: &LinuxInfo, checks: bool) -> Value {
    let exe = std::env::current_exe().unwrap_or_default();
    let look = settings::path().map(|path| settings::load(&path));
    let tools: serde_json::Map<String, Value> = [
        "sudo",
        "flatpak",
        "nix",
        "home-manager",
        "paru",
        "yay",
        "snapper",
        "timeshift",
    ]
    .into_iter()
    .map(|tool| (tool.to_string(), Value::Bool(which(tool).is_some())))
    .collect();
    let size = crossterm::terminal::size()
        .ok()
        .map(|(columns, rows)| format!("{columns}x{rows}"));
    let variable = |name: &str| std::env::var(name).ok();
    let runs: Vec<Value> = history::read(5)
        .unwrap_or_default()
        .iter()
        .map(|record| {
            json!({
                "date": updates::date(record.unix_timestamp),
                "success": record.success,
                "names": record.names,
                "result": record.result,
            })
        })
        .collect();
    let health: Option<Vec<Value>> = checks.then(|| {
        doctor::examine(&doctor::Here, platform.manager)
            .iter()
            .map(|diagnosis| {
                json!({
                    "id": diagnosis.id,
                    "health": diagnosis.health.label(),
                    "title": diagnosis.title,
                    "summary": diagnosis.summary,
                })
            })
            .collect()
    });
    let report = json!({
        "helixforge": crate::VERSION,
        "installed_from": installed_from(&exe, &std::env::temp_dir()),
        "system": crate::json::system(platform),
        "tools": tools,
        "terminal": {
            "term": variable("TERM"),
            "term_program": variable("TERM_PROGRAM"),
            "colorterm": variable("COLORTERM"),
            "size": size,
        },
        "settings": look.map(|look| json!({
            "theme": look.theme.label(),
            "icons": look.icons,
            "keep_going": look.keep_going,
            "snapshots": look.snapshots,
        })),
        "recent_runs": runs,
        "health_checks": health,
    });
    without_home(report)
}

/// Show the home directory as `~` everywhere in the report.
fn without_home(report: Value) -> Value {
    let Some(home) = std::env::var_os("HOME")
        .map(|home| home.to_string_lossy().trim_end_matches('/').to_string())
        .filter(|home| home.len() > 1)
    else {
        return report;
    };
    let text = serde_json::to_string(&report).unwrap_or_default();
    // JSON escapes nothing in an ordinary path, so the plain text matches.
    serde_json::from_str(&text.replace(&home, "~")).unwrap_or(report)
}

fn text(value: &Value) -> String {
    match value {
        Value::Null => "unknown".to_string(),
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

/// The report as Markdown, ready to paste into an issue.
pub fn markdown(report: &Value) -> String {
    let system = &report["system"];
    let mut out = String::from("### HelixForge report\n\n");
    let mut line = |label: &str, value: String| out.push_str(&format!("- {label}: {value}\n"));
    line("HelixForge", text(&report["helixforge"]));
    line("Installed from", text(&report["installed_from"]));
    line(
        "Distribution",
        format!(
            "{} ({}{})",
            text(&system["distribution"]["name"]),
            text(&system["distribution"]["id"]),
            system["distribution"]["id_like"]
                .as_array()
                .filter(|like| !like.is_empty())
                .map(|like| format!(
                    "; like {}",
                    like.iter().map(text).collect::<Vec<_>>().join(", ")
                ))
                .unwrap_or_default()
        ),
    );
    line("Package manager", text(&system["package_manager"]));
    line("Kernel", text(&system["kernel"]));
    line("Architecture", text(&system["architecture"]));
    let tools = report["tools"]
        .as_object()
        .map(|tools| {
            tools
                .iter()
                .map(|(tool, present)| {
                    format!("{tool} {}", if present == true { "yes" } else { "no" })
                })
                .collect::<Vec<_>>()
                .join(", ")
        })
        .unwrap_or_default();
    line("Tools", tools);
    let terminal = &report["terminal"];
    line(
        "Terminal",
        format!(
            "TERM={}, TERM_PROGRAM={}, COLORTERM={}, size {}",
            text(&terminal["term"]),
            text(&terminal["term_program"]),
            text(&terminal["colorterm"]),
            text(&terminal["size"])
        ),
    );
    let settings = &report["settings"];
    line(
        "Settings",
        if settings.is_null() {
            "unknown".to_string()
        } else {
            let on = |value: &Value| if value == true { "on" } else { "off" };
            format!(
                "theme {}, icons {}, keep going {}, snapshots {}",
                text(&settings["theme"]),
                on(&settings["icons"]),
                on(&settings["keep_going"]),
                on(&settings["snapshots"])
            )
        },
    );
    out.push_str("\n#### Recent runs\n\n");
    match report["recent_runs"].as_array() {
        Some(runs) if !runs.is_empty() => {
            for run in runs {
                out.push_str(&format!(
                    "- {} {}: {} ({})\n",
                    text(&run["date"]),
                    if run["success"] == true {
                        "ok"
                    } else {
                        "failed"
                    },
                    run["names"]
                        .as_array()
                        .map(|names| names.iter().map(text).collect::<Vec<_>>().join(", "))
                        .unwrap_or_default(),
                    text(&run["result"])
                ));
            }
        }
        _ => out.push_str("- none recorded\n"),
    }
    if let Some(checks) = report["health_checks"].as_array() {
        out.push_str("\n#### Health checks\n\n");
        for check in checks {
            out.push_str(&format!(
                "- {} {}: {}\n",
                text(&check["health"]),
                text(&check["title"]),
                text(&check["summary"])
            ));
        }
    }
    out.push_str(
        "\nRead it before you post it: it names your distribution, tools and recent runs.\n",
    );
    // What other programs reported never reaches the terminal as control codes.
    out.lines()
        .map(|line| helixforge_core::printable(line) + "\n")
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn where_it_runs_from_says_how_it_was_installed() {
        let temp = Path::new("/tmp");
        assert_eq!(
            installed_from(Path::new("/usr/bin/helixforge"), temp),
            "package"
        );
        assert_eq!(
            installed_from(
                Path::new("/home/me/Helix-Forge/target/release/helixforge"),
                temp
            ),
            "source build"
        );
        assert_eq!(
            installed_from(Path::new("/tmp/tmp.x1/helixforge-linux-x86_64"), temp),
            "start.sh download"
        );
        assert_eq!(
            installed_from(Path::new("/opt/hf/helixforge"), temp),
            "other"
        );
    }

    #[test]
    fn the_markdown_lists_runs_and_checks() {
        let report = json!({
            "helixforge": "0.7.0",
            "installed_from": "package",
            "system": {
                "distribution": { "id": "garuda", "name": "Garuda Linux", "id_like": ["arch"] },
                "package_manager": "pacman",
                "kernel": "7.2.10",
                "architecture": "x86_64",
            },
            "tools": { "sudo": true, "nix": false },
            "terminal": { "term": "xterm-ghostty", "term_program": null, "colorterm": "truecolor", "size": "120x40" },
            "settings": { "theme": "solid", "icons": true, "keep_going": false, "snapshots": true },
            "recent_runs": [{ "date": "2026-10-15", "success": false, "names": ["Git"], "result": "exit 1" }],
            "health_checks": [{ "health": "Problem", "title": "Permissions", "summary": "1 path\u{1b}[2J" }],
        });
        let text = markdown(&report);
        assert!(
            text.contains("- Distribution: Garuda Linux (garuda; like arch)"),
            "{text}"
        );
        assert!(text.contains("- Tools: nix no, sudo yes"), "{text}");
        assert!(text.contains("TERM_PROGRAM=unknown"));
        assert!(text.contains("- 2026-10-15 failed: Git (exit 1)"));
        assert!(text.contains("- Problem Permissions: 1 path?[2J"));
        assert!(!text.contains('\u{1b}'));
    }
}
