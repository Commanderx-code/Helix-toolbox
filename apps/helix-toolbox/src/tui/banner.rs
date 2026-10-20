//! The Helix Toolbox emblem, drawn as a picture in the header of terminals that
//! can show one (with Kitty's graphics or Sixel), with the name beside it in
//! the theme's colors; the others keep the text header.

use ratatui::layout::Size;
use ratatui_image::{
    FontSize, Resize,
    picker::{Picker, ProtocolType},
    protocol::Protocol,
};
use std::{
    fs::File,
    io::{Read, Write},
    os::fd::AsFd,
    time::{Duration, Instant},
};

/// The logo, `assets/icon.svg`, rendered by `scripts/icons.sh`, small so the
/// program stays light. The whole banner, at four rows, is too small to read.
const BANNER: &[u8] = include_bytes!("emblem.png");

/// The height of the emblem in rows.
pub const ROWS: u16 = 4;

/// Kitty's graphics query, the cell size in pixels, then the device
/// attributes, which every terminal answers, so the last reply is known.
const QUERY: &[u8] = b"\x1b_Gi=31,s=1,v=1,a=q,t=d,f=24;AAAA\x1b\\\x1b[16t\x1b[c";

/// How long to wait for the terminal's answers.
const WAIT: Duration = Duration::from_millis(400);

/// What a terminal said it can draw.
#[derive(Debug, Default, PartialEq)]
pub struct Answers {
    pub kitty: bool,
    pub sixel: bool,
    /// Width and height of a cell in pixels.
    pub cell: Option<(u16, u16)>,
    /// The device attributes arrived, so nothing else will.
    pub complete: bool,
}

impl Answers {
    /// Read the replies to `QUERY`.
    pub fn parse(replies: &[u8]) -> Self {
        let text = String::from_utf8_lossy(replies);
        let mut answers = Self {
            kitty: text.contains("\x1b_Gi=31;OK"),
            ..Self::default()
        };
        for reply in text.split('\x1b').filter_map(|part| part.strip_prefix('[')) {
            if let Some(cell) = reply
                .strip_prefix("6;")
                .and_then(|rest| rest.split_once('t'))
            {
                let mut numbers = cell.0.split(';').map(str::parse::<u16>);
                if let (Some(Ok(height)), Some(Ok(width))) = (numbers.next(), numbers.next())
                    && height > 0
                    && width > 0
                {
                    answers.cell = Some((width, height));
                }
            } else if let Some(attributes) = reply
                .strip_prefix('?')
                .and_then(|rest| rest.split_once('c'))
            {
                answers.complete = true;
                answers.sixel = attributes.0.split(';').any(|value| value == "4");
            }
        }
        answers
    }

    /// The protocol to draw with, if any. Konsole and WezTerm answer Kitty's
    /// query but cannot place its images with Unicode placeholders, which the
    /// banner needs; WezTerm draws Sixel well, Konsole does not reliably.
    fn protocol(&self, terminal: Terminal) -> Option<ProtocolType> {
        match terminal {
            Terminal::Konsole => None,
            Terminal::WezTerm => self.sixel.then_some(ProtocolType::Sixel),
            Terminal::Other if self.kitty => Some(ProtocolType::Kitty),
            Terminal::Other => self.sixel.then_some(ProtocolType::Sixel),
        }
    }
}

/// Terminals whose answers need a second look.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Terminal {
    Konsole,
    WezTerm,
    Other,
}

/// Ask the terminal what it can draw, after the terminal is set up and before
/// the first key is read. Returns None when it shows no pictures, does not
/// answer in time, or runs inside tmux or screen, which do not pass pictures
/// on by default.
pub fn detect() -> Option<Picker> {
    let set = |name| std::env::var_os(name).is_some_and(|value| !value.is_empty());
    if set("TMUX") || set("STY") || std::env::var("TERM").is_ok_and(|term| term == "linux") {
        return None;
    }
    let mut terminal = File::options()
        .read(true)
        .write(true)
        .open("/dev/tty")
        .ok()?;
    terminal.write_all(QUERY).ok()?;
    terminal.flush().ok()?;
    let replies = read_replies(&mut terminal);
    let answers = Answers::parse(&replies);
    tracing::debug!(?answers, "terminal graphics");
    let terminal = if set("KONSOLE_VERSION") {
        Terminal::Konsole
    } else if set("WEZTERM_EXECUTABLE") {
        Terminal::WezTerm
    } else {
        Terminal::Other
    };
    let protocol = answers.protocol(terminal)?;
    // Some terminals give the cell size only with the window's.
    let (width, height) = answers.cell.or_else(|| {
        let window = crossterm::terminal::window_size().ok()?;
        (window.width > 0 && window.columns > 0 && window.rows > 0)
            .then(|| (window.width / window.columns, window.height / window.rows))
    })?;
    #[allow(deprecated)] // the replacement queries standard input itself
    let mut picker = Picker::from_fontsize(FontSize::new(width, height));
    picker.set_protocol_type(protocol);
    Some(picker)
}

