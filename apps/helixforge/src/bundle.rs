//! `helixforge bundle`: make an offline installation bundle, check one, and
//! install from one.

use anyhow::{Context, Result, bail};
use clap::Subcommand;
use helixforge_catalog::Catalog;
use helixforge_core::printable;
use helixforge_engine::{bundle, doctor, history};
use helixforge_platform::LinuxInfo;
use std::{path::Path, path::PathBuf, process::Command};

#[derive(Subcommand)]
pub enum BundleCommand {
    /// Download the packages entries need, with HelixForge itself, into a new
    /// folder, for a machine of this distribution without reliable internet;
    /// Flatpak applications installed here are copied with their runtimes
    /// (flatpak create-usb). Nothing is installed here.
    Create {
        /// The folder to make; it must be new or empty.
        directory: PathBuf,
        /// The entries (or profiles) to bundle.
        #[arg(required = true)]
        ids: Vec<String>,
        /// Every dependency, not only those this machine lacks: larger, for a
        /// machine set up less than this one.
        #[arg(long)]
        complete: bool,
        /// Show what would be downloaded and stop.
        #[arg(long)]
        dry_run: bool,
    },
    /// Check a bundle: made for this distribution and architecture, and every
    /// file matching its checksum.
    Verify { directory: PathBuf },
    /// Check a bundle, then install its packages from its files, after
    /// confirmation. A bundle installs as a whole: its
    /// entries share their dependencies.
    Install {
        directory: PathBuf,
        /// Install Debian packages this machine's repository index does not
        /// know, and add a Flatpak remote this machine lacks with the
        /// bundle's copy of its key, on the bundle's word: only for a bundle
        /// you made yourself.
        #[arg(long)]
        trust_bundle: bool,
        /// Show the plan and stop.
        #[arg(long)]
        dry_run: bool,
        /// Skip the confirmation phrase, for deliberate automation.
        #[arg(long)]
        yes: bool,
    },
}

fn curl(url: &str, to: &Path) -> bool {
    // Package files are checked by their signatures or the repository's
    // signed index, so plain HTTP mirrors are allowed for them.
    Command::new("curl")
        .args([
            "--fail",
            "--location",
            "--silent",
            "--show-error",
            "--proto",
            "=https,http",
            "--output",
        ])
        .arg(to)
        .arg(url)
        .status()
        .is_ok_and(|status| status.success())
}

/// `flatpak create-usb`: the refs, with the runtimes they need, into the
/// bundle's Flatpak folder. From the system installation it first updates
/// that installation's summary, which needs root: through sudo, and the
/// copy is then given back to you.
fn create_usb(refs: &[String], user: bool, bundle: &Path) -> bool {
    let elevate = !user && !nix::unistd::geteuid().is_root();
    let mut command = if elevate {
        println!(
            "flatpak create-usb updates the system installation's summary first, which needs root; sudo may ask for your password."
        );
        let mut command = Command::new("sudo");
        command.arg("flatpak");
        command
    } else {
        Command::new("flatpak")
    };
    command
        .arg("create-usb")
        .arg(if user { "--user" } else { "--system" })
        // Locale extensions are installed in part, for your languages.
        .arg("--allow-partial")
        .arg(format!("--destination-repo={}", bundle::FLATPAKS))
        .arg(bundle)
        .args(refs);
    if !command.status().is_ok_and(|status| status.success()) {
        return false;
    }
    !elevate
        || Command::new("sudo")
            .arg("chown")
            .arg("-R")
            .arg(format!(
                "{}:{}",
                nix::unistd::getuid(),
                nix::unistd::getgid()
            ))
            .arg("--")
            .arg(bundle.join(bundle::FLATPAKS))
            .status()
            .is_ok_and(|status| status.success())
}

/// Where flatpak keeps a remote's signing key.
fn flatpak_key(remote: &str, user: bool) -> PathBuf {
    let variable = |name: &str| std::env::var_os(name).filter(|value| !value.is_empty());
    let installation = if user {
        variable("FLATPAK_USER_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                variable("XDG_DATA_HOME")
                    .map(PathBuf::from)
                    .or_else(|| {
                        variable("HOME").map(|home| PathBuf::from(home).join(".local/share"))
                    })
                    .unwrap_or_default()
                    .join("flatpak")
            })
    } else {
        variable("FLATPAK_SYSTEM_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/var/lib/flatpak"))
    };
    installation.join(format!("repo/{remote}.trustedkeys.gpg"))
}

/// The bytes in a folder and everything under it.
fn folder_bytes(path: &Path) -> u64 {
    let Ok(listing) = std::fs::read_dir(path) else {
        return 0;
    };
    listing
        .flatten()
        .map(|item| match item.file_type() {
            Ok(kind) if kind.is_dir() => folder_bytes(&item.path()),
            Ok(kind) if kind.is_file() => item.metadata().map(|data| data.len()).unwrap_or(0),
            _ => 0,
        })
        .sum()
}

fn dnf_download(names: &[String], complete: bool, into: &Path) -> bool {
    let mut command = Command::new("dnf");
    command.args(["download", "--resolve"]);
    if complete {
        command.arg("--alldeps");
    }
    command.arg("--destdir").arg(into).arg("--").args(names);
    command.status().is_ok_and(|status| status.success())
}

