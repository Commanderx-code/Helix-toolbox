//! The key hints shown along the bottom of the screen.

use ratatui::{
    style::Style,
    text::{Line, Span},
};

pub struct Hint {
    keys: &'static [&'static str],
    text: &'static str,
}

impl Hint {
    pub const fn new(keys: &'static [&'static str], text: &'static str) -> Self {
        Self { keys, text }
    }

    fn spans(&self) -> Vec<Span<'static>> {
        let mut spans = Vec::new();
        for key in self.keys {
            spans.push(Span::raw("["));
            spans.push(Span::styled(*key, Style::default().bold()));
            spans.push(Span::raw("] "));
        }
        spans.push(Span::styled(self.text, Style::default().italic()));
        spans
    }
}

fn width(spans: &[Span]) -> usize {
    spans.iter().map(Span::width).sum()
}

/// Lay the hints out in as many columns as the width allows.
pub fn lines(hints: &[Hint], render_width: u16) -> Vec<Line<'static>> {
    let cells: Vec<Vec<Span<'static>>> = hints.iter().map(Hint::spans).collect();
    let widest = cells.iter().map(|spans| width(spans)).max().unwrap_or(0);
    let columns = (render_width as usize / (widest + 4)).max(1);
    cells
        .chunks(columns)
        .map(|row| {
            let mut line = Vec::new();
            for spans in row {
                let padding = widest - width(spans);
                line.extend(spans.iter().cloned());
                line.push(Span::raw(" ".repeat(padding + 4)));
            }
            Line::from(line)
        })
        .collect()
}
