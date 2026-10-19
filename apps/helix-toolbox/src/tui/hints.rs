//! The key hints shown along the bottom of the screen.

pub struct Hint {
    keys: &'static [&'static str],
    text: &'static str,
}

impl Hint {
    pub const fn new(keys: &'static [&'static str], text: &'static str) -> Self {
        Self { keys, text }
    }

    pub fn keys(&self) -> &'static [&'static str] {
        self.keys
    }

    pub fn text(&self) -> &'static str {
        self.text
    }
}