/// Read until the device attributes arrive or the wait ends; the terminal is
/// in raw mode, so the replies are not echoed.
fn read_replies(terminal: &mut File) -> Vec<u8> {
    use nix::poll::{PollFd, PollFlags, PollTimeout, poll};
    let deadline = Instant::now() + WAIT;
    let mut replies = Vec::new();
    while !Answers::parse(&replies).complete {
        let left = deadline.saturating_duration_since(Instant::now());
        let Ok(left) = PollTimeout::try_from(left) else {
            break;
        };
        if left == PollTimeout::ZERO {
            break;
        }
        let mut ready = [PollFd::new(terminal.as_fd(), PollFlags::POLLIN)];
        if !matches!(poll(&mut ready, left), Ok(1..)) {
            break;
        }
        let mut chunk = [0; 256];
        match terminal.read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(read) => replies.extend_from_slice(&chunk[..read]),
        }
    }
    replies
}

/// The emblem encoded for this terminal, `ROWS` high.
pub fn encode(picker: &Picker) -> Option<Protocol> {
    let image = image::load_from_memory_with_format(BANNER, image::ImageFormat::Png).ok()?;
    let font = picker.font_size();
    let columns = (u32::from(ROWS) * u32::from(font.height) * image.width())
        .div_ceil(image.height() * u32::from(font.width).max(1));
    picker
        .new_protocol(
            image,
            Size::new(u16::try_from(columns).ok()?, ROWS),
            Resize::Fit(None),
        )
        .inspect_err(|error| tracing::debug!(%error, "the banner could not be encoded"))
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_kitty_terminal_is_read_from_its_answers() {
        let answers = Answers::parse(b"\x1b_Gi=31;OK\x1b\\\x1b[6;19;9t\x1b[?62;22c");
        assert_eq!(
            answers,
            Answers {
                kitty: true,
                sixel: false,
                cell: Some((9, 19)),
                complete: true,
            }
        );
        assert_eq!(answers.protocol(Terminal::Other), Some(ProtocolType::Kitty));
    }

    #[test]
    fn sixel_comes_from_the_device_attributes() {
        let answers = Answers::parse(b"\x1b[6;20;10t\x1b[?62;4;22c");
        assert!(answers.sixel && !answers.kitty && answers.complete);
        assert_eq!(answers.cell, Some((10, 20)));
        assert_eq!(answers.protocol(Terminal::Other), Some(ProtocolType::Sixel));
    }

    #[test]
    fn plain_terminals_and_konsole_keep_the_text_header() {
        let plain = Answers::parse(b"\x1b[?1;2c");
        assert!(plain.complete);
        assert_eq!(plain.protocol(Terminal::Other), None);
        let both = Answers::parse(b"\x1b_Gi=31;OK\x1b\\\x1b[6;19;9t\x1b[?62;4c");
        assert_eq!(both.protocol(Terminal::Konsole), None);
        assert_eq!(both.protocol(Terminal::WezTerm), Some(ProtocolType::Sixel));
    }

    #[test]
    fn half_an_answer_is_not_complete() {
        let answers = Answers::parse(b"\x1b_Gi=31;OK\x1b\\\x1b[6;19");
        assert!(answers.kitty);
        assert!(!answers.complete);
        assert_eq!(answers.cell, None);
    }

    #[test]
    fn the_emblem_is_four_rows_and_keeps_its_shape() {
        #[allow(deprecated)]
        let mut picker = Picker::from_fontsize(FontSize::new(9, 19));
        picker.set_protocol_type(ProtocolType::Kitty);
        let banner = encode(&picker).expect("the built-in emblem encodes");
        assert_eq!(banner.size().height, ROWS);
        // 4 rows of 19 pixels at the emblem's nearly square shape, in 9-pixel
        // columns.
        assert!(
            (8..=10).contains(&banner.size().width),
            "{:?}",
            banner.size()
        );
    }
}
