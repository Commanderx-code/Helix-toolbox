use clap::ValueEnum;
use ratatui::style::Color;

/// HelixForge's cyan and purple palette, and a reduced one for terminals that
/// render true colour poorly.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, ValueEnum)]
pub enum Theme {
    #[default]
    Default,
    Compatible,
}

impl Theme {
    pub fn next(self) -> Self {
        match self {
            Theme::Default => Theme::Compatible,
            Theme::Compatible => Theme::Default,
        }
    }

    /// Group rows.
    pub fn group(self) -> Color {
        self.pick(Color::Rgb(104, 232, 246), Color::Cyan)
    }

    /// Entry rows.
    pub fn entry(self) -> Color {
        self.pick(Color::Rgb(231, 236, 250), Color::White)
    }

    /// Titles and the tab list.
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
        self.pick(Color::Rgb(90, 104, 128), Color::DarkGray)
    }

    /// Text on a highlighted row.
    pub fn on_highlight(self) -> Color {
        self.pick(Color::Rgb(11, 15, 28), Color::Black)
    }

    fn pick(self, default: Color, compatible: Color) -> Color {
        match self {
            Theme::Default => default,
            Theme::Compatible => compatible,
        }
    }
}