pub fn run(
    command: BundleCommand,
    catalog: &Catalog,
    platform: &LinuxInfo,
    as_json: bool,
) -> Result<()> {
    match command {
        BundleCommand::Create {
            directory,
            ids,
            complete,
            dry_run,
        } => {
            if as_json && !dry_run {
                bail!("--json shows bundles only with --dry-run");
            }
            for id in &ids {
                crate::find(catalog, id)?;
            }
            let scratch = tempfile_dir()?;
            let found =
                bundle::contents(&ids, catalog, platform, complete, &doctor::Here, &scratch)?;
            let _ = std::fs::remove_dir_all(&scratch);
            if as_json {
                crate::json::print(
                    "bundle",
                    serde_json::json!({
                        "entries": found.entries,
                        "downloads": found.downloads.iter().map(|download| serde_json::json!({
                            "url": download.url,
                            "file": download.file,
                            "checksum": download.checksum,
                        })).collect::<Vec<_>>(),
                        "dnf": found.dnf,
                        "flatpaks": found.flatpaks,
                        "complete": complete,
                    }),
                );
                return Ok(());
            }
            for entry in &found.entries {
                match (&entry.network, &entry.flatpak) {
                    (None, Some(id)) => {
                        println!("Offline: {} (the Flatpak {id})", printable(&entry.name))
                    }
                    (None, None) => println!("Offline: {}", printable(&entry.name)),
                    (Some(why), _) => {
                        println!("Needs the network: {} ({why})", printable(&entry.name))
                    }
                }
            }
            let files = found
                .downloads
                .iter()
                .filter(|download| !download.file.ends_with(".sig"))
                .count();
            if !found.dnf.is_empty() {
                println!(
                    "dnf downloads {} with their dependencies.",
                    found.dnf.join(", ")
                );
            } else if files > 0 || found.flatpaks.is_empty() {
                println!(
                    "{files} package files to download{}.",
                    if complete {
                        ", every dependency"
                    } else {
                        ", with the dependencies this machine lacks"
                    }
                );
            }
            if !found.flatpaks.is_empty() {
                println!(
                    "flatpak create-usb copies {}, with the runtimes they need, from this machine's installation.",
                    found
                        .flatpaks
                        .iter()
                        .map(|flatpak| printable(&flatpak.reference))
                        .collect::<Vec<_>>()
                        .join(", ")
                );
            }
            if dry_run {
                for download in &found.downloads {
                    println!("  {}", printable(&download.url));
                }
                println!("Dry run: nothing was downloaded.");
                return Ok(());
            }
            if !found.entries.iter().any(|entry| entry.offline) {
                bail!("none of these entries can be bundled; they all need the network");
            }
            let tools = bundle::Tools {
                system: &doctor::Here,
                download: &curl,
                dnf_download: &dnf_download,
                create_usb: &create_usb,
                flatpak_key: &flatpak_key,
            };
            let this = std::env::current_exe().context("cannot find the HelixForge executable")?;
            let build = crate::inspect::build();
            let manifest = bundle::create(
                &directory,
                &found,
                platform,
                complete,
                Some((&this, crate::VERSION, build.release)),
                history::now(),
                &tools,
            )?;
            let bytes: u64 = manifest.files.iter().map(|file| file.bytes).sum::<u64>()
                + folder_bytes(&directory.join(bundle::FLATPAKS));
            println!(
                "Made {} ({} files, {:.1} MB) for {} {}.\nCopy the folder over; on the other machine, inside it: ./helixforge bundle install .",
                directory.display(),
                manifest.files.len(),
                bytes as f64 / 1e6,
                manifest.distribution,
                manifest.architecture
            );
            Ok(())
        }
        BundleCommand::Verify { directory } => {
            let manifest = bundle::verify(&directory, platform)?;
            if as_json {
                crate::json::print(
                    "bundle",
                    serde_json::to_value(&manifest).unwrap_or_default(),
                );
                return Ok(());
            }
            println!(
                "{}: made for {} {} with HelixForge {}{}; {} files, every one matching its checksum.",
                directory.display(),
                printable(&manifest.distribution),
                printable(&manifest.architecture),
                printable(&manifest.helixforge),
                if manifest.release {
                    " (a release build)"
                } else {
                    ""
                },
                manifest.files.len()
            );
            for flatpak in &manifest.flatpaks {
                println!(
                    "  Flatpak: {} at commit {}, from {}",
                    printable(&flatpak.reference),
                    &flatpak.commit[..12],
                    printable(&flatpak.remote)
                );
            }
            for entry in &manifest.entries {
                match &entry.network {
                    None => println!("  Offline: {}", printable(&entry.name)),
                    Some(why) => println!(
                        "  Needs the network: {} ({})",
                        printable(&entry.name),
                        printable(why)
                    ),
                }
            }
            Ok(())
        }
        BundleCommand::Install {
            directory,
            trust_bundle,
            dry_run,
            yes,
        } => {
            let manifest = bundle::verify(&directory, platform)?;
            println!(
                "The bundle matches its checksums: {} files for {}.",
                manifest.files.len(),
                printable(&manifest.distribution)
            );
            let planned = bundle::plan_install(
                &directory,
                &manifest,
                &[],
                platform,
                &doctor::Here,
                trust_bundle,
            )?;
            crate::confirm_and_run(catalog, platform, &planned, dry_run, yes, "INSTALL")
        }
    }
}

/// A new, private scratch folder for pacman's empty database: made here
/// and now, never one that already existed, so nobody else's files are used.
fn tempfile_dir() -> Result<PathBuf> {
    use std::os::unix::fs::DirBuilderExt;
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.subsec_nanos())
        .unwrap_or(0);
    let base =
        std::env::temp_dir().join(format!("helixforge-bundle-{}-{nanos}", std::process::id()));
    std::fs::DirBuilder::new()
        .mode(0o700)
        .create(&base)
        .with_context(|| format!("cannot create {}", base.display()))?;
    Ok(base)
}
