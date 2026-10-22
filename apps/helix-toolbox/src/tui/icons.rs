//! Icons from Nerd Fonts, with plain text to fall back on. Nerd Fonts keep
//! Font Awesome's code points, which are the ones used here.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icon {
    Dashboard,
    Applications,
    Plan,
    Configurations,
    Doctor,
    Maintenance,
    Security,
    Profiles,
    History,
    Backups,
    Favorites,
    Updates,
    Drift,
    Reliability,
    Hardware,
    Desktop,
    Bundles,
    Services,
    Storage,
    Settings,
    Area,
    Group,
    Entry,
    Queued,
    Search,
    Overview,
    Chevron,
    Logo,
}

impl Icon {
    /// The glyph, or the plain text used without a Nerd Font.
    pub fn glyph(self, nerd: bool) -> &'static str {
        if nerd {
            match self {
                Icon::Dashboard => "\u{f0e4}",      // tachometer
                Icon::Applications => "\u{f1b2}",   // cube
                Icon::Plan => "\u{f0ae}",           // tasks
                Icon::Configurations => "\u{f1de}", // sliders
                Icon::Doctor => "\u{f0f1}",         // stethoscope
                Icon::Maintenance => "\u{f0ad}",    // wrench
                Icon::Security => "\u{f132}",       // shield
                Icon::Profiles => "\u{f1b3}",       // cubes
                Icon::History => "\u{f1da}",        // history
                Icon::Backups => "\u{f187}",        // archive
                Icon::Favorites => "\u{f005}",      // star
                Icon::Updates => "\u{f021}",        // refresh
                Icon::Drift => "\u{f0ec}",          // exchange
                Icon::Reliability => "\u{f0c3}",    // flask
                Icon::Hardware => "\u{f2db}",       // microchip
                Icon::Desktop => "\u{f108}",        // desktop
                Icon::Bundles => "\u{f1c6}",        // file archive
                Icon::Services => "\u{f085}",       // cogs
                Icon::Storage => "\u{f1c0}",        // database
                Icon::Settings => "\u{f013}",       // cog
                Icon::Area => "\u{f07b}",           // folder
                Icon::Group => "\u{f07b}",          // folder
                Icon::Entry => "\u{f10c}",          // circle outline
                Icon::Queued => "\u{f058}",         // check circle
                Icon::Search => "\u{f002}",         // magnifier
                Icon::Overview => "\u{f108}",       // desktop
                Icon::Chevron => "\u{f054}",        // chevron right
                Icon::Logo => "\u{f192}",           // dot in a circle
            }
        } else {
            match self {
                Icon::Group => "[D]",
                Icon::Entry => "[*]",
                Icon::Queued => "[*]",
                Icon::Chevron => ">",
                Icon::Logo => "⬡",
                _ => "",
            }
        }
    }

    /// The icon for a sidebar screen or catalog area, by its id.
    pub fn for_area(id: &str) -> Self {
        match id {
            "applications" => Icon::Applications,
            "configurations" => Icon::Configurations,
            "doctor" => Icon::Doctor,
            "maintenance" => Icon::Maintenance,
            "security" => Icon::Security,
            "profiles" => Icon::Profiles,
            _ => Icon::Area,
        }
    }
}
