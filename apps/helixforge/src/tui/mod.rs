//! The terminal interface: tabs on the left, the entry list on the right,
//! hints along the bottom, and popups over the list for text, confirmation and
//! running commands.

mod app;
mod hints;
pub mod icons;
mod popup;
mod running;
pub mod settings;
mod sysinfo;
pub mod theme;

use crate::TuiArgs;
use anyhow::Result;
use app::App;
use crossterm::event::{self, Event, KeyEventKind};
use helixforge_catalog::Catalog;
use helixforge_engine::pty::OUTPUT_CHANGED;
use helixforge_platform::LinuxInfo;
use std::{sync::atomic::Ordering, time::Duration};

pub fn run(catalog: Catalog, platform: LinuxInfo, args: TuiArgs) -> Result<()> {
    let path = settings::path();
    let look = path.as_deref().map(settings::load).unwrap_or_default();
    let mut app = App::new(catalog, platform, &args).remembering(look, path, &args);
    let mut terminal = ratatui::init();
    let result = event_loop(&mut terminal, &mut app);
    ratatui::restore();
    result
}

fn event_loop(terminal: &mut ratatui::DefaultTerminal, app: &mut App) -> Result<()> {
    terminal.draw(|frame| app.draw(frame))?;
    loop {
        if !event::poll(Duration::from_millis(10))? {
            let output_changed = OUTPUT_CHANGED
                .compare_exchange(true, false, Ordering::AcqRel, Ordering::Acquire)
                .is_ok();
            if output_changed || app.needs_redraw() {
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
