//! The terminal interface: tabs on the left, the entry list on the right,
//! hints along the bottom, and popups over the list for text, confirmation and
//! running commands.

mod app;
mod banner;
mod clipboard;
mod effects;
mod hints;
pub mod icons;
mod mouse;
mod palette;
mod popup;
mod running;
pub mod settings;
pub mod theme;
mod wizard;

use crate::TuiArgs;
use anyhow::Result;
use app::App;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyEventKind},
    execute,
};
use helix_toolbox_catalog::Catalog;
use helix_toolbox_platform::LinuxInfo;
use std::time::Duration;

pub fn run(catalog: Catalog, platform: LinuxInfo, args: TuiArgs) -> Result<()> {
    let path = settings::path();
    let look = path.as_deref().map(settings::load).unwrap_or_default();
    let app = App::new(catalog, platform, &args)
        .remembering(look, path, &args)
        .with_status_checks();
    // Before the terminal is taken over, so a wrong name is said plainly.
    let opens = match args.screen.as_deref() {
        Some(name) => Some(app.screen_named(name).ok_or_else(|| {
            anyhow::anyhow!(
                "there is no screen named {name}; --screen takes {}",
                app.screen_names()
            )
        })?),
        None => None,
    };
    let mut terminal = ratatui::init();
    // The terminal stops reporting the mouse however the interface ends,
    // a panic included.
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = execute!(std::io::stdout(), DisableMouseCapture);
        hook(info);
    }));
    // After the terminal is set up and before the first key is read.
    let banner = banner::detect().and_then(|picker| banner::encode(&picker));
    let mut app = app.with_banner(banner);
    // The first time Helix Toolbox starts, it offers the setup wizard.
    // The screen asked for, with the wizard over it on the first start.
    if let Some(screen) = opens {
        app.open_on(screen);
    }
    if app.first_run() {
        app.open_wizard(true);
    }
    let mut mouse = Mouse(false);
    let result = event_loop(&mut terminal, &mut app, &mut mouse);
    mouse.set(false);
    ratatui::restore();
    result
}

/// Whether the terminal reports the mouse, as the setting asks.
struct Mouse(bool);

impl Mouse {
    fn set(&mut self, on: bool) {
        if on == self.0 {
            return;
        }
        let done = if on {
            execute!(std::io::stdout(), EnableMouseCapture)
        } else {
            execute!(std::io::stdout(), DisableMouseCapture)
        };
        if let Err(error) = done {
            tracing::warn!(%error, "the terminal's mouse reporting did not change");
        }
        self.0 = on;
    }
}

fn event_loop(
    terminal: &mut ratatui::DefaultTerminal,
    app: &mut App,
    mouse: &mut Mouse,
) -> Result<()> {
    mouse.set(app.mouse());
    terminal.draw(|frame| app.draw(frame))?;
    loop {
        if !event::poll(Duration::from_millis(10))? {
            // A run's output and end arrive as execution events.
            if app.needs_redraw() {
                terminal.draw(|frame| app.draw(frame))?;
            }
            continue;
        }
        let going_on = match event::read()? {
            Event::Key(key)
                if key.kind == KeyEventKind::Press || key.kind == KeyEventKind::Repeat =>
            {
                app.handle_key(&key)
            }
            Event::Mouse(event) => app.handle_mouse(&event),
            Event::Resize(..) => true,
            _ => continue,
        };
        if !going_on {
            return Ok(());
        }
        // A configuration to edit: the terminal goes to the user's editor,
        // then comes back.
        if let Some(request) = app.take_edit() {
            mouse.set(false);
            ratatui::restore();
            let closed = std::process::Command::new("sh")
                .arg("-c")
                .arg(format!("{} \"$1\"", request.editor))
                .arg("sh")
                .arg(&request.copy)
                .status()
                .map_err(|error| format!("{} cannot start: {error}", request.editor))
                .and_then(|status| {
                    if status.success() {
                        Ok(())
                    } else {
                        Err(format!("{} exited with {status}", request.editor))
                    }
                });
            *terminal = ratatui::init();
            terminal.clear()?;
            app.edited(request, closed);
        }
        mouse.set(app.mouse());
        terminal.draw(|frame| app.draw(frame))?;
    }
}
