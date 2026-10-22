//! Copying a report: through wl-copy, xclip or xsel where the desktop has
//! one, else by asking the terminal itself, with OSC 52, which Ghostty,
//! Kitty, WezTerm, foot and tmux (with set-clipboard) understand.

use std::io::Write;

/// Runs a copying program with its arguments and the text on its input.
pub type Run<'a> = dyn FnMut(&str, &[&str], &str) -> Result<(), String> + 'a;

/// Copy `text` here; says how it was copied.
pub fn copy(text: &str) -> Result<String, String> {
    copy_with(
        text,
        &|name| std::env::var(name).ok(),
        &mut run,
        &mut std::io::stdout(),
    )
}

/// `copy`, with the environment, the copying programs and the terminal
/// supplied.
pub fn copy_with(
    text: &str,
    env: &dyn Fn(&str) -> Option<String>,
    run: &mut Run,
    terminal: &mut dyn Write,
) -> Result<String, String> {
    let set = |name: &str| env(name).is_some_and(|value| !value.is_empty());
    let mut tools: Vec<(&str, &[&str])> = Vec::new();
    if set("WAYLAND_DISPLAY") {
        tools.push(("wl-copy", &[]));
    }
    if set("DISPLAY") {
        tools.push(("xclip", &["-selection", "clipboard"]));
        tools.push(("xsel", &["--clipboard", "--input"]));
    }
    for (program, args) in tools {
        if run(program, args, text).is_ok() {
            return Ok(format!("Copied with {program}."));
        }
    }
    terminal
        .write_all(osc52(text).as_bytes())
        .and_then(|()| terminal.flush())
        .map_err(|error| format!("The terminal could not be asked to copy it: {error}"))?;
    Ok("Sent to your terminal's clipboard (OSC 52); a terminal that does not allow that ignores it.".to_string())
}

/// The escape sequence that asks the terminal to put `text` on the clipboard.
pub fn osc52(text: &str) -> String {
    format!("\x1b]52;c;{}\x07", base64(text.as_bytes()))
}

fn base64(bytes: &[u8]) -> String {
    const LETTERS: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let word = (u32::from(chunk[0]) << 16)
            | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
            | u32::from(*chunk.get(2).unwrap_or(&0));
        for (position, shift) in [18, 12, 6, 0].into_iter().enumerate() {
            if position <= chunk.len() {
                out.push(LETTERS[(word >> shift & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// Run a copying program with `text` on its standard input; it keeps the
/// clipboard itself after it returns.
fn run(program: &str, args: &[&str], text: &str) -> Result<(), String> {
    use std::process::{Command, Stdio};
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| error.to_string())?;
    let written = child
        .stdin
        .take()
        .map(|mut input| input.write_all(text.as_bytes()))
        .unwrap_or(Ok(()));
    let status = child.wait().map_err(|error| error.to_string())?;
    written.map_err(|error| error.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{program} exited with {status}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_pads_as_the_standard_says() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
        assert_eq!(osc52("hi"), "\x1b]52;c;aGk=\x07");
    }

    #[test]
    fn a_desktop_tool_copies_first_and_the_terminal_otherwise() {
        let wayland = |name: &str| (name == "WAYLAND_DISPLAY").then(|| "wayland-0".to_string());
        let mut ran = Vec::new();
        let mut terminal = Vec::new();
        let said = copy_with(
            "report",
            &wayland,
            &mut |program, args, text| {
                ran.push(format!("{program} {} <{text}", args.join(" ")));
                Ok(())
            },
            &mut terminal,
        );
        assert_eq!(said.unwrap(), "Copied with wl-copy.");
        assert_eq!(ran, ["wl-copy  <report"]);
        assert!(terminal.is_empty());

        // On X, without xclip: xsel; with neither, or no display: OSC 52.
        let x11 = |name: &str| (name == "DISPLAY").then(|| ":0".to_string());
        let mut ran = Vec::new();
        let said = copy_with(
            "report",
            &x11,
            &mut |program, _, _| {
                ran.push(program.to_string());
                if program == "xsel" {
                    Ok(())
                } else {
                    Err("not found".into())
                }
            },
            &mut terminal,
        );
        assert_eq!(said.unwrap(), "Copied with xsel.");
        assert_eq!(ran, ["xclip", "xsel"]);
        let said = copy_with(
            "report",
            &|_| None,
            &mut |_, _, _| unreachable!(),
            &mut terminal,
        );
        assert!(said.unwrap().contains("OSC 52"));
        assert_eq!(String::from_utf8(terminal).unwrap(), osc52("report"));
    }
}
