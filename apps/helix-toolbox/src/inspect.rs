//! `helix-toolbox reliability` and `helix-toolbox trust`: how far entries have
//! been tested, and where an entry's installer and packages come from.

use anyhow::{Context, Result};
use helix_toolbox_catalog::Catalog;
use helix_toolbox_core::Entry;
use helix_toolbox_engine::{
    Probe, doctor, history,
    reliability::{Assessment, Evidence, Level},
    trust::{self, Build, Fact, Inspection},
    updates::Install,
    verification,
};
pub use helix_toolbox_engine::{reliability::rows, trust::text};
use helix_toolbox_platform::LinuxInfo;
use serde_json::{Value, json};

/// This copy of Helix Toolbox, for the provenance of what it ships.
pub fn build() -> Build {
    Build {
        version: crate::VERSION.to_string(),
        release: option_env!("HELIX_TOOLBOX_VERSION").is_some(),
        commit: option_env!("HELIX_TOOLBOX_COMMIT").map(String::from),
        install: crate::this_places()
            .this
            .map(|this| this.install)
            .unwrap_or(Install::Other),
    }
}

/// Counts of each level, best first, for a summary line.
pub fn counts(rows: &[(&Entry, Assessment)]) -> Vec<(Level, usize)> {
    [
        Level::HardwareTested,
        Level::VmTested,
        Level::ContainerTested,
        Level::StandInTested,
        Level::NotYetValidated,
    ]
    .into_iter()
    .map(|level| {
        (
            level,
            rows.iter()
                .filter(|(_, assessment)| assessment.level == level)
                .count(),
        )
    })
    .filter(|(_, count)| *count > 0)
    .collect()
}

/// `helix-toolbox verify`: each entry's checks now.
pub fn verify(
    catalog: &Catalog,
    platform: &LinuxInfo,
    ids: &[String],
    as_json: bool,
) -> Result<()> {
    let records = history::read(1000).unwrap_or_default();
    let mut found = Vec::new();
    for id in ids {
        let entry = crate::find(catalog, id)?;
        let rebooted = verification::rebooted_since(&doctor::Here, &records, &entry.id);
        let checks = verification::check_entry(
            entry,
            platform,
            rebooted,
            &Probe::holds,
            &helix_toolbox_platform::binary_exists,
            &doctor::Here,
        );
        found.push((entry, checks));
    }
    let failed = found
        .iter()
        .any(|(_, checks)| checks.iter().any(|check| !check.passed));
    if as_json {
        crate::json::print(
            "verify",
            json!({
                "passed": !failed,
                "entries": found.iter().map(|(entry, checks)| json!({
                    "id": entry.id,
                    "name": entry.name,
                    "checks": checks.iter().map(|check| json!({
                        "name": check.name,
                        "label": check.label(),
                        "passed": check.passed,
                        "unsure": check.unsure,
                        "later": check.later,
                        "detail": check.detail,
                    })).collect::<Vec<_>>(),
                })).collect::<Vec<_>>(),
            }),
        );
    } else {
        for (entry, checks) in &found {
            println!("{}", entry.name);
            if checks.is_empty() {
                println!(
                    "  Nothing to check: it declares no checks and installs nothing Helix Toolbox can find."
                );
            }
            for check in checks {
                println!("  {}: {}: {}", check.label(), check.name, check.detail);
            }
        }
    }
    if failed {
        anyhow::bail!("not everything is as it should be");
    }
    Ok(())
}

pub fn reliability(catalog: &Catalog, ids: &[String], critical: bool, as_json: bool) -> Result<()> {
    let evidence = Evidence::load(catalog.root());
    let records = history::read(1000).unwrap_or_default();
    let mut rows = rows(catalog, &evidence, &records);
    if !ids.is_empty() {
        for id in ids {
            crate::find(catalog, id)?;
        }
        rows.retain(|(entry, _)| ids.contains(&entry.id));
    }
    if critical {
        rows.retain(|(_, assessment)| assessment.critical);
    }
    if as_json {
        crate::json::print(
            "reliability",
            json!({
                "record": evidence.results.as_ref().map(|results| json!({
                    "run": results.run,
                    "url": results.url,
                    "commit": results.commit,
                    "finished": results.finished,
                    "distributions": results.images.keys().collect::<Vec<_>>(),
                })),
                "entries": rows.iter().map(|(entry, assessment)| json!({
                    "id": entry.id,
                    "name": entry.name,
                    "assessment": assessment_json(assessment),
                })).collect::<Vec<_>>(),
            }),
        );
        return Ok(());
    }
    println!("{}", evidence.summary());
    let summary: Vec<String> = counts(&rows)
        .into_iter()
        .map(|(level, count)| format!("{count} {}", level.label()))
        .collect();
    println!("{}\n", summary.join(", "));
    for (entry, assessment) in &rows {
        println!(
            "{:<18} {:<36} {:<11} {}{}",
            assessment.level.label(),
            entry.name,
            assessment.verified.as_deref().unwrap_or("-"),
            assessment.systems.join(", "),
            match (assessment.critical, assessment.here) {
                (true, Some(when)) => format!(
                    "  [boot, login or drivers; ran here {}]",
                    helix_toolbox_engine::date(when)
                ),
                (true, None) => "  [boot, login or drivers]".to_string(),
                (false, Some(when)) => format!("  [ran here {}]", helix_toolbox_engine::date(when)),
                (false, None) => String::new(),
            }
        );
    }
    Ok(())
}

pub fn assessment_json(assessment: &Assessment) -> Value {
    json!({
        "level": assessment.level,
        "label": assessment.level.label(),
        "systems": assessment.systems,
        "verified": assessment.verified,
        "evidence": assessment.evidence,
        "ran_here": assessment.here,
        "critical": assessment.critical,
    })
}

/// Inspect one entry on this machine.
pub fn inspection(catalog: &Catalog, platform: &LinuxInfo, entry: &Entry) -> Inspection {
    trust::inspect(
        entry,
        catalog.root(),
        platform,
        &build(),
        &doctor::Here,
        &Evidence::load(catalog.root()),
        &history::read(1000).unwrap_or_default(),
    )
}

pub fn trust(catalog: &Catalog, platform: &LinuxInfo, id: &str, as_json: bool) -> Result<()> {
    let entry = catalog
        .find(id)
        .with_context(|| format!("no entry named '{id}'; see helix-toolbox list"))?;
    let found = inspection(catalog, platform, entry);
    if as_json {
        let facts = |facts: &[Fact]| {
            facts
                .iter()
                .map(|fact| {
                    json!({
                        "standing": fact.standing,
                        "label": fact.standing.label(),
                        "subject": fact.subject,
                        "text": fact.text,
                    })
                })
                .collect::<Vec<_>>()
        };
        crate::json::print(
            "trust",
            json!({
                "id": found.id,
                "name": found.name,
                "kind": found.kind,
                "origin": found.origin,
                "file": found.file,
                "sha256": found.sha256,
                "installer_license": found.installer_license,
                "package_license": found.package_license,
                "provenance": facts(&found.provenance),
                "integrity": facts(&found.integrity),
                "downloads": facts(&found.downloads),
                "privileges": found.privileges,
                "validation": assessment_json(&found.validation),
            }),
        );
        return Ok(());
    }
    print!("{}", text(&found));
    Ok(())
}
