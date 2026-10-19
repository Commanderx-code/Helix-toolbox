//! `htb`, the short name for `helix-toolbox`: runs the `helix-toolbox`
//! installed beside it with the same arguments. The packages make `htb` a
//! link to `helix-toolbox` instead; this is for `cargo install`.

use std::{os::unix::process::CommandExt, process::Command};

fn main() {
    let program = std::env::current_exe()
        .ok()
        .and_then(|this| Some(this.parent()?.join("helix-toolbox")))
        .filter(|program| program.is_file());
    let Some(program) = program else {
        eprintln!("htb: helix-toolbox is not installed beside it");
        std::process::exit(127);
    };
    let error = Command::new(&program)
        .args(std::env::args_os().skip(1))
        .exec();
    eprintln!("htb: cannot run {}: {error}", program.display());
    std::process::exit(126);
}
