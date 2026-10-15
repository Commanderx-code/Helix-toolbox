use clap::ValueEnum;
use ratatui::style::{Color, Style};
use serde::{Deserialize, Serialize};

/// HelixForge's cyan and purple palette. `Default` draws over the terminal's
/// own background, so a wallpaper or transparency shows through; `Solid` paints
/// the dark panels of the dashboard design; `Compatible` keeps to the sixteen
/// basic colours for terminals that render true colour poorly.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, ValueEnum, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    #[default]
    Default,
    Solid,
    Compatible,
}

impl Theme {
    pub fn next(self) -> Self {
        match self {
            Theme::Default => Theme::Solid,
            Theme::Solid => Theme::Compatible,
            Theme::Compatible => Theme::Default,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Theme::Default => "default",
            Theme::Solid => "solid",
            Theme::Compatible => "compatible",
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

    pub fn failure(self) -> Color {
        self.pick(Color::Rgb(255, 120, 140), Color::Red)
    }

    pub fn disabled(self) -> Color {
        Color::DarkGray
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
        Style::default()
            .bg(self.selected())
            .fg(self.on_selected())
            .bold()
    }

    fn pick(self, rich: Color, compatible: Color) -> Color {
        match self {
            Theme::Default | Theme::Solid => rich,
            Theme::Compatible => compatible,
        }
    }
}
