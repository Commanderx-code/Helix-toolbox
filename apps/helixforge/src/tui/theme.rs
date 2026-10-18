use clap::ValueEnum;
use ratatui::style::{Color, Style};
use serde::{Deserialize, Serialize};

/// HelixForge's cyan and purple palette. `Solid`, the default, paints the dark
/// panels of the dashboard design; `Transparent` draws over the terminal's own
/// background, so a wallpaper or transparency shows through; `Compatible`
/// keeps to the sixteen basic colors for terminals that render true color
/// poorly; `Monochrome` uses no color at all, only bold and reverse video,
/// and is what `NO_COLOR` asks for.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, ValueEnum, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    #[default]
    Solid,
    /// Called `default` before Solid became the default; the old name is
    /// still read from saved settings and the command line.
    #[serde(alias = "default")]
    #[value(alias = "default")]
    Transparent,
    Compatible,
    Monochrome,
}

impl Theme {
    pub fn next(self) -> Self {
        match self {
            Theme::Solid => Theme::Transparent,
            Theme::Transparent => Theme::Compatible,
            Theme::Compatible => Theme::Monochrome,
            Theme::Monochrome => Theme::Solid,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Theme::Solid => "solid",
            Theme::Transparent => "transparent",
            Theme::Compatible => "compatible",
            Theme::Monochrome => "monochrome",
        }
    }

    /// Group rows.
    pub fn group(self) -> Color {
        self.pick(Color::Rgb(104, 232, 246), Color::Cyan)
    }

    /// Entry rows and ordinary text.
    pub fn entry(self) -> Color {
        self.pick(Color::Rgb(231, 236, 250), Color::White)
    }

    /// Titles and the wordmark's second half.
    pub fn title(self) -> Color {
        self.pick(Color::Rgb(177, 139, 255), Color::Magenta)
    }

    pub fn focused(self) -> Color {
        self.pick(Color::Rgb(104, 232, 246), Color::Cyan)
    }

    pub fn unfocused(self) -> Color {
        self.pick(Color::Rgb(143, 160, 179), Color::DarkGray)
    }

    pub fn success(self) -> Color {
        self.pick(Color::Rgb(120, 230, 160), Color::Green)
    }

    /// Something that wants attention without being wrong: an update.
    pub fn warning(self) -> Color {
        self.pick(Color::Rgb(250, 204, 102), Color::Yellow)
    }

    pub fn failure(self) -> Color {
        self.pick(Color::Rgb(255, 120, 140), Color::Red)
    }

    pub fn disabled(self) -> Color {
        self.pick(Color::DarkGray, Color::DarkGray)
    }

    pub fn muted(self) -> Color {
        self.pick(Color::Rgb(143, 160, 179), Color::Gray)
    }

    /// Section labels such as MAIN MENU, quieter than muted text.
    pub fn faint(self) -> Color {
        self.pick(Color::Rgb(110, 124, 145), Color::DarkGray)
    }

    /// Card borders and divider lines.
    pub fn line(self) -> Color {
        self.pick(Color::Rgb(52, 64, 84), Color::DarkGray)
    }

    /// The fill of a selected card or row.
    pub fn selected(self) -> Color {
        self.pick(Color::Rgb(31, 53, 80), Color::Blue)
    }

    /// The border of a selected card that does not have the focus.
    pub fn selected_line(self) -> Color {
        self.pick(Color::Rgb(78, 110, 148), Color::Gray)
    }

    /// Text on a selected card or row.
    pub fn on_selected(self) -> Color {
        self.pick(Color::Rgb(240, 246, 255), Color::White)
    }

    /// Text on a highlighted row.
    pub fn on_highlight(self) -> Color {
        self.pick(Color::Rgb(11, 15, 28), Color::Black)
    }

    /// The screen behind everything: the terminal's own, or the design's
    /// dark navy for `Solid`.
    pub fn base(self) -> Style {
        match self {
            Theme::Solid => Style::default().bg(Color::Rgb(13, 17, 26)),
            _ => Style::default(),
        }
    }

    /// The inside of an unselected card.
    pub fn card(self) -> Style {
        match self {
            Theme::Solid => Style::default().bg(Color::Rgb(20, 26, 38)),
            _ => Style::default(),
        }
    }

    /// A selected row or card.
    pub fn selection(self) -> Style {
        match self {
            Theme::Monochrome => Style::default().reversed().bold(),
            _ => Style::default()
                .bg(self.selected())
                .fg(self.on_selected())
                .bold(),
        }
    }

    /// The fill behind a selected card or sidebar row.
    pub fn selected_fill(self) -> Style {
        match self {
            Theme::Monochrome => Style::default().reversed(),
            _ => Style::default().bg(self.selected()),
        }
    }

    /// A button in a popup, in the color that says what it does.
    pub fn button(self, color: Color) -> Style {
        match self {
            Theme::Monochrome => Style::default().reversed().bold(),
            _ => Style::default().bg(color).fg(self.on_highlight()).bold(),
        }
    }

    /// Whether this theme draws color; the banner picture needs it.
    pub fn colorful(self) -> bool {
        matches!(self, Theme::Solid | Theme::Transparent)
    }

    fn pick(self, rich: Color, compatible: Color) -> Color {
        match self {
            Theme::Solid | Theme::Transparent => rich,
            Theme::Compatible => compatible,
            Theme::Monochrome => Color::Reset,
        }
    }
}

/// Whether `NO_COLOR` asks for no color: set, and not empty
/// (<https://no-color.org>).
pub fn no_color() -> bool {
    std::env::var_os("NO_COLOR").is_some_and(|value| !value.is_empty())
}
