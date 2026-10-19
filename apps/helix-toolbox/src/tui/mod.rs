//! The terminal interface: tabs on the left, the entry list on the right,
//! hints along the bottom, and popups over the list for text, confirmation and
//! running commands.

mod app;
mod banner;
mod effects;
mod hints;
pub mod icons;
mod palette;
mod popup;
mod running;
pub mod settings;
pub mod theme;
mod wizard;

use crate::TuiArgs;
use anyhow::Result;
use app::App;
use crossterm::event::{self, Event, KeyEventKind};
use helix_toolbox_catalog::Catalog;
use helix_toolbox_platform::LinuxInfo;
use std::time::Duration;

pub fn run(catalog: Catalog, platform: LinuxInfo, args: TuiArgs) -> Result<()> {
    let path = settings::path();
    let look = path.as_deref().map(settings::load).unwrap_or_default();
    let app = App::new(catalog, platform, &args)
        .remembering(look, path, &args)
        .with_status_checks();
    let mut terminal = ratatui::init();
    // After the terminal is set up and before the first key is read.
    let banner = banner::detect().and_then(|picker| banner::encode(&picker));
    let mut app = app.with_banner(banner);
    // The first time Helix Toolbox starts, it offers the setup wizard.
    if app.first_run() {
        app.open_wizard(true);
    }
    let result = event_loop(&mut terminal, &mut app);
    ratatui::restore();
    result
}

fn event_loop(terminal: &mut ratatui::DefaultTerminal, app: &mut App) -> Result<()> {
    terminal.draw(|frame| app.draw(frame))?;
    loop {
        if !event::poll(Duration::from_millis(10))? {
            // A run's output and end arrive as execution events.
            if app.needs_redraw() {
                terminal.draw(|frame| app.draw(frame))?;
            }
            continue;
        }
        match event::read()? {
            Event::Key(key)
                if key.kind == KeyEventKind::Press || key.kind == KeyEventKind::Repeat =>
            {
                if !app.handle_key(&key) {
                    return Ok(());
                }
            }
            Event::Resize(..) => {}
            _ => continue,
        }
        terminal.draw(|frame| app.draw(frame))?;
    }
}
