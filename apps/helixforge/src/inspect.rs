//! `helixforge reliability` and `helixforge trust`: how far entries have
//! been tested, and where an entry's installer and packages come from.

use anyhow::{Context, Result};
use helixforge_catalog::Catalog;
use helixforge_core::{Entry, printable};
use helixforge_engine::{
    doctor, history,
    reliability::{Assessment, Evidence, Level},
    trust::{self, Build, Fact, Inspection},
    updates::Install,
};
use helixforge_platform::LinuxInfo;
use serde_json::{Value, json};

/// This copy of HelixForge, for the provenance of what it ships.
pub fn build() -> Build {
    Build {
        version: crate::VERSION.to_string(),
        release: option_env!("HELIXFORGE_VERSION").is_some(),
        commit: option_env!("HELIXFORGE_COMMIT").map(String::from),
        install: crate::this_places()
            .this
            .map(|this| this.install)
            .unwrap_or(Install::Other),
    }
}

/// The entries a reliability listing covers, critical ones first, then by
/// how little they are tested, then by name. Profiles are left out: their
/// entries are listed.
pub fn rows<'a>(
    catalog: &'a Catalog,
    evidence: &Evidence,
    records: &[history::Record],
) -> Vec<(&'a Entry, Assessment)> {
    let mut rows: Vec<(&Entry, Assessment)> = Vec::new();
    for entry in catalog.entries() {
        if matches!(entry.action, helixforge_core::Action::Profile { .. })
            || rows.iter().any(|(known, _)| known.id == entry.id)
        {
            continue;
        }
        rows.push((entry, evidence.assess(entry, records)));
    }
    rows.sort_by(|(a, first), (b, second)| {
        second
            .critical
            .cmp(&first.critical)
            .then(second.level.cmp(&first.level))
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    rows
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
                    helixforge_engine::date(when)
                ),
                (true, None) => "  [boot, login or drivers]".to_string(),
                (false, Some(when)) => format!("  [ran here {}]", helixforge_engine::date(when)),
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
        .with_context(|| format!("no entry named '{id}'; see helixforge list"))?;
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
                "license": found.license,
                "software_license": found.software_license,
                // The same, under the names HelixForge 1.2 gave them.
                "licence": found.license,
                "software_licence": found.software_license,
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

/// An inspection as text, for the command line and the menu.
pub fn text(found: &Inspection) -> String {
    let mut text = format!("{} ({})\n\n", found.name, found.id);
    text.push_str(&format!("Installer   {}\n", found.origin));
    if let Some(file) = &found.file {
        text.push_str(&format!("File        scripts/{file}\n"));
    }
    if let Some(sum) = &found.sha256 {
        text.push_str(&format!("SHA-256     {sum}\n"));
    }
    if let Some(license) = &found.license {
        text.push_str(&format!("License     {license} for the installer"));
        match &found.software_license {
            Some(software) => text.push_str(&format!("; {software} for the software\n")),
            None => text.push('\n'),
        }
    }
    let section = |text: &mut String, title: &str, facts: &[Fact], empty: &str| {
        text.push_str(&format!("\n{title}\n"));
        if facts.is_empty() {
            text.push_str(&format!("  {empty}\n"));
        }
        for fact in facts {
            let subject = if fact.subject == "Download" {
                String::new()
            } else {
                format!("{}: ", fact.subject)
            };
            text.push_str(&format!(
                "  {:<19} {subject}{}\n",
                fact.standing.label(),
                fact.text
            ));
        }
    };
    section(
        &mut text,
        "Release provenance (the HelixForge build it ships in)",
        &found.provenance,
        "none",
    );
    section(
        &mut text,
        "Package integrity (checked by the package tools)",
        &found.integrity,
        "it installs no packages HelixForge knows of",
    );
    section(
        &mut text,
        "Downloads the installer makes itself",
        &found.downloads,
        "none found in its script",
    );
    text.push_str("\nPrivileges\n");
    for line in &found.privileges {
        text.push_str(&format!("  {line}\n"));
    }
    text.push_str(&format!(
        "\nValidation  {}{}\n",
        found.validation.level.label(),
        found
            .validation
            .verified
            .as_deref()
            .map(|date| format!(", last verified {date}"))
            .unwrap_or_default()
    ));
    text.push_str(&format!("  {}\n", found.validation.level.explain()));
    for line in &found.validation.evidence {
        text.push_str(&format!("  {}\n", printable(line)));
    }
    text
}
