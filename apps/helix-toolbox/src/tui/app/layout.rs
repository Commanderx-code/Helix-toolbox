//! The frame every screen shares: header, sidebar, Dashboard, footer and hints.

use super::*;

impl App {
    pub(super) fn glyph(&self, icon: Icon) -> &'static str {
        icon.glyph(self.icons)
    }

    /// An icon and the space after it, or nothing when icons are off.
    pub(super) fn icon_gap(&self, icon: Icon) -> String {
        match self.glyph(icon) {
            "" => String::new(),
            glyph => format!("{glyph}  "),
        }
    }

    pub(super) fn menu_icon(&self, screen: Screen) -> Icon {
        match screen {
            Screen::Dashboard => Icon::Dashboard,
            Screen::Plan => Icon::Plan,
            Screen::History => Icon::History,
            Screen::Backups => Icon::Backups,
            Screen::Updates => Icon::Updates,
            Screen::Drift => Icon::Drift,
            Screen::Reliability => Icon::Reliability,
            Screen::Hardware => Icon::Hardware,
            Screen::Desktop => Icon::Desktop,
            Screen::Bundles => Icon::Bundles,
            Screen::Services => Icon::Services,
            Screen::Storage => Icon::Storage,
            Screen::Performance => Icon::Performance,
            Screen::Recovery => Icon::Recovery,
            Screen::Migration => Icon::Migration,
            Screen::Restic => Icon::Restic,
            Screen::Repositories => Icon::Repositories,
            Screen::Configs => Icon::Configs,
            Screen::HomeManager => Icon::HomeManager,
            Screen::Favorites => Icon::Favorites,
            Screen::Doctor => Icon::Doctor,
            Screen::Settings => Icon::Settings,
            Screen::Area(index) => Icon::for_area(&self.catalog.tabs[index].id),
        }
    }

    /// The style of the selected row of a list that has the focus.
    pub(super) fn highlight(&self, focused: bool) -> Style {
        if focused {
            self.theme.selection()
        } else {
            Style::default()
        }
    }

    /// What marks the selected row of a list.
    pub(super) fn marker(&self, focused: bool) -> &'static str {
        match (focused, self.icons) {
            (false, _) => "  ",
            (true, true) => "▌ ",
            (true, false) => HIGHLIGHT,
        }
    }

    pub fn draw(&mut self, frame: &mut Frame) {
        self.hits.get_mut().clear();
        let area = frame.area();
        frame.render_widget(Block::default().style(self.theme.base()), area);
        let too_small = (area.width < MIN_WIDTH || area.height < MIN_HEIGHT)
            && !self.size_bypass
            && self.popup.is_none();
        if too_small {
            let warning = Paragraph::new(format!(
                "The terminal is {} by {}; Helix Toolbox needs {} by {}.\nEnlarge the window, or start with --size-bypass.",
                area.width, area.height, MIN_WIDTH, MIN_HEIGHT
            ))
            .alignment(Alignment::Center)
            .style(Style::default().fg(self.theme.failure()).bold())
            .wrap(Wrap { trim: true });
            let rows = Layout::vertical([
                Constraint::Fill(1),
                Constraint::Length(4),
                Constraint::Fill(1),
            ])
            .split(area);
            frame.render_widget(warning, rows[1]);
            return;
        }
        let header = if self.banner(area.width).is_some() {
            banner::ROWS + 1
        } else {
            HEADER_HEIGHT
        };
        let rows = Layout::vertical([
            Constraint::Length(header),
            Constraint::Min(1),
            Constraint::Length(2),
        ])
        .split(area);
        self.draw_header(frame, rows[0]);
        let columns = Layout::horizontal([Constraint::Length(SIDEBAR_WIDTH), Constraint::Fill(1)])
            .split(rows[1]);
        self.draw_sidebar(frame, columns[0]);
        let main = columns[1];
        self.hits.get_mut().main(main);
        match self.screen {
            Screen::Dashboard => self.draw_dashboard(frame, main),
            Screen::Area(_) => self.draw_area(frame, main),
            Screen::Favorites => self.draw_favorites(frame, main),
            Screen::Plan => self.draw_plan(frame, main),
            Screen::History => self.draw_history(frame, main),
            Screen::Backups => self.draw_backups(frame, main),
            Screen::Updates => self.draw_updates(frame, main),
            Screen::Drift => self.draw_drift(frame, main),
            Screen::Reliability => self.draw_reliability(frame, main),
            Screen::Hardware => self.draw_hardware(frame, main),
            Screen::Desktop => self.draw_desktop(frame, main),
            Screen::Bundles => self.draw_bundles(frame, main),
            Screen::Services => self.draw_services(frame, main),
            Screen::Storage => self.draw_storage(frame, main),
            Screen::Performance => self.draw_performance(frame, main),
            Screen::Recovery => self.draw_recovery(frame, main),
            Screen::Migration => self.draw_migration(frame, main),
            Screen::Restic => self.draw_restic(frame, main),
            Screen::Repositories => self.draw_repos(frame, main),
            Screen::Configs => self.draw_configs(frame, main),
            Screen::HomeManager => self.draw_home(frame, main),
            Screen::Doctor => self.draw_doctor(frame, main),
            Screen::Settings => self.draw_settings(frame, main),
        }
        match &mut self.popup {
            // The keys are wider than the screen beside the sidebar; under
            // the header, which the banner's picture stays drawn over.
            Some(Popup::Text(popup)) if popup.title() == "Keys" => {
                let below = Rect::new(
                    area.x,
                    rows[1].y,
                    area.width,
                    rows[1].height + rows[2].height,
                );
                popup.draw(frame, centered(below, 98, 98), self.theme)
            }
            Some(Popup::Text(popup) | Popup::HardwareReport(popup, _)) => {
                popup.draw(frame, centered(main, 95, 95), self.theme)
            }
            Some(
                Popup::ProfileName(popup)
                | Popup::BaselineName(popup)
                | Popup::FirmwareWord(popup)
                | Popup::LookName(popup)
                | Popup::Ask(popup, _),
            ) => {
                let area = popup.fitted(centered(main, 80, 70));
                popup.draw(frame, area, self.theme)
            }
            Some(Popup::Wizard(popup)) => popup.draw(frame, centered(main, 92, 90), self.theme),
            Some(Popup::Palette(popup)) => {
                let area = popup.fitted(main);
                popup.draw(frame, area, self.theme)
            }
            Some(Popup::Effects(popup)) => popup.draw(frame, centered(main, 96, 96), self.theme),
            Some(Popup::Confirm(popup)) => {
                let area = popup.fitted(centered(main, 80, 70));
                popup.draw(frame, area, self.theme)
            }
            Some(Popup::Running(popup)) => popup.draw(frame, centered(main, 96, 96), self.theme),
            None => {}
        }
        self.draw_footer(frame, rows[2]);
    }

    pub(super) fn draw_header(&self, frame: &mut Frame, area: Rect) {
        let (status, color) = match &self.popup {
            Some(Popup::Running(running)) if !running.is_finished() => {
                ("● RUNNING", self.theme.title())
            }
            _ => ("● SYSTEM READY", self.theme.success()),
        };
        if let Some(banner) = self.banner(area.width) {
            let size = banner.size();
            frame.render_widget(
                Image::new(banner),
                Rect::new(area.x + 2, area.y, size.width, size.height).intersection(area),
            );
            // The name and tagline in the middle two rows, beside the emblem.
            let top = area.y + size.height.saturating_sub(2) / 2;
            let text = Rect::new(
                area.x + size.width + 4,
                top,
                area.width.saturating_sub(size.width + 6),
                2,
            )
            .intersection(area);
            let [helix, toolbox] = WORDMARK;
            let brand = Text::from(vec![
                Line::from(vec![
                    Span::styled(helix, Style::default().fg(self.theme.focused()).bold()),
                    Span::raw("   "),
                    Span::styled(toolbox, Style::default().fg(self.theme.title()).bold()),
                ]),
                Line::styled(TAGLINE, Style::default().fg(self.theme.faint())),
            ]);
            frame.render_widget(Paragraph::new(brand), text);
            let middle = Rect::new(area.x, top, area.width.saturating_sub(2), 1).intersection(area);
            let room = (text.width as usize)
                .saturating_sub(helix.chars().count() + 3 + toolbox.chars().count());
            frame.render_widget(
                Paragraph::new(self.header_status(status, color, room)).alignment(Alignment::Right),
                middle,
            );
            self.rule(frame, area.x, area.bottom().saturating_sub(1), area.width);
            return;
        }
        // The logo tile, then the wordmark and the tagline beside it.
        let tile = Rect::new(area.x + 2, area.y, 5, 3).intersection(area);
        let inside = self.framed(
            frame,
            tile,
            self.theme.selected_line(),
            self.theme.selected_fill(),
        );
        frame.render_widget(
            Paragraph::new(Line::styled(
                self.glyph(Icon::Logo),
                Style::default().fg(self.theme.focused()).bold(),
            ))
            .alignment(Alignment::Center),
            inside,
        );
        let text = Rect::new(
            area.x + 9,
            area.y,
            area.width.saturating_sub(11),
            area.height.min(3),
        )
        .intersection(area);
        let brand = Text::from(vec![
            Line::from(vec![
                Span::styled("HELIX", Style::default().fg(self.theme.focused()).bold()),
                Span::styled(" TOOLBOX", Style::default().fg(self.theme.title()).bold()),
            ]),
            Line::styled(TAGLINE, Style::default().fg(self.theme.faint())),
        ]);
        frame.render_widget(Paragraph::new(brand), text);
        let room = (text.width as usize).saturating_sub("HELIX TOOLBOX".len());
        frame.render_widget(
            Paragraph::new(self.header_status(status, color, room)).alignment(Alignment::Right),
            text,
        );
        self.rule(frame, area.x, area.bottom().saturating_sub(1), area.width);
    }

    /// READY or RUNNING at the right of the header, with the version before
    /// it when `room` columns beside the name hold both.
    pub(super) fn header_status(
        &self,
        status: &'static str,
        color: Color,
        room: usize,
    ) -> Line<'static> {
        const VERSION: &str = concat!("v", env!("CARGO_PKG_VERSION"), "   ");
        let mut spans = Vec::new();
        if VERSION.chars().count() + status.chars().count() + 2 <= room {
            spans.push(Span::styled(
                VERSION,
                Style::default().fg(self.theme.faint()),
            ));
        }
        spans.push(Span::styled(status, Style::default().fg(color).bold()));
        Line::from(spans)
    }

    /// A rounded card whose fill stays inside its border, as in the design;
    /// returns the space inside.
    pub(super) fn framed(&self, frame: &mut Frame, area: Rect, border: Color, fill: Style) -> Rect {
        let block = Block::bordered()
            .border_set(border::ROUNDED)
            .border_style(Style::default().fg(border));
        let inside = block.inner(area);
        frame.render_widget(block, area);
        frame.render_widget(Block::default().style(fill), inside);
        inside
    }

    /// A horizontal divider.
    pub(super) fn rule(&self, frame: &mut Frame, x: u16, y: u16, width: u16) {
        frame.render_widget(
            Paragraph::new("─".repeat(width as usize))
                .style(Style::default().fg(self.theme.line())),
            Rect::new(x, y, width, 1).intersection(frame.area()),
        );
    }

    pub(super) fn draw_sidebar(&mut self, frame: &mut Frame, area: Rect) {
        let focused = self.focus == Focus::Sidebar;
        let edge = Block::default()
            .borders(ratatui::widgets::Borders::RIGHT)
            .border_set(border::PLAIN)
            .border_style(Style::default().fg(self.theme.line()));
        let inner = edge.inner(area);
        frame.render_widget(edge, area);
        let inner = Rect::new(
            inner.x + 1,
            inner.y + 1,
            inner.width.saturating_sub(2),
            inner.height.saturating_sub(1),
        );
        // Menu items by section, in order.
        let mut sections: Vec<(&'static str, Vec<usize>)> = Vec::new();
        for (index, item) in self.menu.iter().enumerate() {
            match sections.last_mut() {
                Some((name, items)) if *name == item.section => items.push(index),
                _ => sections.push((item.section, vec![index])),
            }
        }
        // The first section's items are cards when there is room for them.
        // The main menu's items are cards when they fit with a few lines of
        // the other sections below them, which scroll when they must.
        let first = sections[0].1.len();
        let cards = inner.height as usize >= 1 + 3 * first + 1 + 2 + 1 + 6;
        if !cards {
            self.draw_sidebar_rows(frame, inner, &sections, focused);
            return;
        }
        let (name, items) = &sections[0];
        frame.render_widget(
            Paragraph::new(Line::styled(
                *name,
                Style::default().fg(self.theme.faint()).bold(),
            )),
            Rect::new(inner.x, inner.y, inner.width, 1),
        );
        let mut y = inner.y + 1;
        for &index in items {
            let item = &self.menu[index];
            let current = item.screen == self.screen;
            let label = self.menu_label(item, current, inner.width.saturating_sub(4));
            let border = match (current, focused) {
                (true, true) => self.theme.focused(),
                (true, false) => self.theme.selected_line(),
                _ => self.theme.line(),
            };
            let fill = if current {
                self.theme.selected_fill()
            } else {
                self.theme.card()
            };
            let card = Rect::new(inner.x, y, inner.width, 3);
            self.hits.get_mut().menu(card.intersection(inner), index);
            let inside = self.framed(frame, card, border, fill);
            frame.render_widget(
                Paragraph::new(label).block(Block::default().padding(Padding::horizontal(1))),
                inside,
            );
            y += 3;
        }
        if sections.len() > 1 {
            y += 1;
            self.rule(frame, inner.x, y, inner.width);
            y += 2;
            let rest = Rect::new(inner.x, y, inner.width, inner.bottom().saturating_sub(y));
            self.draw_sidebar_rows(frame, rest, &sections[1..], focused);
        }
    }

    /// A menu item's icon and label, with its count at the right of
    /// `width` when it has one, as the window's sidebar shows.
    pub(super) fn menu_label(&self, item: &MenuItem, current: bool, width: u16) -> Line<'static> {
        let icon = self.icon_gap(self.menu_icon(item.screen));
        let text_style = if current {
            Style::default().fg(self.theme.on_selected()).bold()
        } else {
            Style::default().fg(self.theme.entry())
        };
        let count = self.menu_count(item.screen);
        let used = icon.chars().count() + item.label.chars().count();
        let mut spans = vec![
            Span::styled(
                icon,
                Style::default().fg(if current {
                    self.theme.focused()
                } else {
                    self.theme.muted()
                }),
            ),
            Span::styled(item.label.clone(), text_style),
        ];
        if count > 0 {
            let number = count.to_string();
            let gap = (width as usize).saturating_sub(used + number.chars().count());
            if gap >= 1 {
                // The Install Plan's count stands out, as in the window.
                let color = if item.screen == Screen::Plan {
                    self.theme.focused()
                } else {
                    self.theme.muted()
                };
                spans.push(Span::raw(" ".repeat(gap)));
                spans.push(Span::styled(number, Style::default().fg(color).bold()));
            }
        }
        Line::from(spans)
    }

    /// One menu item as a line, marked when it is the current screen.
    pub(super) fn menu_row(
        &self,
        frame: &mut Frame,
        row: Rect,
        label: Line<'static>,
        current: bool,
        focused: bool,
    ) {
        let edge = if current && focused {
            Span::styled("▌", Style::default().fg(self.theme.focused()))
        } else {
            Span::raw(" ")
        };
        let mut spans = vec![edge, Span::raw(" ")];
        spans.extend(label.spans);
        let style = if current {
            self.theme.selected_fill()
        } else {
            Style::default()
        };
        frame.render_widget(Paragraph::new(Line::from(spans)).style(style), row);
    }

    /// The menu as one line per item: with a rule and space between the
    /// sections when they fit, without them when not, and scrolled to keep
    /// the current screen in view when even that is too tall.
    pub(super) fn draw_sidebar_rows(
        &self,
        frame: &mut Frame,
        inner: Rect,
        sections: &[(&'static str, Vec<usize>)],
        focused: bool,
    ) {
        enum Row {
            Rule,
            Blank,
            Heading(&'static str),
            Item(usize),
        }
        let build = |spaced: bool| {
            let mut rows = Vec::new();
            for (number, (name, items)) in sections.iter().enumerate() {
                if number > 0 && spaced {
                    rows.push(Row::Rule);
                    rows.push(Row::Blank);
                }
                rows.push(Row::Heading(name));
                rows.extend(items.iter().map(|&index| Row::Item(index)));
                if spaced {
                    rows.push(Row::Blank);
                }
            }
            rows
        };
        let height = inner.height as usize;
        let mut rows = build(true);
        // The blank after the last section is not needed to fit.
        if rows.len().saturating_sub(1) > height {
            rows = build(false);
        }
        let current = rows
            .iter()
            .position(
                |row| matches!(row, Row::Item(index) if self.menu[*index].screen == self.screen),
            )
            .unwrap_or(0);
        let offset = (current + 1)
            .saturating_sub(height)
            .min(rows.len().saturating_sub(height));
        for (line, row) in rows.iter().skip(offset).take(height).enumerate() {
            let area = Rect::new(inner.x, inner.y + line as u16, inner.width, 1);
            match row {
                Row::Rule => self.rule(frame, area.x, area.y, area.width),
                Row::Blank => {}
                Row::Heading(name) => frame.render_widget(
                    Paragraph::new(Line::styled(
                        *name,
                        Style::default().fg(self.theme.faint()).bold(),
                    )),
                    area,
                ),
                Row::Item(index) => {
                    self.hits.borrow_mut().menu(area, *index);
                    let item = &self.menu[*index];
                    let current = item.screen == self.screen;
                    self.menu_row(
                        frame,
                        area,
                        self.menu_label(item, current, area.width.saturating_sub(3)),
                        current,
                        focused,
                    );
                }
            }
        }
    }

    /// A screen's title and subtitle; returns the space below them.
    pub(super) fn page(&self, frame: &mut Frame, area: Rect, title: &str, subtitle: &str) -> Rect {
        let inner = Rect::new(
            area.x + 2,
            area.y + 1,
            area.width.saturating_sub(4),
            area.height.saturating_sub(1),
        );
        frame.render_widget(
            Paragraph::new(Text::from(vec![
                Line::styled(
                    title.to_string(),
                    Style::default().fg(self.theme.entry()).bold(),
                ),
                Line::styled(
                    subtitle.to_string(),
                    Style::default().fg(self.theme.muted()),
                ),
            ])),
            inner,
        );
        Rect::new(
            inner.x,
            inner.y + 3,
            inner.width,
            inner.height.saturating_sub(3),
        )
    }

    /// A list's items, their heights noted for the mouse.
    pub(super) fn measure<'a, T: Into<ListItem<'a>>>(
        &self,
        items: impl IntoIterator<Item = T>,
    ) -> Vec<ListItem<'a>> {
        let items: Vec<ListItem> = items.into_iter().map(Into::into).collect();
        self.hits
            .borrow_mut()
            .measured(items.iter().map(|item| item.height() as u16).collect());
        items
    }

    pub(super) fn card(&self, title: &str) -> Block<'static> {
        Block::bordered()
            .border_set(border::ROUNDED)
            .border_style(Style::default().fg(self.theme.line()))
            .title(format!(" {title} "))
            .title_style(Style::default().fg(self.theme.entry()).bold())
            .padding(Padding::horizontal(1))
    }

    /// Fill a card's inside, so the fill never reaches past its rounded
    /// border; draw the card's widget over it afterward.
    pub(super) fn fill(&self, frame: &mut Frame, area: Rect) {
        let inside = Block::bordered().inner(area);
        frame.render_widget(Block::default().style(self.theme.card()), inside);
    }

    pub(super) fn draw_dashboard(&mut self, frame: &mut Frame, area: Rect) {
        let content = self.page(
            frame,
            area,
            "Dashboard",
            "Welcome to your Linux command center.",
        );
        let rows = self.overview.rows.len() as u16;
        // The quick actions in two columns where two of them fit side by
        // side: cards need more room than lines.
        let content_width = content.width;
        let columns = move |card: u16| -> u16 {
            let wide = if card == 1 { 56 } else { 72 };
            if content_width >= wide { 2 } else { 1 }
        };
        let overview_height = |spaced: bool| if spaced { rows * 2 + 1 } else { rows + 2 };
        let actions = |card: u16| (QUICK_ACTIONS.len() as u16).div_ceil(columns(card)) * card;
        // Roomy rows and two-line cards with the counts as tiles when they
        // fit, the counts as a line before smaller cards, tighter otherwise.
        // `tiles` is the counts' height: tiles with a blank line after them,
        // or one line.
        let (tiles, spaced, card) = [
            (4, true, 4),
            (4, false, 4),
            (1, false, 4),
            (4, false, 3),
            (1, false, 3),
            (0, false, 3),
            (1, false, 1),
            (0, false, 1),
        ]
        .into_iter()
        .find(|&(tiles, spaced, card)| {
            tiles + overview_height(spaced) + 3 + actions(card) <= content.height
        })
        .unwrap_or((0, false, 1));
        self.draw_counts(frame, Rect::new(content.x, content.y, content.width, tiles));
        let content = Rect::new(
            content.x,
            content.y + tiles,
            content.width,
            content.height.saturating_sub(tiles),
        );
        let width = content.width.saturating_sub(4) as usize;
        let mut lines: Vec<Line> = Vec::new();
        for (index, (label, value)) in self.overview.rows.iter().enumerate() {
            if spaced && index > 0 {
                lines.push(Line::from(""));
            }
            let gap = width.saturating_sub(label.chars().count() + value.chars().count());
            lines.push(Line::from(vec![
                Span::styled(*label, Style::default().fg(self.theme.muted())),
                Span::raw(" ".repeat(gap)),
                Span::styled(value.clone(), Style::default().fg(self.theme.entry())),
            ]));
        }
        let overview = Rect::new(
            content.x,
            content.y,
            content.width,
            overview_height(spaced).min(content.height),
        );
        let mut block = self.card("System Overview");
        let corner = self.glyph(Icon::Overview);
        if !corner.is_empty() {
            block = block.title(
                Line::styled(
                    format!(" {corner} "),
                    Style::default().fg(self.theme.focused()),
                )
                .right_aligned(),
            );
        }
        self.fill(frame, overview);
        frame.render_widget(Paragraph::new(Text::from(lines)).block(block), overview);
        // A blank line before the heading, unless every action would not fit.
        let gap = u16::from(overview_height(spaced) + 3 + actions(card) <= content.height);
        let heading = overview.bottom() + gap;
        if heading >= content.bottom() {
            return;
        }
        frame.render_widget(
            Paragraph::new(Line::styled(
                "Quick Actions",
                Style::default().fg(self.theme.entry()).bold(),
            )),
            Rect::new(content.x, heading, content.width, 1),
        );
        let list = Rect::new(
            content.x,
            heading + 1,
            content.width,
            content.bottom().saturating_sub(heading + 1),
        );
        let focused = self.focus == Focus::Main;
        let selected = self.main_state.selected().unwrap_or(0);
        let columns = columns(card);
        // Side by side, in reading order, with a column between them.
        let column_width = (list.width + 1) / columns - 1;
        if card == 1 {
            let marker = self.marker(focused);
            let blank = " ".repeat(marker.chars().count());
            for (index, (_, title, detail, _)) in QUICK_ACTIONS.iter().enumerate() {
                let row = index as u16 / columns;
                let column = index as u16 % columns;
                if row >= list.height {
                    break;
                }
                let rect = Rect::new(
                    list.x + column * (column_width + 1),
                    list.y + row,
                    column_width,
                    1,
                );
                self.hits.get_mut().row(rect, index);
                let chosen = index == selected;
                let line = Line::from(vec![
                    Span::raw(if chosen { marker } else { blank.as_str() }.to_string()),
                    Span::styled(*title, Style::default().fg(self.theme.entry()).bold()),
                    Span::styled(
                        format!("   {detail}"),
                        Style::default().fg(self.theme.muted()),
                    ),
                ]);
                let style = if chosen {
                    self.highlight(focused)
                } else {
                    Style::default()
                };
                frame.render_widget(Paragraph::new(line).style(style), rect);
            }
            return;
        }
        for (index, (icon, title, detail, _)) in QUICK_ACTIONS.iter().enumerate() {
            let row = index as u16 / columns;
            let column = index as u16 % columns;
            let rect = Rect::new(
                list.x + column * (column_width + 1),
                list.y + row * card,
                column_width,
                card,
            );
            if rect.bottom() > list.bottom() {
                break;
            }
            self.hits.get_mut().row(rect, index);
            self.action_card(
                frame,
                rect,
                *icon,
                title,
                detail,
                index == selected,
                focused,
            );
        }
    }

    /// The dashboard's counts, as the window's tiles: installed, updates
    /// waiting, queued and pinned. Four tiles in three rows, else one line.
    pub(super) fn draw_counts(&self, frame: &mut Frame, area: Rect) {
        if area.height == 0 {
            return;
        }
        let counts = [
            (self.installed_count(), "installed", self.theme.success()),
            (self.update_count(), "updates", self.theme.warning()),
            (self.queue.len(), "queued", self.theme.focused()),
            (self.favorites.len(), "pinned", self.theme.title()),
        ];
        if area.height < 4 {
            let mut spans = Vec::new();
            for (index, (count, label, color)) in counts.iter().enumerate() {
                if index > 0 {
                    spans.push(Span::styled(" · ", Style::default().fg(self.theme.faint())));
                }
                spans.push(Span::styled(
                    count.to_string(),
                    Style::default().fg(*color).bold(),
                ));
                spans.push(Span::styled(
                    format!(" {label}"),
                    Style::default().fg(self.theme.muted()),
                ));
            }
            frame.render_widget(
                Paragraph::new(Line::from(spans)),
                Rect::new(area.x, area.y, area.width, 1),
            );
            return;
        }
        let tile = (area.width + 1) / 4 - 1;
        for (index, (count, label, color)) in counts.iter().enumerate() {
            let rect = Rect::new(area.x + index as u16 * (tile + 1), area.y, tile, 3);
            let inside = self.framed(frame, rect, self.theme.line(), self.theme.card());
            frame.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::styled(count.to_string(), Style::default().fg(*color).bold()),
                    Span::styled(format!(" {label}"), Style::default().fg(self.theme.muted())),
                ]))
                .alignment(Alignment::Center),
                inside,
            );
        }
    }

    /// A quick action as a card: icon, title, detail and a chevron.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn action_card(
        &self,
        frame: &mut Frame,
        area: Rect,
        icon: Icon,
        title: &str,
        detail: &str,
        selected: bool,
        focused: bool,
    ) {
        let border = match (selected, focused) {
            (true, true) => self.theme.focused(),
            (true, false) => self.theme.selected_line(),
            _ => self.theme.line(),
        };
        let fill = if selected {
            self.theme.selected_fill()
        } else {
            self.theme.card()
        };
        let inside = self.framed(frame, area, border, fill);
        let inner = Rect::new(
            inside.x + 1,
            inside.y,
            inside.width.saturating_sub(2),
            inside.height,
        );
        let icon = self.icon_gap(icon);
        let indent = " ".repeat(icon.chars().count());
        let title_style = Style::default()
            .fg(if selected {
                self.theme.on_selected()
            } else {
                self.theme.entry()
            })
            .bold();
        let icon = Span::styled(icon, Style::default().fg(self.theme.focused()));
        // The chevron takes the last two columns.
        let text = Rect::new(
            inner.x,
            inner.y,
            inner.width.saturating_sub(2),
            inner.height,
        );
        let room = usize::from(text.width);
        let lines = if inner.height >= 2 {
            vec![
                Line::from(vec![icon, Span::styled(title.to_string(), title_style)]),
                Line::from(vec![
                    Span::raw(indent.clone()),
                    Span::styled(
                        clip(detail, room.saturating_sub(indent.chars().count())),
                        Style::default().fg(self.theme.muted()),
                    ),
                ]),
            ]
        } else {
            vec![Line::from(vec![
                icon,
                Span::styled(title.to_string(), title_style),
                Span::styled(
                    format!(
                        "   {}",
                        clip(
                            detail,
                            room.saturating_sub(indent.chars().count() + title.chars().count() + 3)
                        )
                    ),
                    Style::default().fg(self.theme.muted()),
                ),
            ])]
        };
        frame.render_widget(Paragraph::new(Text::from(lines)), text);
        frame.render_widget(
            Paragraph::new(Line::styled(
                self.glyph(Icon::Chevron),
                Style::default().fg(if selected {
                    self.theme.focused()
                } else {
                    self.theme.muted()
                }),
            ))
            .alignment(Alignment::Right),
            Rect::new(inner.x, inner.y, inner.width, 1),
        );
    }

    pub(super) fn draw_footer(&self, frame: &mut Frame, area: Rect) {
        self.rule(frame, area.x, area.y, area.width);
        // How the check of package entries stands, on the rule, as the
        // window's footer says it.
        if let Some(status) = self.check_line() {
            let width = status.chars().count() as u16 + 2;
            let x = area.right().saturating_sub(width + 2).max(area.x);
            frame.render_widget(
                Paragraph::new(Line::styled(
                    format!(" {status} "),
                    Style::default().fg(self.theme.faint()),
                )),
                Rect::new(x, area.y, width.min(area.width), 1).intersection(area),
            );
        }
        let (_, hints) = self.hints();
        // Help and quit sit on the right, as in the design.
        let (right, mut left): (Vec<&Hint>, Vec<&Hint>) = hints
            .iter()
            .partition(|hint| matches!(hint.keys(), ["?"] | ["q"]));
        // Hints that do not fit beside help and quit are left out, last first.
        let width = |hints: &[&Hint]| -> usize {
            hints
                .iter()
                .map(|hint| hint.keys().join("/").chars().count() + 1 + hint.text().chars().count())
                .sum::<usize>()
                + hints.len().saturating_sub(1) * 3
        };
        let room = area.width.saturating_sub(2) as usize;
        while !left.is_empty() && width(&left) + width(&right) + 2 > room {
            left.pop();
        }
        let spans = |hints: &[&Hint]| {
            let mut spans: Vec<Span> = Vec::new();
            for (index, hint) in hints.iter().enumerate() {
                if index > 0 {
                    spans.push(Span::styled(" · ", Style::default().fg(self.theme.faint())));
                }
                spans.push(Span::styled(
                    hint.keys().join("/"),
                    Style::default().fg(self.theme.entry()).bold(),
                ));
                spans.push(Span::styled(
                    format!(" {}", hint.text()),
                    Style::default().fg(self.theme.muted()),
                ));
            }
            spans
        };
        let row =
            Rect::new(area.x + 1, area.y + 1, area.width.saturating_sub(2), 1).intersection(area);
        let right_line = Line::from(spans(&right));
        let right_x = row.right().saturating_sub(right_line.width() as u16);
        self.hint_hits(&left, row.x, row);
        self.hint_hits(&right, right_x, row);
        frame.render_widget(Paragraph::new(Line::from(spans(&left))), row);
        frame.render_widget(Paragraph::new(right_line).alignment(Alignment::Right), row);
    }

    /// Where each of the hints drawn from `x` on is, for a click: each key
    /// of a hint where its name is, its description sending the first.
    pub(super) fn hint_hits(&self, hints: &[&Hint], mut x: u16, row: Rect) {
        let mut hits = self.hits.borrow_mut();
        let width = |text: &str| Span::raw(text).width() as u16;
        for (index, hint) in hints.iter().enumerate() {
            if index > 0 {
                x += width(" · ");
            }
            let start = x;
            for (number, name) in hint.keys().iter().enumerate() {
                if number > 0 {
                    x += width("/");
                }
                if let Some(key) = mouse::key_named(name) {
                    hits.key(Rect::new(x, row.y, width(name), 1).intersection(row), key);
                }
                x += width(name);
            }
            let text = width(&format!(" {}", hint.text()));
            if let Some(key) = hint.keys().first().and_then(|name| mouse::key_named(name)) {
                hits.key(
                    Rect::new(start, row.y, x - start + text, 1).intersection(row),
                    key,
                );
            }
            x += text;
        }
    }

    pub(super) fn hints(&self) -> (&'static str, Vec<Hint>) {
        match &self.popup {
            Some(Popup::Text(popup)) => return popup.hints(),
            Some(Popup::HardwareReport(popup, _)) => {
                let (title, mut list) = popup.hints();
                list.insert(0, Hint::new(&["o"], "Open the issue"));
                return (title, list);
            }
            Some(
                Popup::ProfileName(popup)
                | Popup::BaselineName(popup)
                | Popup::FirmwareWord(popup)
                | Popup::LookName(popup)
                | Popup::Ask(popup, _),
            ) => return popup.hints(),
            Some(Popup::Wizard(popup)) => return popup.hints(),
            Some(Popup::Palette(popup)) => return popup.hints(),
            Some(Popup::Effects(popup)) => return popup.hints(),
            Some(Popup::Confirm(popup)) => return popup.hints(),
            Some(Popup::Running(popup)) => return popup.hints(),
            None => {}
        }
        let mut list = Vec::new();
        match self.focus {
            Focus::Search => {
                list.push(Hint::new(&["Enter"], "Keep results"));
                list.push(Hint::new(&["Esc"], "Clear"));
                return ("Search", list);
            }
            Focus::Sidebar => {
                list.push(Hint::new(&["↑↓"], "Navigate"));
                list.push(Hint::new(&["Enter"], "Open"));
            }
            Focus::Main => match self.screen {
                Screen::Dashboard => {
                    list.push(Hint::new(&["↑↓"], "Navigate"));
                    list.push(Hint::new(&["Enter"], "Select"));
                }
                Screen::Area(_) | Screen::Favorites => {
                    list.push(Hint::new(&["↑↓"], "Navigate"));
                    match self.selected_row() {
                        Some(Row::Up) => list.push(Hint::new(&["Enter", "h"], "Parent")),
                        Some(Row::Group { .. }) => list.push(Hint::new(&["Enter"], "Open")),
                        Some(Row::Entry(entry)) => {
                            list.push(Hint::new(&["Enter"], "Run"));
                            if removable(entry) {
                                list.push(Hint::new(&["r"], "Remove"));
                            }
                            if let Some(Status::Update { .. }) = self.statuses.get(&entry.id) {
                                list.push(Hint::new(&["u"], "Update"));
                            }
                            list.push(Hint::new(&["Space"], "Plan"));
                            if matches!(entry.action, Action::Profile { .. }) {
                                list.push(Hint::new(&["e"], "Its entries to plan"));
                            }
                            list.push(Hint::new(&["d"], "Describe"));
                            list.push(Hint::new(&["p"], "Preview"));
                            list.push(Hint::new(&["v"], "Trust"));
                            list.push(Hint::new(&["c"], "Check"));
                            if own_profile(entry) {
                                list.push(Hint::new(&["x"], "Export"));
                                list.push(Hint::new(&["Del"], "Delete"));
                            }
                            list.push(Hint::new(
                                &["*"],
                                if self.favorites.contains(&entry.id) {
                                    "Unpin"
                                } else {
                                    "Pin"
                                },
                            ));
                        }
                        None => {}
                    }
                    list.push(Hint::new(&["i"], self.filter_hint()));
                    if self.in_profiles() {
                        list.push(Hint::new(&["o"], "Import"));
                    }
                }
                Screen::Plan => {
                    list.push(Hint::new(&["Enter"], "Run plan"));
                    if !self.queue.is_empty() {
                        list.push(Hint::new(&["e"], "Changes"));
                        list.push(Hint::new(&["s"], "Save as profile"));
                    }
                    if self.queue.len() > 1 {
                        list.push(Hint::new(&["K", "J"], "Move"));
                    }
                    list.push(Hint::new(&["x"], "Remove"));
                    list.push(Hint::new(&["c"], "Clear"));
                }
                Screen::History => {
                    list.push(Hint::new(&["Enter"], "Show log"));
                    if self
                        .main_state
                        .selected()
                        .and_then(|index| self.records.get(index))
                        .is_some_and(|record| record.success)
                    {
                        list.push(Hint::new(&["w"], "Report it worked"));
                    }
                    list.push(Hint::new(&["r"], "Reload"));
                }
                Screen::Doctor => {
                    list.push(Hint::new(&["↑↓"], "Navigate"));
                    match self.selected_diagnosis() {
                        Some(diagnosis) => {
                            list.push(Hint::new(&["Enter"], "Details"));
                            if diagnosis.repair.is_some() {
                                list.push(Hint::new(&["f"], "Repair"));
                            } else if diagnosis.entry.is_some() {
                                list.push(Hint::new(&["f"], "Run the entry"));
                            }
                        }
                        None => {
                            list.push(Hint::new(&["Enter"], "Run"));
                            list.push(Hint::new(&["Space"], "Plan"));
                            list.push(Hint::new(&["d"], "Details"));
                        }
                    }
                    list.push(Hint::new(&["r"], "Check again"));
                }
                Screen::Updates => {
                    list.push(Hint::new(&["↑↓"], "Navigate"));
                    match self.selected_update_row() {
                        UpdateRow::Channel => {
                            list.push(Hint::new(&["Enter"], "Details"));
                            list.push(Hint::new(&["u"], "Update"));
                            list.push(Hint::new(&["U"], "Update all"));
                        }
                        UpdateRow::Entry(_) => {
                            list.push(Hint::new(&["Enter", "u"], "Update it"));
                            list.push(Hint::new(&["d"], "Details"));
                        }
                        UpdateRow::Every => list.push(Hint::new(&["Enter"], "Update every entry")),
                        UpdateRow::Nothing => {}
                    }
                    list.push(Hint::new(&["r"], "Check online"));
                }
                Screen::Drift => {
                    list.push(Hint::new(&["↑↓"], "Navigate"));
                    if let Some(difference) = self.selected_difference() {
                        list.push(Hint::new(&["Enter"], "Details"));
                        if difference.entry.is_some() && restorable(difference.kind) {
                            list.push(Hint::new(&["Space"], "Plan"));
                        }
                    }
                    if self
                        .drift_report
                        .as_ref()
                        .is_some_and(|(_, report)| !report.restore().is_empty())
                    {
                        list.push(Hint::new(&["a"], "Plan all missing"));
                        list.push(Hint::new(&["R"], "Run them now"));
                    }
                    if self.drift_targets.len() > 1 {
                        list.push(Hint::new(&["b"], "Compare with"));
                    }
                    list.push(Hint::new(&["s"], "Save baseline"));
                    list.push(Hint::new(&["r"], "Check again"));
                }
                Screen::Desktop => {
                    list.push(Hint::new(&["↑↓"], "Navigate"));
                    match self.selected_desktop_row() {
                        Some(DesktopRow::Entry { .. }) => {
                            list.push(Hint::new(&["Enter"], "Run"));
                            list.push(Hint::new(&["Space"], "Plan"));
                        }
                        Some(DesktopRow::Look { .. }) => list.push(Hint::new(&["Enter"], "Apply")),
                        Some(DesktopRow::Info { .. }) => {
                            list.push(Hint::new(&["Enter"], "Details"))
                        }
                        _ => {}
                    }
                    list.push(Hint::new(&["s"], "Save this look"));
                    list.push(Hint::new(&["r"], "Read again"));
                }
                Screen::Hardware => {
                    list.push(Hint::new(&["↑↓"], "Navigate"));
                    if let Some(HardwareRow::Item { act, .. }) = self.selected_hardware() {
                        list.push(Hint::new(&["Enter"], "Details"));
                        match act {
                            Some(HardwareAct::Firmware(_)) => {
                                list.push(Hint::new(&["f"], "Update firmware"))
                            }
                            Some(HardwareAct::Smart(_)) => {
                                list.push(Hint::new(&["s"], "SMART check"))
                            }
                            None => {}
                        }
                    }
                    list.push(Hint::new(&["r"], "Read again"));
                }
                Screen::Reliability => {
                    list.push(Hint::new(&["↑↓"], "Navigate"));
                    if let Some((_, _, assessment)) = self
                        .main_state
                        .selected()
                        .and_then(|index| self.reliability_rows.get(index))
                    {
                        list.push(Hint::new(&["Enter"], "Details"));
                        list.push(Hint::new(&["v"], "Trust"));
                        if assessment.here.is_some() {
                            list.push(Hint::new(&["w"], "Report it worked"));
                        }
                    }
                    list.push(Hint::new(
                        &["i"],
                        match self.coverage {
                            Coverage::All => "Show: all",
                            Coverage::Critical => "Show: critical",
                            Coverage::Untested => "Show: untested",
                        },
                    ));
                }
                Screen::Backups => {
                    list.push(Hint::new(&["↑↓"], "Navigate"));
                    if !self.backups.is_empty() {
                        list.push(Hint::new(&["Enter"], "Restore"));
                        list.push(Hint::new(&["e"], "Side by side"));
                        list.push(Hint::new(&["d"], "Details"));
                    }
                    list.push(Hint::new(&["r"], "Reload"));
                }
                Screen::Bundles => {
                    list.push(Hint::new(&["↑↓"], "Navigate"));
                    let verb = match self.selected_bundle_row() {
                        Some(BundleRow::Entries) => "Install Plan",
                        Some(BundleRow::Folder | BundleRow::Source) => "Change",
                        Some(BundleRow::Complete | BundleRow::Trust) => "Switch",
                        Some(BundleRow::Contents | BundleRow::Check) => "Check",
                        Some(BundleRow::Make) => "Make",
                        Some(BundleRow::Install) => "Install",
                        _ => "",
                    };
                    if !verb.is_empty() {
                        list.push(Hint::new(&["Enter"], verb));
                    }
                }
                Screen::Services => {
                    list.push(Hint::new(&["↑↓"], "Navigate"));
                    match self.selected_service_row() {
                        Some(ServiceRow::Unit(index)) => {
                            list.push(Hint::new(&["Enter"], "Details"));
                            if let Some(unit) = self.unit(index) {
                                list.push(Hint::new(
                                    &["s"],
                                    if unit.running() { "Stop" } else { "Start" },
                                ));
                                if unit.switchable() {
                                    list.push(Hint::new(
                                        &["e"],
                                        if unit.enabled() { "Disable" } else { "Enable" },
                                    ));
                                }
                                if unit.running() {
                                    list.push(Hint::new(&["R"], "Restart"));
                                }
                            }
                        }
                        Some(ServiceRow::Autostart(_)) => {
                            list.push(Hint::new(&["Enter"], "On or off at login"));
                            list.push(Hint::new(&["d"], "Details"));
                        }
                        _ => {}
                    }
                    list.push(Hint::new(&["i"], "Show"));
                    list.push(Hint::new(&["f"], "Filter"));
                    list.push(Hint::new(&["r"], "Read again"));
                }
                Screen::Storage => {
                    list.push(Hint::new(&["↑↓"], "Navigate"));
                    if let Some(StorageRow::Item(index)) = self.selected_storage_row()
                        && self
                            .storage_found
                            .as_ref()
                            .and_then(|found| found.items.get(index))
                            .is_some_and(|item| item.not_offered.is_none())
                    {
                        list.push(Hint::new(&["Enter"], "Clean"));
                    }
                    list.push(Hint::new(&["d"], "Details"));
                    list.push(Hint::new(&["r"], "Read again"));
                }
                Screen::Performance => {
                    list.push(Hint::new(&["↑↓"], "Navigate"));
                    match self.selected_performance_row() {
                        Some(PerformanceRow::Hint(_)) => {
                            list.push(Hint::new(&["Enter"], "Act on it"));
                            list.push(Hint::new(&["d"], "Details"));
                        }
                        Some(PerformanceRow::Unit(..)) => {
                            list.push(Hint::new(&["Enter"], "In Services"))
                        }
                        _ => {}
                    }
                    if self.earlier_boots().len() > 1 {
                        list.push(Hint::new(&["b"], "An earlier boot"));
                    }
                    list.push(Hint::new(&["r"], "Read again"));
                }
                Screen::Recovery => {
                    list.push(Hint::new(&["↑↓"], "Navigate"));
                    list.push(Hint::new(&["Enter"], "Details"));
                    if self
                        .selected_readiness()
                        .and_then(|check| check.entry.as_deref())
                        .and_then(|id| self.catalog.find(id))
                        .is_some()
                    {
                        list.push(Hint::new(&["f"], "Set it up"));
                    }
                    if self.snapshots_here().is_some() {
                        list.push(Hint::new(&["c"], "Check snapshots"));
                    }
                    list.push(Hint::new(&["r"], "Check again"));
                }
                Screen::Migration => {
                    list.push(Hint::new(&["↑↓"], "Navigate"));
                    if self.selected_migration_item().is_some() {
                        list.push(Hint::new(&["d"], "Details"));
                    }
                    list.push(Hint::new(&["e"], "Export"));
                    list.push(Hint::new(&["o"], "Open a file"));
                    if self
                        .migration_opened
                        .as_ref()
                        .is_some_and(|(_, translation)| translation.count(Standing::Install) > 0)
                    {
                        list.push(Hint::new(&["a"], "Install"));
                    }
                    list.push(Hint::new(&["r"], "Read again"));
                }
                Screen::Restic => {
                    list.push(Hint::new(&["↑↓"], "Navigate"));
                    if self.restic_config().is_some() {
                        match self
                            .restic_rows
                            .get(self.main_state.selected().unwrap_or(0))
                        {
                            Some(ResticRow::Snapshot(_)) => {
                                list.push(Hint::new(&["Enter"], "Browse"));
                                list.push(Hint::new(&["R"], "Restore"));
                            }
                            Some(ResticRow::Node(_) | ResticRow::Version(..)) => {
                                list.push(Hint::new(&["R"], "Restore"));
                            }
                            Some(ResticRow::Baseline(_)) => list.push(Hint::new(&["v"], "Test")),
                            _ => list.push(Hint::new(&["F"], "Restore…")),
                        }
                        list.push(Hint::new(&["b"], "Back up"));
                        list.push(Hint::new(&["s"], "History"));
                        if self.restic_search.is_some() {
                            list.push(Hint::new(
                                &["i"],
                                if self.restic_ignore_case {
                                    "Match case"
                                } else {
                                    "Ignore case"
                                },
                            ));
                        }
                        list.push(Hint::new(&["n"], "Baseline"));
                    }
                    list.push(Hint::new(&["r"], "Read again"));
                }
                Screen::Repositories => self.repo_hints(&mut list),
                Screen::Configs => self.config_hints(&mut list),
                Screen::HomeManager => self.home_hints(&mut list),
                Screen::Settings => list.push(Hint::new(&["Enter"], "Change")),
            },
        }
        list.push(Hint::new(&["/"], "Search"));
        list.push(Hint::new(&["^K"], "Commands"));
        list.push(Hint::new(&["?"], "Help"));
        list.push(Hint::new(&["q"], "Quit"));
        ("Helix Toolbox", list)
    }

    /// The color of a status word.
    pub(super) fn status_color(&self, status: &Status) -> Color {
        match status {
            Status::Installed { .. } | Status::Elsewhere { .. } => self.theme.success(),
            Status::Update { .. } => self.theme.warning(),
            Status::Managed { .. } => self.theme.title(),
            Status::Absent { .. } => self.theme.muted(),
        }
    }
}
