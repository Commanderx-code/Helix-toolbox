use super::*;
use ratatui::{Terminal, backend::TestBackend};

#[test]
fn clipped_text_ends_at_a_word() {
    assert_eq!(
        clip("Search and install entries", 40),
        "Search and install entries"
    );
    assert_eq!(clip("Search and install entries", 16), "Search and…");
    assert_eq!(clip("Supercalifragilistic", 8), "Superca…");
    assert_eq!(clip("anything", 0), "");
}

fn app() -> App {
    let args = TuiArgs {
        override_validation: true,
        bypass_root: true,
        ..TuiArgs::default()
    };
    let platform =
        LinuxInfo::from_os_release("ID=garuda\nID_LIKE=arch\nPRETTY_NAME=\"Garuda Linux\"\n")
            .unwrap();
    let mut app = App::new(Catalog::builtin(false).unwrap(), platform, &args);
    // Plain text keeps these screen tests readable; one test draws icons.
    app.icons = false;
    app
}

fn screen(app: &mut App, width: u16, height: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|frame| app.draw(frame)).unwrap();
    let buffer = terminal.backend().buffer();
    (0..height)
        .map(|y| {
            (0..width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
                + "\n"
        })
        .collect()
}

fn press(app: &mut App, code: KeyCode) -> bool {
    app.handle_key(&KeyEvent::new(code, KeyModifiers::NONE))
}

fn type_text(app: &mut App, text: &str) {
    for character in text.chars() {
        press(app, KeyCode::Char(character));
    }
}

#[test]
fn the_dashboard_is_the_first_screen() {
    let mut app = app();
    let text = screen(&mut app, 120, 40);
    assert!(text.contains("HELIX TOOLBOX"));
    assert!(text.contains("SYSTEM READY"));
    assert!(text.contains("MAIN MENU"));
    assert!(text.contains("MORE TOOLS"));
    assert!(text.contains("Dashboard"));
    assert!(text.contains("System Overview"));
    assert!(text.contains("Garuda Linux"));
    assert!(text.contains("Quick Actions"));
    assert!(text.contains("Browse Applications"));
    assert!(text.contains("Navigate"));
}

fn with_banner(mut app: App) -> App {
    #[allow(deprecated)]
    let mut picker = ratatui_image::picker::Picker::from_fontsize((9, 19).into());
    picker.set_protocol_type(ratatui_image::picker::ProtocolType::Kitty);
    app.icons = true;
    app.with_banner(banner::encode(&picker))
}

#[test]
fn the_banner_replaces_the_text_header_where_it_can_be_drawn() {
    let mut app = with_banner(app());
    let text = screen(&mut app, 120, 40);
    let lines: Vec<&str> = text.lines().collect();
    assert!(lines[0].contains('\u{10EEEE}'), "{}", lines[0]);
    assert!(lines[3].contains('\u{10EEEE}'));
    assert!(lines[1].contains("SYSTEM READY"));
    // The name and tagline beside the emblem, in the middle rows.
    assert!(
        lines[1].contains("H E L I X   T O O L B O X"),
        "{}",
        lines[1]
    );
    assert!(lines[2].contains("BUILD • CONFIGURE • OPTIMIZE"));
    assert!(!text.contains("HELIX TOOLBOX"));
    // Four rows of picture, then the rule and the sidebar below it.
    assert!(lines[4].contains('─'));
    assert!(lines[6].contains("MAIN MENU"));
}

#[test]
fn the_text_header_returns_without_icons_in_sixteen_colors_or_when_narrow() {
    let mut app = with_banner(app());
    app.icons = false;
    assert!(screen(&mut app, 120, 40).contains("HELIX TOOLBOX"));
    app.icons = true;
    app.theme = Theme::Compatible;
    assert!(screen(&mut app, 120, 40).contains("HELIX TOOLBOX"));
    app.theme = Theme::Solid;
    app.size_bypass = true;
    let narrow = screen(&mut app, 40, 30);
    assert!(!narrow.contains('\u{10EEEE}'));
    assert!(narrow.contains("HELIX TOOLBOX"));
}

#[test]
fn a_small_terminal_gets_a_warning_unless_bypassed() {
    let mut app = app();
    assert!(screen(&mut app, 60, 20).contains("needs 100 by 25"));
    app.size_bypass = true;
    assert!(screen(&mut app, 60, 20).contains("HELIX TOOLBOX"));
}

#[test]
fn the_sidebar_opens_areas_and_groups_remember_their_row() {
    let mut app = app();
    press(&mut app, KeyCode::Char('j'));
    assert_eq!(app.screen, Screen::Area(0));
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.focus, Focus::Main);
    let text = screen(&mut app, 120, 40);
    assert!(text.contains("[D]  Developer Tools"));
    assert!(text.contains("FLAGS"));
    press(&mut app, KeyCode::Char('j'));
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.areas[0].path.len(), 1);
    assert!(screen(&mut app, 120, 40).contains("[D]  .."));
    press(&mut app, KeyCode::Char('h'));
    assert!(app.at_root());
    assert_eq!(app.main_state.selected(), Some(1));
    press(&mut app, KeyCode::Char('h'));
    assert_eq!(app.focus, Focus::Sidebar);
}

/// Where `text` is drawn, as the column and row of its first cell.
fn find(app: &mut App, text: &str) -> (u16, u16) {
    let drawn = screen(app, 120, 40);
    for (row, line) in drawn.lines().enumerate() {
        if let Some(at) = line.find(text) {
            return (line[..at].chars().count() as u16, row as u16);
        }
    }
    panic!("{text:?} is not on the screen:\n{drawn}");
}

fn mouse_at(app: &mut App, kind: MouseEventKind, (column, row): (u16, u16)) -> bool {
    app.handle_mouse(&MouseEvent {
        kind,
        column,
        row,
        modifiers: KeyModifiers::NONE,
    })
}

fn click(app: &mut App, text: &str) -> bool {
    let at = find(app, text);
    mouse_at(app, MouseEventKind::Down(MouseButton::Left), at)
}

/// A click on the lowest place `text` is drawn, such as a result below
/// the search that found it.
fn click_last(app: &mut App, text: &str) -> bool {
    let drawn = screen(app, 120, 40);
    let (column, row) = drawn
        .lines()
        .enumerate()
        .filter_map(|(row, line)| {
            line.find(text)
                .map(|at| (line[..at].chars().count() as u16, row as u16))
        })
        .last()
        .unwrap_or_else(|| panic!("{text:?} is not on the screen:\n{drawn}"));
    mouse_at(app, MouseEventKind::Down(MouseButton::Left), (column, row))
}

#[test]
fn clicks_open_screens_and_select_rows_and_a_second_click_is_enter() {
    let mut app = app();
    let area = app.menu[1].label.clone();
    click(&mut app, &area);
    assert_eq!(app.screen, Screen::Area(0));
    assert_eq!(app.focus, Focus::Sidebar);
    // A row of the list: selected, by the keys, and the list focused.
    click(&mut app, "Developer Tools");
    assert_eq!(app.focus, Focus::Main);
    let row = app.main_state.selected().unwrap();
    assert!(
        matches!(app.selected_row(), Some(Row::Group { .. })),
        "row {row}"
    );
    assert!(app.at_root());
    click(&mut app, "Developer Tools");
    assert_eq!(
        app.areas[0].path.len(),
        1,
        "the second click opens the group"
    );
    // On an entry, the second click asks first, as Enter does; a click
    // beside the confirmation answers nothing.
    press(&mut app, KeyCode::Char('/'));
    type_text(&mut app, "System Report (script)");
    let name = "System Report (script)";
    click_last(&mut app, name);
    assert_eq!(app.focus, Focus::Main);
    assert!(app.popup.is_none());
    click_last(&mut app, name);
    assert!(matches!(app.popup, Some(Popup::Confirm(_))));
    click(&mut app, &area);
    mouse_at(&mut app, MouseEventKind::Down(MouseButton::Left), (60, 20));
    assert!(matches!(app.popup, Some(Popup::Confirm(_))));
    assert_eq!(app.screen, Screen::Area(0));
    // Its hints are its keys: n cancels.
    click(&mut app, "n/q/Esc");
    assert!(app.popup.is_none());
}

#[test]
fn the_wheel_scrolls_the_list_and_popups_and_hints_are_keys() {
    let mut app = app();
    press(&mut app, KeyCode::Char('j'));
    let list = find(&mut app, "Developer Tools");
    mouse_at(&mut app, MouseEventKind::ScrollDown, list);
    assert_eq!(app.focus, Focus::Main);
    let before = app.main_state.selected().unwrap();
    mouse_at(&mut app, MouseEventKind::ScrollDown, list);
    assert_eq!(app.main_state.selected(), Some(before + 1));
    mouse_at(&mut app, MouseEventKind::ScrollUp, list);
    assert_eq!(app.main_state.selected(), Some(before));
    // The wheel over the sidebar changes no screen.
    let sidebar = find(&mut app, "MAIN MENU");
    mouse_at(&mut app, MouseEventKind::ScrollDown, sidebar);
    assert_eq!(app.screen, Screen::Area(0));
    // The ? hint is the ? key; the wheel scrolls the help it opens.
    click(&mut app, "? Help");
    let Some(Popup::Text(_)) = &app.popup else {
        panic!("the help did not open")
    };
    mouse_at(&mut app, MouseEventKind::ScrollDown, (60, 20));
    let Some(Popup::Text(help)) = &app.popup else {
        panic!("the wheel closed the help")
    };
    assert_eq!(help.scroll(), 1);
    click(&mut app, "Enter/q/Esc");
    assert!(app.popup.is_none());
    // And q quits.
    assert!(!click(&mut app, "q Quit"));
}

#[test]
fn the_mouse_can_be_turned_off_in_settings() {
    let mut app = app();
    assert!(app.mouse());
    app.open_screen(Screen::Settings);
    press(&mut app, KeyCode::Enter);
    click(&mut app, "Mouse");
    click(&mut app, "Mouse");
    assert!(!app.mouse(), "{}", screen(&mut app, 120, 40));
    assert!(screen(&mut app, 120, 40).contains("off"));
    // Off, clicks do nothing at all.
    let area = app.menu[1].label.clone();
    click(&mut app, &area);
    assert_eq!(app.screen, Screen::Settings);
}

#[test]
fn the_install_plan_saves_as_a_profile_that_appears_under_profiles() {
    let mut app = app();
    let directory = tempfile::tempdir().unwrap();
    app.profiles_dir = Some(directory.path().to_path_buf());
    app.queue = vec![
        app.catalog.find("git").unwrap().clone(),
        app.catalog.find("meld").unwrap().clone(),
    ];
    app.open_screen(Screen::Plan);
    app.focus = Focus::Main;
    assert!(screen(&mut app, 140, 40).contains("s Save as profile"));
    press(&mut app, KeyCode::Char('s'));
    assert!(screen(&mut app, 140, 40).contains("Save as a profile"));
    // A blank name is not accepted; Escape would cancel.
    press(&mut app, KeyCode::Enter);
    assert!(matches!(app.popup, Some(Popup::ProfileName(_))));
    type_text(&mut app, "Dev Box");
    press(&mut app, KeyCode::Enter);
    let shown = screen(&mut app, 140, 40);
    assert!(shown.contains("Profile saved"), "{shown}");
    let saved = directory.path().join("dev-box.toml");
    assert!(saved.exists());
    let entry = app.catalog.find("profile-dev-box").unwrap();
    assert!(matches!(&entry.action, Action::Profile { entries } if entries == &["git", "meld"]));
}

/// Replace what an `Ask` popup holds with `text`.
fn answer(app: &mut App, text: &str) {
    for _ in 0..200 {
        press(app, KeyCode::Backspace);
    }
    type_text(app, text);
    press(app, KeyCode::Enter);
}

#[test]
fn your_profiles_are_imported_exported_and_deleted_in_the_profiles_area() {
    let mut app = app();
    let directory = tempfile::tempdir().unwrap();
    let elsewhere = tempfile::tempdir().unwrap();
    app.profiles_dir = Some(directory.path().to_path_buf());
    app.queue = vec![app.catalog.find("git").unwrap().clone()];
    app.save_profile("Dev Box");
    app.popup = None;
    let area = area_of(&app, "profile-dev-box");
    app.open_screen(Screen::Area(area));
    app.focus = Focus::Main;
    assert!(screen(&mut app, 140, 40).contains("o Import"));
    // Into Your profiles, then onto the profile.
    press(&mut app, KeyCode::Enter);
    app.main_state.select(Some(1));
    assert_eq!(app.selected_entry().unwrap().id, "profile-dev-box");
    let shown = screen(&mut app, 140, 40);
    assert!(shown.contains("x Export"), "{shown}");

    press(&mut app, KeyCode::Char('x'));
    assert!(screen(&mut app, 140, 40).contains("~/dev-box.toml"));
    let exported = elsewhere.path().join("shared.toml");
    answer(&mut app, &exported.display().to_string());
    assert!(screen(&mut app, 140, 40).contains("Profile exported"));
    assert!(std::fs::read_to_string(&exported).unwrap().contains("git"));
    press(&mut app, KeyCode::Esc);
    press(&mut app, KeyCode::Char('x'));
    answer(&mut app, "shared.toml");
    assert!(screen(&mut app, 140, 40).contains("Give the full path"));
    press(&mut app, KeyCode::Esc);

    // Deleting needs the word.
    press(&mut app, KeyCode::Delete);
    answer(&mut app, "no");
    assert!(screen(&mut app, 140, 40).contains("Not deleted"));
    assert!(directory.path().join("dev-box.toml").exists());
    press(&mut app, KeyCode::Esc);
    app.queue = vec![app.catalog.find("profile-dev-box").unwrap().clone()];
    press(&mut app, KeyCode::Delete);
    answer(&mut app, "yes");
    assert!(screen(&mut app, 140, 40).contains("Profile deleted"));
    assert!(!directory.path().join("dev-box.toml").exists());
    assert!(app.catalog.find("profile-dev-box").is_none());
    assert!(app.queue.is_empty(), "it leaves the Install Plan");
    assert!(app.areas[area].path.is_empty(), "its group is gone");
    press(&mut app, KeyCode::Esc);

    // And it comes back from the exported file.
    press(&mut app, KeyCode::Char('o'));
    answer(&mut app, &exported.display().to_string());
    assert!(screen(&mut app, 140, 40).contains("Profile imported"));
    assert!(app.catalog.find("profile-dev-box").is_some());
    assert!(directory.path().join("dev-box.toml").exists());
}

#[test]
fn offline_bundles_are_made_from_the_install_plan_and_checked_before_installing() {
    let mut app = app();
    let folder = tempfile::tempdir().unwrap();
    app.open_screen(Screen::Bundles);
    app.focus = Focus::Main;
    let shown = screen(&mut app, 140, 40);
    for text in [
        "Offline bundles",
        "Make a bundle",
        "Install from a bundle",
        "none: the Install Plan is empty",
        "~/helix-toolbox-bundle",
    ] {
        assert!(shown.contains(text), "{text}: {shown}");
    }
    let row = |wanted: BundleRow| BUNDLE_ROWS.iter().position(|&row| row == wanted);
    app.main_state.select(row(BundleRow::Make));
    press(&mut app, KeyCode::Enter);
    assert!(screen(&mut app, 140, 40).contains("The Install Plan is empty"));
    press(&mut app, KeyCode::Esc);

    app.queue = vec![app.catalog.find("git").unwrap().clone()];
    app.main_state.select(row(BundleRow::Folder));
    press(&mut app, KeyCode::Enter);
    let target = folder.path().join("bundle");
    answer(&mut app, &target.display().to_string());
    assert_eq!(app.bundle_folder, target.display().to_string());
    app.main_state.select(row(BundleRow::Complete));
    press(&mut app, KeyCode::Enter);
    assert!(app.bundle_complete);
    app.main_state.select(row(BundleRow::Make));
    press(&mut app, KeyCode::Enter);
    assert!(matches!(app.popup, Some(Popup::Confirm(_))));
    let planned = app.pending.as_ref().unwrap();
    let command = planned.steps[0].display();
    assert!(
        command.contains("bundle create") && command.contains("--complete"),
        "{command}"
    );
    press(&mut app, KeyCode::Char('n'));
    // A folder with something in it is not used.
    std::fs::create_dir_all(&target).unwrap();
    std::fs::write(target.join("keep.txt"), "mine").unwrap();
    press(&mut app, KeyCode::Enter);
    assert!(screen(&mut app, 140, 40).contains("Cannot make the bundle"));
    press(&mut app, KeyCode::Esc);

    // Installing checks the bundle first.
    app.main_state.select(row(BundleRow::Install));
    press(&mut app, KeyCode::Enter);
    assert!(screen(&mut app, 140, 40).contains("No bundle chosen"));
    press(&mut app, KeyCode::Esc);
    app.main_state.select(row(BundleRow::Source));
    press(&mut app, KeyCode::Enter);
    answer(&mut app, &target.display().to_string());
    app.main_state.select(row(BundleRow::Check));
    press(&mut app, KeyCode::Enter);
    assert!(screen(&mut app, 140, 40).contains("Checking every file"));
    assert!(app.needs_redraw());
    assert!(screen(&mut app, 140, 40).contains("The bundle does not check out"));
    press(&mut app, KeyCode::Esc);
    app.main_state.select(row(BundleRow::Install));
    press(&mut app, KeyCode::Enter);
    assert!(app.needs_redraw());
    assert!(screen(&mut app, 140, 40).contains("Cannot install from it"));
    assert!(app.pending.is_none());
}

#[test]
fn settings_make_a_bug_report_and_keep_a_copy() {
    let mut app = app();
    let directory = tempfile::tempdir().unwrap();
    app.reports_dir = Some(directory.path().to_path_buf());
    app.open_screen(Screen::Settings);
    app.focus = Focus::Main;
    app.main_state.select(Some(11));
    assert!(screen(&mut app, 120, 40).contains("Bug report, to paste into an issue"));
    press(&mut app, KeyCode::Enter);
    assert!(screen(&mut app, 120, 40).contains("Gathering the report"));
    let start = std::time::Instant::now();
    while app.reporting.is_some() && start.elapsed() < std::time::Duration::from_secs(20) {
        app.needs_redraw();
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let shown = screen(&mut app, 120, 40);
    assert!(shown.contains("Paste it into a new issue"), "{shown}");
    let saved = std::fs::read_to_string(directory.path().join("bug-report.md")).unwrap();
    assert!(saved.contains("### Helix Toolbox report"), "{saved}");
    // c copies the report itself, not the words around it.
    assert!(shown.contains("c Copy"), "{shown}");
    app.copier = |text| {
        if text.starts_with("### Helix Toolbox report") {
            Ok("Copied with wl-copy.".into())
        } else {
            Err(format!("not the report: {text}"))
        }
    };
    press(&mut app, KeyCode::Char('c'));
    assert!(screen(&mut app, 120, 40).contains("Copied with wl-copy."));
    assert!(matches!(app.popup, Some(Popup::Text(_))));
    let (choices, _) = app.palette_choices();
    assert!(
        choices
            .iter()
            .any(|choice| choice.label == "Make a bug report")
    );
}

#[test]
fn quick_actions_open_screens() {
    let mut app = app();
    press(&mut app, KeyCode::Enter);
    for _ in 0..4 {
        press(&mut app, KeyCode::Char('j'));
    }
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.screen, Screen::Doctor);
    assert!(screen(&mut app, 120, 40).contains("Machine Summary"));
}

#[test]
fn quick_actions_set_up_the_machine_and_find_anything() {
    let mut app = app();
    press(&mut app, KeyCode::Enter);
    let text = screen(&mut app, 120, 40);
    assert!(text.contains("Set up this machine"), "{text}");
    assert!(text.contains("Find anything"), "{text}");
    press(&mut app, KeyCode::Char('j'));
    press(&mut app, KeyCode::Char('j'));
    press(&mut app, KeyCode::Enter);
    assert!(matches!(app.popup, Some(Popup::Wizard(_))));
    app.popup = None;
    press(&mut app, KeyCode::Char('j'));
    press(&mut app, KeyCode::Enter);
    assert!(matches!(app.popup, Some(Popup::Palette(_))));
}

#[test]
fn the_dashboard_counts_what_is_installed_queued_and_pinned() {
    let mut app = app();
    app.statuses.insert(
        "ripgrep".to_string(),
        Status::Installed {
            source: "pacman".to_string(),
            version: Some("15.2.0-1".to_string()),
            shadowed: None,
        },
    );
    app.favorites.push("marker".to_string());
    let text = screen(&mut app, 120, 40);
    assert!(text.contains("1 installed"), "{text}");
    assert!(text.contains("0 updates"), "{text}");
    assert!(text.contains("0 queued"), "{text}");
    assert!(text.contains("1 pinned"), "{text}");
    // Taller, the counts are tiles.
    let tall = screen(&mut app, 120, 60);
    assert!(tall.contains("╭"), "{tall}");
    assert!(tall.contains("1 installed"), "{tall}");
}

#[test]
fn the_doctor_shows_evidence_and_repairs_with_their_own_confirmation() {
    let mut app = app();
    app.diagnoses = vec![
        doctor::Diagnosis {
            id: "network".into(),
            title: "Network".into(),
            health: doctor::Health::Good,
            summary: "Connected through wlan0".into(),
            evidence: vec!["default routes: wlan0".into()],
            advice: None,
            repair: None,
            entry: None,
        },
        doctor::Diagnosis {
            id: "permissions".into(),
            title: "Home folder permissions".into(),
            health: doctor::Health::Problem,
            summary: "1 item(s) in your home folder belong to root".into(),
            evidence: vec!["owned by root: /home/me/.local/share/Shelly".into()],
            advice: Some("This usually comes from running a program with sudo.".into()),
            repair: Some(doctor::Repair {
                label: "give them back to you".into(),
                commands: vec![vec![
                    "sudo".into(),
                    "chown".into(),
                    "-R".into(),
                    "1000:1000".into(),
                    "--".into(),
                    "/home/me/.local/share/Shelly".into(),
                ]],
            }),
            entry: None,
        },
    ];
    app.open_screen(Screen::Doctor);
    app.focus = Focus::Main;
    let shown = screen(&mut app, 160, 40);
    assert!(shown.contains("OK      Network"), "{shown}");
    assert!(shown.contains("Problem Home folder permissions"));
    assert!(shown.contains("f repairs"));
    assert!(
        shown.contains("Report  Machine Summary"),
        "the reports stay below"
    );
    press(&mut app, KeyCode::Char('j'));
    press(&mut app, KeyCode::Enter);
    let shown = screen(&mut app, 160, 40);
    assert!(
        shown.contains("owned by root: /home/me/.local/share/Shelly"),
        "{shown}"
    );
    assert!(shown.contains("What to do: This usually comes from"));
    press(&mut app, KeyCode::Esc);
    app.popup = None;
    press(&mut app, KeyCode::Char('f'));
    assert!(
        matches!(app.popup, Some(Popup::Confirm(_))),
        "its own confirmation"
    );
    assert!(
        screen(&mut app, 160, 40)
            .contains("sudo chown -R 1000:1000 -- /home/me/.local/share/Shelly")
    );
    app.popup = None;
    app.pending = None;
    // A report below the checks runs like any entry.
    press(&mut app, KeyCode::Char('j'));
    press(&mut app, KeyCode::Enter);
    assert!(matches!(app.popup, Some(Popup::Confirm(_))));
}

#[test]
fn entries_show_how_they_stand() {
    let mut app = app();
    app.statuses.insert(
        "ripgrep".into(),
        Status::Update {
            source: "pacman".into(),
            version: Some("15.2.0-1".into()),
            latest: Some("15.3.0-1".into()),
            shadowed: None,
        },
    );
    app.statuses.insert(
        "fzf".into(),
        Status::Managed {
            by: "Home Manager".into(),
            version: Some("0.74.4".into()),
        },
    );
    press(&mut app, KeyCode::Char('/'));
    type_text(&mut app, "ripgrep");
    let wide = screen(&mut app, 140, 40);
    let row = wide.lines().find(|line| line.contains("Ripgrep")).unwrap();
    assert!(
        row.contains("Update  pacman 15.2.0-1 → 15.3.0-1  I"),
        "{row}"
    );
    assert!(wide.contains("STATUS · FLAGS"));
    // Short of room, the detail goes first and the word stays.
    app.size_bypass = true;
    let narrow = screen(&mut app, 70, 30);
    let row = narrow
        .lines()
        .find(|line| line.contains("Ripgrep"))
        .unwrap();
    assert!(row.contains("Update") && !row.contains("15.3.0-1"), "{row}");
    press(&mut app, KeyCode::Esc);
    press(&mut app, KeyCode::Char('/'));
    type_text(&mut app, "fzf");
    press(&mut app, KeyCode::Enter);
    press(&mut app, KeyCode::Char('d'));
    assert!(
        screen(&mut app, 140, 40).contains("Installed from: Home Manager 0.74.4"),
        "the description says it too"
    );
}

#[test]
fn backups_are_listed_described_and_restored_and_snapshots_go_first() {
    let mut app = app();
    app.backups = vec![backups::Backup {
        backup: "/home/me/.config/ghostty/config.commander-backup-20261001000000-1".into(),
        original: "/home/me/.config/ghostty/config".into(),
        taken: Some("2026-10-01 00:00:00".into()),
        system: false,
        directory: false,
    }];
    app.snapshot_tool = Some(backups::Snapshots::Snapper);
    app.screen = Screen::Backups;
    app.focus = Focus::Main;
    app.main_state.select(Some(0));
    let shown = screen(&mut app, 140, 40);
    assert!(shown.contains("2026-10-01 00:00:00"), "{shown}");
    assert!(shown.contains("ghostty/config"));
    assert!(shown.contains("Snapper is set up"));
    assert!(shown.contains("does not roll packages"), "{shown}");
    press(&mut app, KeyCode::Char('d'));
    assert!(screen(&mut app, 140, 40).contains("moved aside as its own backup"));
    press(&mut app, KeyCode::Esc);
    app.popup = None;
    press(&mut app, KeyCode::Enter);
    assert!(matches!(app.popup, Some(Popup::Confirm(_))));
    let shown = screen(&mut app, 140, 40);
    assert!(
        shown.contains("cp -a /home/me/.config/ghostty/config.commander-backup"),
        "{shown}"
    );
    assert!(
        !shown.contains("Take a Snapper snapshot"),
        "a user file needs no snapshot"
    );
    app.popup = None;
    app.pending = None;
    // A GRUB theme changes system files: the snapshot comes first.
    let grub = app.catalog.find("grub").unwrap().clone();
    app.confirm(vec![grub]);
    let shown = screen(&mut app, 160, 40);
    assert!(shown.contains("1. Take a Snapper snapshot"), "{shown}");
    app.popup = None;
    app.pending = None;
    app.snapshots = false;
    app.confirm(vec![app.catalog.find("grub").unwrap().clone()]);
    assert!(
        !screen(&mut app, 160, 40).contains("Snapper snapshot"),
        "off in Settings"
    );
}

#[test]
fn a_blocked_plan_says_why_instead_of_asking() {
    let fixtures =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/packages");
    let args = TuiArgs {
        override_validation: true,
        bypass_root: true,
        ..TuiArgs::default()
    };
    let platform = LinuxInfo::from_os_release("ID=arch\nPRETTY_NAME=\"Arch Linux\"\n").unwrap();
    let mut app = App::new(
        Catalog::from_directory(&fixtures, false).unwrap(),
        platform,
        &args,
    );
    app.icons = false;
    for name in ["Conflict A", "Conflict B"] {
        // A fresh search for each.
        press(&mut app, KeyCode::Char('/'));
        press(&mut app, KeyCode::Esc);
        press(&mut app, KeyCode::Char('/'));
        type_text(&mut app, name);
        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::Char(' '));
    }
    assert_eq!(app.queue.len(), 2);
    app.open_screen(Screen::Plan);
    app.focus = Focus::Main;
    let plan = screen(&mut app, 140, 40);
    assert!(
        plan.contains("Blocked: Conflict A and Conflict B replace each other"),
        "{plan}"
    );
    press(&mut app, KeyCode::Enter);
    assert!(matches!(app.popup, Some(Popup::Text(_))), "no confirmation");
    let shown = screen(&mut app, 140, 40);
    assert!(
        shown.contains("Cannot run yet") && shown.contains("Nothing was changed"),
        "{shown}"
    );
    assert!(app.pending.is_none());
}

#[test]
fn u_plans_an_update_and_waits_for_the_status_check() {
    let mut app = app();
    press(&mut app, KeyCode::Char('/'));
    type_text(&mut app, "ripgrep");
    press(&mut app, KeyCode::Enter);
    press(&mut app, KeyCode::Char('u'));
    assert!(screen(&mut app, 140, 40).contains("still checking what is installed"));
    press(&mut app, KeyCode::Esc);
    let inventory = Inventory::from_parts(
        Some(helix_toolbox_core::PackageManager::Arch),
        Some([("ripgrep".to_string(), "15.2.0-1".to_string())].into()),
        [("ripgrep".to_string(), "15.3.0-1".to_string())].into(),
        HashMap::new(),
        Default::default(),
    );
    let ripgrep = app.catalog.find("ripgrep").unwrap().clone();
    let status = inventory
        .status(&ripgrep, &app.platform, &|_| None)
        .unwrap();
    assert_eq!(status.label(), "Update");
    app.statuses.insert("ripgrep".into(), status);
    app.inventory = Some(inventory);
    assert!(
        screen(&mut app, 140, 40).contains("u Update"),
        "the hint offers it"
    );
    press(&mut app, KeyCode::Char('u'));
    assert!(matches!(app.popup, Some(Popup::Confirm(_))));
    let shown = screen(&mut app, 140, 40);
    assert!(shown.contains("sudo pacman -Syu"), "{shown}");
    assert!(shown.contains("partial upgrade"));
    press(&mut app, KeyCode::Esc);
    app.popup = None;
    press(&mut app, KeyCode::Char('U'));
    assert!(
        matches!(app.popup, Some(Popup::Confirm(_))),
        "U finds it too"
    );
}

#[test]
fn search_spans_every_area_and_escape_clears_it() {
    let mut app = app();
    press(&mut app, KeyCode::Char('/'));
    assert!(matches!(app.screen, Screen::Area(_)));
    type_text(&mut app, "git");
    assert!(!app.rows.is_empty());
    // Names holding the text come before looser matches.
    let names = entry_names(&app);
    let holding = names
        .iter()
        .take_while(|name| name.to_lowercase().contains("git"))
        .count();
    assert!(holding > 0);
    assert!(
        names[holding..]
            .iter()
            .all(|name| !name.to_lowercase().contains("git")),
        "{names:?}"
    );
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.focus, Focus::Main);
    press(&mut app, KeyCode::Char('/'));
    press(&mut app, KeyCode::Esc);
    assert!(app.search.is_empty());
    assert!(app.rows.iter().any(|row| matches!(row, Row::Group { .. })));
}

#[test]
fn entries_queue_into_the_install_plan_and_run_from_it() {
    let mut app = app();
    press(&mut app, KeyCode::Char('/'));
    type_text(&mut app, "System Report (script)");
    press(&mut app, KeyCode::Enter);
    press(&mut app, KeyCode::Char(' '));
    assert!(
        matches!(app.popup, Some(Popup::Text(_))),
        "a solo entry explains it cannot be queued"
    );
    press(&mut app, KeyCode::Esc);
    press(&mut app, KeyCode::Char('h'));
    type_text(&mut app, "/Machine Summary");
    press(&mut app, KeyCode::Enter);
    press(&mut app, KeyCode::Char(' '));
    assert_eq!(app.queue.len(), 1);
    assert!(screen(&mut app, 120, 40).contains("Machine Summary *"));
    app.open_screen(Screen::Plan);
    app.focus = Focus::Main;
    let text = screen(&mut app, 120, 40);
    assert!(text.contains("Install Plan"));
    assert!(text.contains("1 step, risk low"));
    assert!(text.contains("helix-toolbox native system-report"));
    press(&mut app, KeyCode::Enter);
    assert!(matches!(app.popup, Some(Popup::Confirm(_))));
    assert!(screen(&mut app, 120, 40).contains("Risk: low"));
    press(&mut app, KeyCode::Char('n'));
    press(&mut app, KeyCode::Char('x'));
    assert!(app.queue.is_empty());
    assert!(screen(&mut app, 120, 40).contains("Nothing is queued"));
}

#[test]
fn running_an_entry_asks_first_and_quitting_works() {
    let mut app = app();
    press(&mut app, KeyCode::Char('/'));
    type_text(&mut app, "System Report (script)");
    press(&mut app, KeyCode::Enter);
    press(&mut app, KeyCode::Char('d'));
    assert!(screen(&mut app, 120, 40).contains("Print the distribution"));
    press(&mut app, KeyCode::Esc);
    press(&mut app, KeyCode::Char('p'));
    assert!(screen(&mut app, 120, 40).contains("HELIXFORGE_LIB"));
    press(&mut app, KeyCode::Esc);
    press(&mut app, KeyCode::Enter);
    assert!(matches!(app.popup, Some(Popup::Confirm(_))));
    let text = screen(&mut app, 120, 40);
    assert!(text.contains("1. System Report (script)"), "{text}");
    assert!(text.contains("system-report.sh"), "{text}");
    press(&mut app, KeyCode::Char('n'));
    assert!(app.popup.is_none());
    assert!(!press(&mut app, KeyCode::Char('q')));
}

#[test]
fn removable_entries_offer_removal_and_others_do_not() {
    let mut app = app();
    press(&mut app, KeyCode::Char('/'));
    type_text(&mut app, "Tenacity");
    press(&mut app, KeyCode::Enter);
    assert!(screen(&mut app, 120, 40).contains("Remove"));
    press(&mut app, KeyCode::Char('r'));
    let text = screen(&mut app, 120, 40);
    assert!(
        text.contains("not installed") || text.contains("Remove Tenacity"),
        "{text}"
    );
    press(&mut app, KeyCode::Esc);
    app.popup = None;
    press(&mut app, KeyCode::Char('h'));
    press(&mut app, KeyCode::Char('/'));
    type_text(&mut app, "Machine Summary");
    press(&mut app, KeyCode::Enter);
    press(&mut app, KeyCode::Char('r'));
    assert!(
        app.popup.is_none(),
        "a non-package entry has nothing to remove"
    );
}

fn entry_names(app: &App) -> Vec<String> {
    app.rows
        .iter()
        .filter_map(|row| match row {
            Row::Entry(entry) => Some(entry.name.clone()),
            _ => None,
        })
        .collect()
}

fn area_of(app: &App, id: &str) -> usize {
    app.catalog
        .tabs
        .iter()
        .position(|tab| tab.entries().iter().any(|entry| entry.id == id))
        .unwrap()
}

#[test]
fn search_puts_the_closest_names_first() {
    let mut app = app();
    press(&mut app, KeyCode::Char('/'));
    type_text(&mut app, "git");
    let names = entry_names(&app);
    assert_eq!(names.first().map(String::as_str), Some("Git"), "{names:?}");
    // A loose spelling still finds it.
    press(&mut app, KeyCode::Esc);
    press(&mut app, KeyCode::Char('/'));
    type_text(&mut app, "rpgrp");
    assert!(entry_names(&app).contains(&"Ripgrep".to_string()));
}

#[test]
fn escaping_a_search_returns_to_the_row_it_left() {
    let mut app = app();
    app.open_screen(Screen::Area(area_of(&app, "ripgrep")));
    app.focus = Focus::Main;
    app.main_state.select(Some(2));
    press(&mut app, KeyCode::Char('/'));
    type_text(&mut app, "zzz");
    press(&mut app, KeyCode::Esc);
    assert_eq!(app.main_state.selected(), Some(2));
}

#[test]
fn the_filter_shows_installed_entries_then_updates() {
    let mut app = app();
    app.statuses.insert(
        "ripgrep".into(),
        Status::Update {
            source: "pacman".into(),
            version: Some("15.2.0-1".into()),
            latest: Some("15.3.0-1".into()),
            shadowed: None,
        },
    );
    app.statuses.insert(
        "fzf".into(),
        Status::Managed {
            by: "Home Manager".into(),
            version: None,
        },
    );
    let index = area_of(&app, "ripgrep");
    assert_eq!(index, area_of(&app, "fzf"), "both share an area");
    app.open_screen(Screen::Area(index));
    app.focus = Focus::Main;
    press(&mut app, KeyCode::Char('i'));
    assert_eq!(entry_names(&app), ["fzf", "Ripgrep"]);
    assert!(screen(&mut app, 120, 40).contains("Entries · installed"));
    press(&mut app, KeyCode::Char('i'));
    assert_eq!(entry_names(&app), ["Ripgrep"]);
    press(&mut app, KeyCode::Char('i'));
    assert!(entry_names(&app).len() > 2, "back to everything");
}

#[test]
fn pinned_and_recent_entries_are_listed_as_favorites() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("favorites.toml");
    let mut app = app();
    app.favorites_path = Some(path.clone());
    assert!(screen(&mut app, 120, 40).contains("Favorites"));
    press(&mut app, KeyCode::Char('/'));
    type_text(&mut app, "ripgrep");
    press(&mut app, KeyCode::Enter);
    press(&mut app, KeyCode::Char('*'));
    assert_eq!(settings::load_favorites(&path), ["ripgrep"]);
    assert!(screen(&mut app, 120, 40).contains("Ripgrep ★"));

    app.search.clear();
    app.screen = Screen::Favorites;
    app.take_recent(vec![history::Record {
        unix_timestamp: 1,
        entries: vec!["fzf".into(), "ripgrep".into(), "gone".into()],
        names: Vec::new(),
        success: true,
        result: "exit 0".into(),
        log: None,
    }]);
    app.update_rows();
    app.main_state.select(Some(0));
    assert_eq!(entry_names(&app), ["Ripgrep", "fzf"], "pins come first");
    press(&mut app, KeyCode::Char('*'));
    assert!(settings::load_favorites(&path).is_empty());
    assert_eq!(entry_names(&app), ["fzf", "Ripgrep"]);
    app.recent.clear();
    app.update_rows();
    assert!(screen(&mut app, 120, 40).contains("Nothing here yet"));
}

fn channel(id: &str, name: &str, pending: Option<Vec<updates::Pending>>) -> updates::Channel {
    updates::Channel {
        id: id.into(),
        name: name.into(),
        pending,
        note: "from the cache".into(),
        commands: vec![vec!["sudo".into(), "pacman".into(), "-Syu".into()]],
        requires: vec!["pacman".into(), "sudo".into()],
        caveat: Some("updates the whole system".into()),
    }
}

#[test]
fn the_updates_screen_lists_channels_and_updates_through_them() {
    let mut app = app();
    app.channels = vec![
        channel(
            "pacman",
            "Arch packages",
            Some(vec![updates::Pending {
                name: "mesa".into(),
                from: Some("1-1".into()),
                to: Some("2-1".into()),
            }]),
        ),
        channel("home-manager", "Home Manager", None),
    ];
    app.open_screen(Screen::Updates);
    app.focus = Focus::Main;
    let text = screen(&mut app, 120, 40);
    assert!(text.contains("Arch packages"), "{text}");
    assert!(text.contains("1 update"));
    assert!(text.contains("not counted"));
    press(&mut app, KeyCode::Enter);
    let text = screen(&mut app, 120, 40);
    assert!(text.contains("mesa  1-1 → 2-1"), "{text}");
    assert!(text.contains("u runs: sudo pacman -Syu"));
    press(&mut app, KeyCode::Esc);
    app.popup = None;
    press(&mut app, KeyCode::Char('U'));
    assert!(matches!(app.popup, Some(Popup::Confirm(_))));
    let text = screen(&mut app, 120, 40);
    assert!(text.contains("Update Arch packages"), "{text}");
    assert!(
        !text.contains("Update Home Manager"),
        "uncounted channels wait for u"
    );
    press(&mut app, KeyCode::Char('n'));
    press(&mut app, KeyCode::Char('j'));
    press(&mut app, KeyCode::Char('u'));
    assert!(screen(&mut app, 120, 40).contains("Update Home Manager"));
    press(&mut app, KeyCode::Char('n'));
    // A copy that updates itself has nothing to run.
    let mut launcher = channel("helixforge", "Helix Toolbox", Some(Vec::new()));
    launcher.commands.clear();
    launcher.note = "1.0.4; it fetches the latest release each time it starts".into();
    app.channels.push(launcher);
    app.main_state.select(Some(2));
    press(&mut app, KeyCode::Char('u'));
    let text = screen(&mut app, 120, 40);
    assert!(text.contains("Nothing to run"), "{text}");
    assert!(text.contains("each time it starts"), "{text}");
    press(&mut app, KeyCode::Esc);
    app.popup = None;
    app.channels.truncate(1);
    app.channels[0].pending = Some(Vec::new());
    app.clamp_main();
    press(&mut app, KeyCode::Char('U'));
    assert!(screen(&mut app, 120, 40).contains("Nothing to update"));
}

#[test]
fn the_history_screen_lists_records() {
    let state = tempfile::tempdir().unwrap();
    let mut app = app();
    history::record_in(
        state.path(),
        &[("git".into(), "Git".into())],
        false,
        "exit 1",
        None,
    )
    .unwrap();
    app.records = history::read_in(state.path(), 10).unwrap();
    app.screen = Screen::History;
    app.focus = Focus::Main;
    app.clamp_main();
    let text = screen(&mut app, 120, 40);
    assert!(text.contains("failed"));
    assert!(text.contains("Git"));
    assert!(text.contains("exit 1"));
    press(&mut app, KeyCode::Enter);
    assert!(screen(&mut app, 120, 40).contains("No log was saved"));
}

#[test]
fn a_run_that_worked_becomes_a_hardware_report_to_send() {
    let state = tempfile::tempdir().unwrap();
    let reports = tempfile::tempdir().unwrap();
    let mut app = app();
    app.reports_dir = Some(reports.path().to_path_buf());
    app.machine = Some(hardware_report::Machine {
        distribution: "Garuda Linux".into(),
        model: Some("Lenovo ThinkPad T15p Gen 2i".into()),
        graphics: vec!["NVIDIA GeForce GTX 1650 Mobile / Max-Q (nvidia)".into()],
        architecture: "x86_64".into(),
        ..Default::default()
    });
    history::record_in(
        state.path(),
        &[("git".into(), "Git".into())],
        false,
        "exit 1",
        None,
    )
    .unwrap();
    history::record_in(
        state.path(),
        &[("git".into(), "Git".into()), ("meld".into(), "Meld".into())],
        true,
        "exit status 0",
        None,
    )
    .unwrap();
    app.records = history::read_in(state.path(), 10).unwrap();
    app.open_screen(Screen::History);
    app.records = history::read_in(state.path(), 10).unwrap();
    app.focus = Focus::Main;
    let successful = app
        .records
        .iter()
        .position(|record| record.success)
        .unwrap();
    app.main_state.select(Some(1 - successful));
    assert!(!screen(&mut app, 140, 40).contains("Report it worked"));
    press(&mut app, KeyCode::Char('w'));
    assert!(screen(&mut app, 140, 40).contains("No hardware report"));
    press(&mut app, KeyCode::Esc);

    app.main_state.select(Some(successful));
    assert!(screen(&mut app, 140, 40).contains("w Report it worked"));
    press(&mut app, KeyCode::Char('w'));
    let shown = screen(&mut app, 140, 40);
    assert!(shown.contains("[[hardware]]"), "{shown}");
    assert!(shown.contains("o Open the issue"), "{shown}");
    let Some(Popup::HardwareReport(_, url)) = &app.popup else {
        panic!("no hardware report");
    };
    assert!(url.starts_with(hardware_report::ISSUES));
    assert!(
        url.contains("entry%20%3D%20%22meld%22"),
        "both entries: {url}"
    );
    let saved = std::fs::read_to_string(reports.path().join("hardware-report.md")).unwrap();
    assert!(saved.contains(url.as_str()));
    // c copies the report with its issue link, as the window's Copy does.
    app.copier = |text| {
        if text.contains("[[hardware]]") && text.contains("The issue, filled in:") {
            Ok("Copied with xclip.".into())
        } else {
            Err(format!("not the report: {text}"))
        }
    };
    press(&mut app, KeyCode::Char('c'));
    assert!(screen(&mut app, 140, 40).contains("Copied with xclip."));
    assert!(matches!(app.popup, Some(Popup::HardwareReport(..))));
    app.opener = |url| {
        if url.starts_with(hardware_report::ISSUES) {
            Ok(())
        } else {
            Err("not the issue".into())
        }
    };
    press(&mut app, KeyCode::Char('o'));
    assert!(screen(&mut app, 140, 40).contains("open in your browser"));
    press(&mut app, KeyCode::Esc);

    // From Reliability, one entry's.
    app.open_screen(Screen::Reliability);
    app.focus = Focus::Main;
    let row = app
        .reliability_rows
        .iter()
        .position(|(id, _, _)| id == "meld")
        .unwrap();
    app.main_state.select(Some(row));
    assert!(screen(&mut app, 140, 40).contains("w Report it worked"));
    press(&mut app, KeyCode::Char('w'));
    let Some(Popup::HardwareReport(_, url)) = &app.popup else {
        panic!("no hardware report");
    };
    assert!(url.contains("entry%20%3D%20%22meld%22") && !url.contains("%22git%22"));
}

#[test]
fn c_checks_an_entry_now_and_says_when_there_is_nothing_to_check() {
    let mut app = app();
    let area = area_of(&app, "git");
    app.open_screen(Screen::Area(area));
    app.focus = Focus::Main;
    app.search = "git".chars().collect();
    app.update_rows();
    let row = app
        .rows
        .iter()
        .position(|row| matches!(row, Row::Entry(entry) if entry.id == "git"))
        .unwrap();
    app.main_state.select(Some(row));
    assert!(screen(&mut app, 140, 40).contains("c Check"));
    press(&mut app, KeyCode::Char('c'));
    let shown = screen(&mut app, 140, 40);
    assert!(shown.contains("Checks: Git"), "{shown}");
    assert!(
        shown.contains("verified: Git") || shown.contains("Verified: Git"),
        "a package entry is checked for its package: {shown}"
    );
    press(&mut app, KeyCode::Esc);
    // An entry that installs nothing it can find and declares no checks.
    let nothing = app
        .catalog
        .entries()
        .into_iter()
        .find(|entry| {
            matches!(entry.action, Action::Script { .. })
                && entry.detect.is_none()
                && entry.effects.checks.is_empty()
        })
        .unwrap()
        .clone();
    app.rows = vec![Row::Entry(Box::new(nothing))];
    app.main_state.select(Some(0));
    press(&mut app, KeyCode::Char('c'));
    assert!(screen(&mut app, 140, 40).contains("nothing to check"));
}

fn stand_in_services() -> services::Services {
    let unit = |name: &str, enablement: &str, active: &str, user: bool| services::Unit {
        name: name.into(),
        description: format!("About {name}"),
        enablement: Some(enablement.into()),
        active: Some(active.into()),
        sub: Some(
            if active == "active" {
                "running"
            } else {
                "dead"
            }
            .into(),
        ),
        user,
    };
    services::Services {
        systemd: true,
        session: true,
        units: vec![
            unit("broken.service", "enabled", "failed", false),
            unit("fstrim.timer", "disabled", "inactive", false),
            unit("sddm.service", "enabled", "active", false),
            unit("pipewire.service", "indirect", "active", true),
        ],
        autostart: vec![services::Autostart {
            file: "discover.desktop".into(),
            name: "Discover".into(),
            exec: "discover --notifier".into(),
            enabled: true,
            here: true,
            system: true,
            ..Default::default()
        }],
    }
}

#[test]
fn the_services_screen_lists_units_and_asks_before_stopping_what_is_needed() {
    let mut app = app();
    app.services_found = Some(stand_in_services());
    app.open_screen(Screen::Services);
    app.focus = Focus::Main;
    let shown = screen(&mut app, 140, 40);
    assert!(shown.contains("The machine's services (2)"), "{shown}");
    assert!(shown.contains("sddm.service") && !shown.contains("pipewire.service"));
    let row = |app: &App, name: &str| {
        app.service_rows
                .iter()
                .position(|row| matches!(row, ServiceRow::Unit(index) if app.unit(*index).unwrap().name == name))
                .unwrap()
    };
    app.main_state.select(Some(row(&app, "sddm.service")));
    assert!(screen(&mut app, 140, 40).contains("s Stop"));
    press(&mut app, KeyCode::Enter);
    assert!(screen(&mut app, 140, 40).contains("login screen"));
    press(&mut app, KeyCode::Esc);
    // Disabling the login screen is high risk, and says why first.
    press(&mut app, KeyCode::Char('e'));
    assert!(matches!(app.popup, Some(Popup::Confirm(_))));
    let shown = screen(&mut app, 140, 40);
    assert!(
        shown.contains("Risk: high") && shown.contains("Warning: It runs the login screen"),
        "{shown}"
    );
    assert!(shown.contains("sudo systemctl disable sddm.service"));
    press(&mut app, KeyCode::Char('n'));
    // The views: timers, your session's, at login, failed.
    press(&mut app, KeyCode::Char('i'));
    assert!(screen(&mut app, 140, 40).contains("fstrim.timer"));
    press(&mut app, KeyCode::Char('i'));
    assert!(screen(&mut app, 140, 40).contains("pipewire.service"));
    press(&mut app, KeyCode::Char('i'));
    assert!(screen(&mut app, 140, 40).contains("Discover"));
    press(&mut app, KeyCode::Char('i'));
    let shown = screen(&mut app, 140, 40);
    assert!(shown.contains("broken.service") && !shown.contains("sddm.service"));
    // The filter narrows any view.
    press(&mut app, KeyCode::Char('i'));
    press(&mut app, KeyCode::Char('f'));
    answer(&mut app, "sddm");
    let shown = screen(&mut app, 140, 40);
    assert!(
        shown.contains("The machine's services (1)") && shown.contains("with \"sddm\""),
        "{shown}"
    );
}

#[test]
fn the_storage_screen_shows_what_each_cleanup_frees_and_what_stays() {
    let mut app = app();
    app.storage_found = Some(storage::Storage {
        filesystems: vec![storage::Filesystem {
            path: "/".into(),
            free: 40_000_000_000,
            total: 500_000_000_000,
        }],
        items: vec![
            storage::Item {
                id: "package-cache".into(),
                title: "Package cache".into(),
                detail: "Downloaded packages.".into(),
                size: Some(2_000_000_000),
                frees: Some(1_500_000_000),
                ..Default::default()
            },
            storage::Item {
                id: "trash".into(),
                title: "The trash".into(),
                detail: "Deleted files.".into(),
                size: Some(0),
                not_offered: Some("the trash is empty".into()),
                ..Default::default()
            },
        ],
        kept: vec![storage::Kept {
            title: "Kernels".into(),
            detail: "2 installed: never removed here.".into(),
            size: None,
        }],
    });
    app.open_screen(Screen::Storage);
    app.focus = Focus::Main;
    let shown = screen(&mut app, 140, 40);
    for text in [
        "40.0 GB free of 500 GB",
        "frees 1.5 GB",
        "the trash is empty",
        "Kept, never cleaned here",
        "Kernels",
    ] {
        assert!(shown.contains(text), "{text}: {shown}");
    }
    let row = |app: &App, wanted: usize| {
        app.storage_rows
            .iter()
            .position(|row| matches!(row, StorageRow::Item(index) if *index == wanted))
            .unwrap()
    };
    app.main_state.select(Some(row(&app, 1)));
    press(&mut app, KeyCode::Enter);
    assert!(screen(&mut app, 140, 40).contains("not offered"));
    press(&mut app, KeyCode::Esc);
    app.main_state.select(Some(row(&app, 0)));
    assert!(screen(&mut app, 140, 40).contains("Enter Clean"));
    press(&mut app, KeyCode::Char('d'));
    assert!(screen(&mut app, 140, 40).contains("frees about 1.5 GB"));
}

#[test]
fn the_performance_screen_sets_this_boot_beside_an_earlier_one() {
    let mut app = app();
    let boot = |id: &str, booted: u64, total: u64, waited: u64| performance::Boot {
        id: id.into(),
        booted,
        kernel: format!("7.2.{booted}"),
        phases: performance::Phases {
            kernel: 1000,
            userspace: total - 1000,
            total,
            ..Default::default()
        },
        slowest: vec![performance::Timed {
            name: "plocate-updatedb.service".into(),
            took: 4183,
        }],
        chain: vec![performance::Link {
            name: "NetworkManager-wait-online.service".into(),
            at: Some(1000),
            took: Some(waited),
        }],
    };
    let now = boot("now", 3000, 20_000, 6000);
    let before = boot("before", 2000, 15_000, 1000);
    let oldest = boot("oldest", 1000, 30_000, 1000);
    let found = performance::Performance {
        previous: Some(performance::compare(&before, &now, &[])),
        hints: performance::hints(Some(&now), &performance::Now::default()),
        boot: Some(now.clone()),
        recorded: 3,
        ..Default::default()
    };
    app.performance_found = Some((found, vec![now, before, oldest]));
    app.open_screen(Screen::Performance);
    app.focus = Focus::Main;
    let shown = screen(&mut app, 140, 50);
    for text in [
        "Kernel 1.0 s · Services 19.0 s",
        "Against the boot of 1970-01-01 00:33 UTC: +5.0 s",
        "NetworkManager-wait-online.service",
        "(+5.0 s)",
        "The login screen waited for",
        "The boot waited 6.0 s for NetworkManager-wait-online.service",
        "Enter turns it off",
        "b An earlier boot",
    ] {
        assert!(shown.contains(text), "{text}: {shown}");
    }
    // b sets it beside the boot before that one, and back.
    press(&mut app, KeyCode::Char('b'));
    let shown = screen(&mut app, 140, 50);
    assert!(
        shown.contains("Against the boot of 1970-01-01 00:16 UTC: -10.0 s"),
        "{shown}"
    );
    press(&mut app, KeyCode::Char('b'));
    assert!(screen(&mut app, 140, 50).contains("+5.0 s"));

    let hint = app
        .performance_rows
        .iter()
        .position(|row| matches!(row, PerformanceRow::Hint(_)))
        .unwrap();
    app.main_state.select(Some(hint));
    press(&mut app, KeyCode::Enter);
    assert!(matches!(app.popup, Some(Popup::Confirm(_))));
    let shown = screen(&mut app, 140, 50);
    assert!(
        shown.contains("sudo systemctl disable NetworkManager-wait-online.service"),
        "{shown}"
    );
    press(&mut app, KeyCode::Char('n'));
    // A unit opens in Services, filtered to it.
    let unit = app
            .performance_rows
            .iter()
            .position(|row| matches!(row, PerformanceRow::Unit(name, _) if name == "plocate-updatedb.service"))
            .unwrap();
    app.main_state.select(Some(unit));
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.screen, Screen::Services);
    assert_eq!(app.service_filter, "plocate-updatedb.service");
}

#[test]
fn the_recovery_screen_says_what_is_ready_and_offers_the_snapshot_check() {
    let mut app = app();
    let check =
        |id: &str, title: &str, health, summary: &str, entry: Option<&str>| doctor::Diagnosis {
            id: id.into(),
            title: title.into(),
            health,
            summary: summary.into(),
            evidence: vec!["The root file system is btrfs".into()],
            advice: None,
            repair: None,
            entry: entry.map(str::to_string),
        };
    app.recovery_found = vec![
        check(
            "snapshots",
            "System snapshots",
            doctor::Health::Good,
            "Snapper has 2 snapshots",
            None,
        ),
        check(
            "own-files",
            "Your own files",
            doctor::Health::Notice,
            "Nothing Helix Toolbox knows of backs up your home folder",
            None,
        ),
        check(
            "second-kernel",
            "A second kernel",
            doctor::Health::Notice,
            "Only one kernel",
            Some("zram-swap"),
        ),
    ];
    app.snapshot_tool = Some(backups::Snapshots::Snapper);
    app.open_screen(Screen::Recovery);
    app.focus = Focus::Main;
    let shown = screen(&mut app, 140, 40);
    for text in [
        "Recovery",
        "Snapper has 2 snapshots",
        "c checks them",
        "Notice  Your own files",
        "f sets it up",
    ] {
        assert!(shown.contains(text), "{text}: {shown}");
    }
    press(&mut app, KeyCode::Enter);
    let shown = screen(&mut app, 140, 40);
    assert!(shown.contains("looks inside the newest"), "{shown}");
    press(&mut app, KeyCode::Esc);
    press(&mut app, KeyCode::Char('c'));
    assert!(matches!(app.popup, Some(Popup::Confirm(_))));
    let shown = screen(&mut app, 140, 40);
    assert!(shown.contains("sudo snapper -c root list"), "{shown}");
    press(&mut app, KeyCode::Char('n'));
    app.main_state.select(Some(2));
    press(&mut app, KeyCode::Char('f'));
    assert!(matches!(app.popup, Some(Popup::Confirm(_))));
}

/// A migration made on Fedora, written where a test can open it.
fn fedora_migration(directory: &Path) -> PathBuf {
    let path = directory.join("from-fedora.toml");
    std::fs::write(
            &path,
            "version = 1\nfrom = \"Fedora Linux 43\"\nfamily = \"fedora\"\narchitecture = \"x86_64\"\nmade = 0\nentries = [\"git\", \"no-such-entry\"]\nflatpaks = [\"com.spotify.Client\"]\npackages = [\"python3-requests\"]\n",
        )
        .unwrap();
    path
}

/// restic answering from a fixed repository, for the Restic screen's
/// tests.
struct FakeRestic;

impl Restic for FakeRestic {
    fn run(&self, args: &[String]) -> Result<String, String> {
        let line = args.join(" ");
        if line.contains(" ls ") {
            Ok([
                    r#"{"struct_type":"snapshot","id":"b6f77f6d00"}"#,
                    r#"{"name":"me","type":"dir","path":"/home/me","struct_type":"node"}"#,
                    r#"{"name":"Documents","type":"dir","path":"/home/me/Documents","struct_type":"node"}"#,
                    r#"{"name":"notes.txt","type":"file","path":"/home/me/notes.txt","size":6,"struct_type":"node"}"#,
                ]
                .join("\n"))
        } else if line.contains(" snapshots ") {
            Ok(r#"[{"id":"b6f77f6d00","short_id":"b6f77f6d","time":"2026-10-21T22:00:04+02:00","hostname":"t15p","paths":["/home/me"]},{"id":"aa0d9cfe00","short_id":"aa0d9cfe","time":"2026-10-20T22:00:04+02:00","hostname":"t15p","paths":["/home/me"]}]"#.to_string())
        } else if line.contains(" find ") {
            Ok(r#"[{"matches":[{"type":"file","path":"/home/me/notes.txt","size":6}],"snapshot":"b6f77f6d00"},{"matches":[{"type":"file","path":"/home/me/notes.txt","size":5}],"snapshot":"aa0d9cfe00"}]"#.to_string())
        } else {
            Err(format!("not answered: {line}"))
        }
    }
}

fn restic_app(baselines: Vec<restic::Baseline>) -> App {
    let mut app = app();
    app.restic = std::sync::Arc::new(FakeRestic);
    let config = restic::Config {
        repository: "/run/media/me/Backup/restic".into(),
        origin: restic::Origin::Settings,
        password: restic::Password::File("/home/me/.restic-password".into()),
        paths: vec!["/home/me".into()],
        excludes: Vec::new(),
    };
    app.restic_reading = Some(ResticReading {
        snapshots: restic::snapshots(&FakeRestic, &config),
        found: restic::Found::Ready(config),
        restores: true,
        timer: None,
        baselines,
    });
    app
}

#[test]
fn the_restic_screen_lists_snapshots_browses_them_and_finds_a_files_history() {
    let mut app = app();
    app.restic_reading = Some(ResticReading {
        found: restic::Found::Missing,
        snapshots: Ok(Vec::new()),
        restores: false,
        timer: None,
        baselines: Vec::new(),
    });
    app.open_screen(Screen::Restic);
    let shown = screen(&mut app, 140, 40);
    assert!(shown.contains("No restic repository is set up"), "{shown}");
    let mut app = restic_app(Vec::new());
    app.open_screen(Screen::Restic);
    app.focus = Focus::Main;
    let shown = screen(&mut app, 140, 40);
    for text in [
        "/run/media/me/Backup/restic (from Helix Toolbox's settings)",
        "Snapshots (2)",
        "b6f77f6d  2026-10-21 22:00",
        "Recovery tests (0 baselines)",
    ] {
        assert!(shown.contains(text), "{text}: {shown}");
    }
    let first = app
        .restic_rows
        .iter()
        .position(|row| matches!(row, ResticRow::Snapshot(0)))
        .unwrap();
    app.main_state.select(Some(first));
    press(&mut app, KeyCode::Enter);
    let shown = screen(&mut app, 140, 40);
    for text in ["In b6f77f6d at /home/me", "Documents/", "notes.txt", ".."] {
        assert!(shown.contains(text), "{text}: {shown}");
    }
    press(&mut app, KeyCode::Char('s'));
    assert!(matches!(
        app.popup,
        Some(Popup::Ask(_, Asked::ResticHistory))
    ));
    type_text(&mut app, "notes.txt");
    press(&mut app, KeyCode::Enter);
    let shown = screen(&mut app, 140, 40);
    for text in [
        "History of notes.txt",
        "/home/me/notes.txt",
        "changed",
        "first",
    ] {
        assert!(shown.contains(text), "{text}: {shown}");
    }
}

#[test]
fn the_restic_screen_reviews_restores_backups_and_recovery_tests_before_they_run() {
    let baseline = restic::Baseline {
        id: "1792600000000".into(),
        path: "/home/me/notes.txt".into(),
        sha256: "5891b5b522d5df086d0ff0b110fbd9d21bb4fc7163af34d08286a2e846f6be03".into(),
        recorded: 1_792_600_000,
    };
    let mut app = restic_app(vec![baseline]);
    app.open_screen(Screen::Restic);
    app.focus = Focus::Main;
    press(&mut app, KeyCode::Char('b'));
    assert!(matches!(app.popup, Some(Popup::Confirm(_))));
    assert!(screen(&mut app, 140, 40).contains("'backup' '--exclude-caches'"));
    press(&mut app, KeyCode::Char('n'));
    assert!(!matches!(app.popup, Some(Popup::Running(_))));
    // A restore asks for a new folder, which must not be there yet.
    let first = app
        .restic_rows
        .iter()
        .position(|row| matches!(row, ResticRow::Snapshot(0)))
        .unwrap();
    app.main_state.select(Some(first));
    press(&mut app, KeyCode::Char('R'));
    assert!(matches!(
        app.popup,
        Some(Popup::Ask(_, Asked::ResticRestore(..)))
    ));
    let home = std::env::var("HOME").unwrap();
    for _ in 0.."~/restored".len() {
        press(&mut app, KeyCode::Backspace);
    }
    type_text(&mut app, &home);
    press(&mut app, KeyCode::Enter);
    assert!(screen(&mut app, 140, 40).contains("is there already"));
    press(&mut app, KeyCode::Esc);
    // v on a baseline tests a restore of it, after review.
    let baseline = app
        .restic_rows
        .iter()
        .position(|row| matches!(row, ResticRow::Baseline(0)))
        .unwrap();
    app.main_state.select(Some(baseline));
    press(&mut app, KeyCode::Char('v'));
    assert!(matches!(
        app.popup,
        Some(Popup::Ask(_, Asked::ResticTest(_)))
    ));
    press(&mut app, KeyCode::Enter);
    assert!(matches!(app.popup, Some(Popup::Confirm(_))));
    let shown = screen(&mut app, 140, 40);
    assert!(
        shown.contains("Test a restore against its baseline"),
        "{shown}"
    );
    press(&mut app, KeyCode::Char('n'));
    assert!(!matches!(app.popup, Some(Popup::Running(_))));
}

/// An app reading configurations in a home of its own: a Ghostty
/// configuration with a backup beside it, and a Fastfetch one that does
/// not check.
fn configs_app(home: &Path) -> App {
    let config = home.join(".config");
    std::fs::create_dir_all(config.join("ghostty")).unwrap();
    std::fs::create_dir_all(config.join("fastfetch")).unwrap();
    std::fs::write(config.join("ghostty/config"), "font-size = 11\n").unwrap();
    std::fs::write(
        config.join("ghostty/config.helix-toolbox-backup-20261001000000-1"),
        "font-size = 10\n",
    )
    .unwrap();
    std::fs::write(
        config.join("fastfetch/config.jsonc"),
        "{ \"modules\": \"os\" }\n",
    )
    .unwrap();
    let mut app = app();
    app.config_places = Some((home.to_path_buf(), config));
    app.config_folder = Some(home.join("copies"));
    app
}

#[test]
fn the_configs_screen_shows_both_files_their_checks_fields_and_backups() {
    let directory = tempfile::tempdir().unwrap();
    let home = directory.path().canonicalize().unwrap();
    let mut app = configs_app(&home);
    app.open_screen(Screen::Configs);
    app.focus = Focus::Main;
    let shown = screen(&mut app, 160, 50);
    for text in [
        "Ghostty",
        "named by the live file",
        "Checks: Not checked: Ghostty is not installed here",
        "Font size",
        "11",
        "Edit the whole file in your editor",
        "backups kept (1), newest first:",
        "2026-10-01 00:00:00",
        "Fastfetch",
        "Checks: Invalid: \"modules\" must be a list",
    ] {
        assert!(shown.contains(text), "{text}: {shown}");
    }
    // d tells where it is and what a save keeps.
    let field = app
        .config_rows
        .iter()
        .position(|row| matches!(row, ConfigRow::Field(configs::Kind::Ghostty, 1)))
        .unwrap();
    app.main_state.select(Some(field));
    press(&mut app, KeyCode::Char('d'));
    let told = screen(&mut app, 160, 50);
    assert!(told.contains("SHA-256"), "{told}");
    assert!(told.contains("as a backup beside it"), "{told}");
}

#[test]
fn the_configs_screen_reviews_a_change_an_edit_and_a_backup_before_anything_is_saved() {
    let directory = tempfile::tempdir().unwrap();
    let home = directory.path().canonicalize().unwrap();
    let file = home.join(".config/ghostty/config");
    let mut app = configs_app(&home);
    app.open_screen(Screen::Configs);
    app.focus = Focus::Main;
    // Enter on a field asks for its value, then reviews the save.
    let field = app
        .config_rows
        .iter()
        .position(|row| matches!(row, ConfigRow::Field(configs::Kind::Ghostty, 1)))
        .unwrap();
    app.main_state.select(Some(field));
    press(&mut app, KeyCode::Enter);
    assert!(matches!(
        app.popup,
        Some(Popup::Ask(
            _,
            Asked::ConfigField(configs::Kind::Ghostty, "font-size")
        ))
    ));
    press(&mut app, KeyCode::Backspace);
    press(&mut app, KeyCode::Backspace);
    type_text(&mut app, "14");
    press(&mut app, KeyCode::Enter);
    // It goes into the draft, and s reviews saving it.
    assert!(app.popup.is_none());
    let shown = screen(&mut app, 160, 50);
    assert!(shown.contains("Draft: changed, not saved yet"), "{shown}");
    assert!(shown.contains("Font size                 14"), "{shown}");
    press(&mut app, KeyCode::Char('s'));
    assert!(matches!(app.popup, Some(Popup::Confirm(_))));
    let shown = screen(&mut app, 160, 50);
    assert!(shown.contains("Save Ghostty's configuration"), "{shown}");
    press(&mut app, KeyCode::Char('n'));
    assert!(!matches!(app.popup, Some(Popup::Running(_))));
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "font-size = 11\n");
    // E hands a private copy to the editor; what comes back is reviewed.
    press(&mut app, KeyCode::Char('E'));
    let request = app.take_edit().expect("a copy for the editor");
    assert!(request.copy.starts_with(home.join("copies")));
    std::fs::write(&request.copy, "font-size = 16\n").unwrap();
    let copy = request.copy.clone();
    app.edited(request, Ok(()));
    assert!(!copy.exists(), "the copy is removed");
    assert!(matches!(app.popup, Some(Popup::Confirm(_))));
    press(&mut app, KeyCode::Char('n'));
    // An edit of Fastfetch that does not check is refused.
    let fastfetch = app
        .config_rows
        .iter()
        .position(|row| matches!(row, ConfigRow::Edit(configs::Kind::Fastfetch)))
        .unwrap();
    app.main_state.select(Some(fastfetch));
    press(&mut app, KeyCode::Enter);
    let request = app.take_edit().unwrap();
    std::fs::write(&request.copy, "{ \"modules\": [ { } ] }").unwrap();
    app.edited(request, Ok(()));
    let shown = screen(&mut app, 160, 50);
    assert!(shown.contains("Not saved: module 1"), "{shown}");
    press(&mut app, KeyCode::Esc);
    // A backup is put back after review.
    let backup = app
        .config_rows
        .iter()
        .position(|row| matches!(row, ConfigRow::Backup(configs::Kind::Ghostty, 0)))
        .unwrap();
    app.main_state.select(Some(backup));
    press(&mut app, KeyCode::Enter);
    assert!(matches!(app.popup, Some(Popup::Confirm(_))));
    press(&mut app, KeyCode::Char('n'));
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "font-size = 11\n");
}

#[test]
fn the_migration_screen_says_what_a_migration_means_here() {
    let directory = tempfile::tempdir().unwrap();
    let path = fedora_migration(directory.path());
    let mut app = app();
    app.migration_here = Some(migration::Migration {
        version: migration::VERSION,
        from: "Garuda Linux".into(),
        entries: vec!["git".into()],
        flatpaks: vec!["org.kde.haruna".into()],
        packages: vec!["htop".into(), "nano".into()],
        ..Default::default()
    });
    app.open_screen(Screen::Migration);
    app.focus = Focus::Main;
    let shown = screen(&mut app, 140, 40);
    for text in [
        "Migration",
        "This machine",
        "1 entries, 1 Flatpaks and 2 packages of yours, from Garuda Linux",
        "e Export",
        "o Open a file",
    ] {
        assert!(shown.contains(text), "{text}: {shown}");
    }
    press(&mut app, KeyCode::Char('o'));
    assert!(matches!(
        app.popup,
        Some(Popup::Ask(_, Asked::OpenMigration))
    ));
    type_text(&mut app, &path.display().to_string());
    press(&mut app, KeyCode::Enter);
    assert!(app.popup.is_none());
    let shown = screen(&mut app, 140, 40);
    for text in [
        "From Fedora Linux 43 (x86_64), made 1970-01-01 00:00 UTC",
        "To install (2)",
        "Git",
        "com.spotify.Client",
        "Flathub",
        "Cannot be installed here (1)",
        "no-such-entry",
        "not in the catalog here",
        "Left to you (1)",
        "python3-requests",
        "named as on Fedora Linux 43; look for what Garuda Linux calls it",
        "a Install",
    ] {
        assert!(shown.contains(text), "{text}: {shown}");
    }
    // d tells of the selected one.
    let left = app
            .migration_rows
            .iter()
            .position(|row| {
                matches!(row, MigrationRow::Item(index)
                    if app.migration_opened.as_ref().unwrap().1.items[*index].id == "python3-requests")
            })
            .unwrap();
    app.main_state.select(Some(left));
    press(&mut app, KeyCode::Char('d'));
    let shown = screen(&mut app, 140, 40);
    assert!(shown.contains("It is left to you"), "{shown}");
    press(&mut app, KeyCode::Esc);
    // A file that is not a migration says why.
    std::fs::write(directory.path().join("bad.toml"), "packages = 1\n").unwrap();
    press(&mut app, KeyCode::Char('o'));
    for _ in 0..path.display().to_string().len() {
        press(&mut app, KeyCode::Backspace);
    }
    type_text(
        &mut app,
        &directory.path().join("bad.toml").display().to_string(),
    );
    press(&mut app, KeyCode::Enter);
    let shown = screen(&mut app, 140, 40);
    assert!(shown.contains("Cannot open the migration"), "{shown}");
    // The one opened before stays.
    assert!(app.migration_opened.is_some());
}

#[test]
fn the_migration_screen_installs_what_it_can_after_review() {
    let directory = tempfile::tempdir().unwrap();
    let path = fedora_migration(directory.path());
    let mut app = app();
    app.open_screen(Screen::Migration);
    app.focus = Focus::Main;
    // Nothing open yet: a says how to open one.
    press(&mut app, KeyCode::Char('a'));
    assert!(screen(&mut app, 140, 40).contains("No migration open"));
    press(&mut app, KeyCode::Esc);
    app.open_migration(path);
    press(&mut app, KeyCode::Char('a'));
    assert!(matches!(app.popup, Some(Popup::Confirm(_))));
    let shown = screen(&mut app, 140, 40);
    for text in ["sudo pacman -S --needed git", "2. com.spotify.Client"] {
        assert!(shown.contains(text), "{text}: {shown}");
    }
    assert!(!shown.contains("python3-requests"), "{shown}");
    // Declined, nothing runs.
    press(&mut app, KeyCode::Char('n'));
    assert!(!matches!(app.popup, Some(Popup::Running(_))));
    // Enter on a row to install plans the same.
    let install = app
        .migration_rows
        .iter()
        .position(|row| matches!(row, MigrationRow::Item(0)))
        .unwrap();
    app.main_state.select(Some(install));
    press(&mut app, KeyCode::Enter);
    assert!(matches!(app.popup, Some(Popup::Confirm(_))));
    press(&mut app, KeyCode::Char('n'));
    // e writes this machine's migration where it is told.
    app.migration_here = Some(migration::Migration {
        version: migration::VERSION,
        from: "Garuda Linux".into(),
        packages: vec!["htop".into()],
        ..Default::default()
    });
    press(&mut app, KeyCode::Char('e'));
    assert!(matches!(
        app.popup,
        Some(Popup::Ask(_, Asked::ExportMigration))
    ));
    let target = directory.path().join("out.toml");
    app.answered(Asked::ExportMigration, &target.display().to_string());
    let written = migration::read(&target).unwrap();
    assert_eq!(written.packages, ["htop"]);
    assert!(screen(&mut app, 140, 40).contains("Migration exported"));
}

/// A repository of the test's own under `folder/projects`: one commit
/// and a new file.
fn sample_repository(folder: &Path) -> PathBuf {
    let repo = folder.join("projects/sample");
    std::fs::create_dir_all(&repo).unwrap();
    let git = |args: &[&str]| {
        let output = std::process::Command::new("git")
            .arg("-C")
            .arg(&repo)
            .args(args)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_AUTHOR_NAME", "Test")
            .env("GIT_AUTHOR_EMAIL", "test@example.com")
            .env("GIT_COMMITTER_NAME", "Test")
            .env("GIT_COMMITTER_EMAIL", "test@example.com")
            .output()
            .unwrap();
        assert!(output.status.success(), "git {args:?}");
    };
    git(&["init", "-q", "-b", "main"]);
    std::fs::write(repo.join("one.txt"), "one\n").unwrap();
    git(&["add", "one.txt"]);
    git(&["commit", "-q", "-m", "First"]);
    std::fs::write(repo.join("new.txt"), "new\n").unwrap();
    repo.canonicalize().unwrap()
}

fn repository_app(folder: &Path) -> App {
    let mut app = app();
    app.repo_places = Some(repos::Places {
        roots: vec![folder.join("projects")],
        depth: 3,
        origin: "settings".into(),
    });
    app.workspaces_file = Some(folder.join("workspaces.toml"));
    app
}

fn select_repo_row(app: &mut App, wanted: impl Fn(&RepoRow) -> bool) {
    let index = app.repo_rows.iter().position(wanted).unwrap();
    app.main_state.select(Some(index));
}

#[test]
fn the_repositories_screen_lists_them_and_opens_one() {
    let folder = tempfile::tempdir().unwrap();
    sample_repository(folder.path());
    let mut app = repository_app(folder.path());
    app.open_screen(Screen::Repositories);
    app.focus = Focus::Main;
    let shown = screen(&mut app, 160, 40);
    for text in [
        "1 repositories",
        "sample",
        "main",
        "1 changed",
        "no upstream",
        "First",
    ] {
        assert!(shown.contains(text), "{text}: {shown}");
    }
    select_repo_row(&mut app, |row| matches!(row, RepoRow::Repository(0)));
    press(&mut app, KeyCode::Enter);
    let shown = screen(&mut app, 160, 40);
    for text in [
        "sample on main: no upstream",
        "Changes (1, 0 staged)",
        "new.txt",
        "Branches",
        "Tasks",
        "n keeps a command of yours",
        "Recent commits",
    ] {
        assert!(shown.contains(text), "{text}: {shown}");
    }
    // Enter on a file shows its diff.
    select_repo_row(&mut app, |row| matches!(row, RepoRow::File(0)));
    press(&mut app, KeyCode::Enter);
    assert!(screen(&mut app, 160, 40).contains("+new"));
    press(&mut app, KeyCode::Esc);
    // f keeps it as a favorite, marked in the list.
    press(&mut app, KeyCode::Char('f'));
    let kept = std::fs::read_to_string(folder.path().join("workspaces.toml")).unwrap();
    assert!(kept.contains("favorite = true"), "{kept}");
    press(&mut app, KeyCode::Char('h'));
    assert!(app.repo_open.is_none());
    assert!(screen(&mut app, 160, 40).contains("★ sample"));
}

#[test]
fn changes_to_a_repository_are_reviewed_before_anything_runs() {
    let folder = tempfile::tempdir().unwrap();
    let repo = sample_repository(folder.path());
    let mut app = repository_app(folder.path());
    app.open_screen(Screen::Repositories);
    app.focus = Focus::Main;
    app.open_repo(repo.clone());
    select_repo_row(&mut app, |row| matches!(row, RepoRow::File(0)));
    press(&mut app, KeyCode::Char(' '));
    assert!(matches!(app.popup, Some(Popup::Confirm(_))));
    let shown = screen(&mut app, 160, 40);
    for text in ["Stage 1 file in sample", "add --all -- new.txt"] {
        assert!(shown.contains(text), "{text}: {shown}");
    }
    // Declined: nothing is staged.
    press(&mut app, KeyCode::Char('n'));
    assert!(!matches!(app.popup, Some(Popup::Running(_))));
    let staged = std::process::Command::new("git")
        .arg("-C")
        .arg(&repo)
        .args(["diff", "--cached", "--name-only"])
        .output()
        .unwrap();
    assert!(staged.stdout.is_empty());
    // A commit with nothing staged says why.
    press(&mut app, KeyCode::Char('c'));
    assert!(matches!(
        app.popup,
        Some(Popup::Ask(_, Asked::CommitMessage))
    ));
    app.answered(Asked::CommitMessage, "Second");
    assert!(screen(&mut app, 160, 40).contains("nothing is staged"));
    press(&mut app, KeyCode::Esc);
    // A command of the user's becomes a task, reviewed before it runs.
    press(&mut app, KeyCode::Char('n'));
    assert!(matches!(app.popup, Some(Popup::Ask(_, Asked::AddTask))));
    app.answered(Asked::AddTask, "sh -c 'echo hi'");
    let kept = std::fs::read_to_string(folder.path().join("workspaces.toml")).unwrap();
    assert!(kept.contains("id = \"sh-c-echo-hi\""), "{kept}");
    press(&mut app, KeyCode::Esc);
    select_repo_row(&mut app, |row| matches!(row, RepoRow::Task(0)));
    press(&mut app, KeyCode::Enter);
    assert!(matches!(app.popup, Some(Popup::Confirm(_))));
    assert!(screen(&mut app, 160, 40).contains("sh -c 'echo hi'"));
    press(&mut app, KeyCode::Char('n'));
    // x takes the task away again.
    press(&mut app, KeyCode::Char('x'));
    let kept = std::fs::read_to_string(folder.path().join("workspaces.toml")).unwrap();
    assert!(!kept.contains("sh-c-echo-hi"), "{kept}");
}

#[test]
fn settings_change_the_theme_and_the_confirmation() {
    let mut app = app();
    app.open_screen(Screen::Settings);
    app.focus = Focus::Main;
    assert!(screen(&mut app, 120, 40).contains("Ask before running"));
    assert_eq!(app.theme, Theme::Solid, "solid is the default");
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.theme, Theme::Transparent);
    press(&mut app, KeyCode::Char('j'));
    press(&mut app, KeyCode::Enter);
    assert!(app.icons);
    press(&mut app, KeyCode::Char('j'));
    press(&mut app, KeyCode::Enter);
    assert!(app.skip_confirmation);
}

#[test]
fn settings_turn_the_update_notifications_on_and_off() {
    let directory = tempfile::tempdir().unwrap();
    let mut app = app();
    app.open_screen(Screen::Settings);
    app.focus = Focus::Main;
    app.main_state.select(Some(6));
    assert!(screen(&mut app, 120, 40).contains("not available here"));
    press(&mut app, KeyCode::Enter);
    assert!(app.popup.is_none(), "nothing to turn on without a home");
    let places = notifications::Places {
        units: directory.path().join("systemd/user"),
        told: directory.path().join("notified.txt"),
    };
    app.notices = Some(places.clone());
    app.settings_path = Some(directory.path().join("settings.toml"));
    app.systemctl = |_| Some(true);
    press(&mut app, KeyCode::Enter);
    assert!(notifications::is_on(&places));
    assert!(screen(&mut app, 120, 40).contains("Helix Toolbox now checks"));
    press(&mut app, KeyCode::Esc);
    assert!(screen(&mut app, 120, 40).contains("yes, every 6 hours"));
    // Health problems too, then updates off: the timer stays for them.
    app.main_state.select(Some(7));
    press(&mut app, KeyCode::Enter);
    assert!(screen(&mut app, 120, 40).contains("Helix Doctor finds a new problem"));
    press(&mut app, KeyCode::Esc);
    app.main_state.select(Some(6));
    press(&mut app, KeyCode::Enter);
    assert!(app.popup.is_none());
    assert!(notifications::is_on(&places));
    let watched = watch::read(&places, app.settings_path.as_deref());
    assert!(watched.health && !watched.updates);
    // Drift: no baseline saved yet, so nothing to choose.
    let baselines = tempfile::tempdir().unwrap();
    app.baselines_dir = Some(baselines.path().to_path_buf());
    assert!(screen(&mut app, 120, 40).contains("no (save one in Drift)"));
    std::fs::write(
        baselines.path().join("fresh.toml"),
        "name = \"Fresh\"\nsystem = \"Garuda Linux\"\ncreated = 1\n",
    )
    .unwrap();
    app.main_state.select(Some(8));
    press(&mut app, KeyCode::Enter);
    press(&mut app, KeyCode::Esc);
    assert!(screen(&mut app, 120, 40).contains("from fresh"));
    press(&mut app, KeyCode::Enter);
    assert!(
        watch::read(&places, app.settings_path.as_deref())
            .drift
            .is_none()
    );
    // Everything off removes the timer.
    app.main_state.select(Some(7));
    press(&mut app, KeyCode::Enter);
    assert!(!notifications::is_on(&places));
    assert!(app.popup.is_none());
    // Without systemd it says so, and nothing is written.
    app.systemctl = |_| None;
    app.main_state.select(Some(6));
    press(&mut app, KeyCode::Enter);
    assert!(screen(&mut app, 120, 40).contains("could not be turned on"));
    assert!(!places.units.join(notifications::TIMER).exists());
}

#[test]
fn help_and_flags_guide_open_as_popups() {
    let mut app = app();
    press(&mut app, KeyCode::Char('?'));
    assert!(screen(&mut app, 120, 40).contains("Install Plan"));
    press(&mut app, KeyCode::Esc);
    app.open_screen(Screen::Area(0));
    press(&mut app, KeyCode::Char('g'));
    assert!(screen(&mut app, 120, 40).contains("privileged"));
}

#[test]
fn the_dashboard_draws_cards_and_icons_in_every_theme() {
    let mut app = app();
    app.icons = true;
    for theme in [
        Theme::Solid,
        Theme::Transparent,
        Theme::Compatible,
        Theme::Monochrome,
    ] {
        app.theme = theme;
        let text = screen(&mut app, 120, 40);
        assert!(text.contains("\u{f0e4}  Dashboard"), "{text}");
        assert!(text.contains("\u{f0ad}  Maintenance"), "{text}");
        assert!(text.contains("╭"), "{text}");
        assert!(text.contains("See what is queued and run it"), "{text}");
        assert!(text.contains("? Help"), "{text}");
    }
    // The smallest supported size still shows every quick action.
    let text = screen(&mut app, 100, 25);
    assert!(text.contains("View History"), "{text}");
    assert!(text.contains("Install Plan"), "{text}");
    app.icons = false;
    assert!(!screen(&mut app, 120, 40).contains('\u{f0e4}'));
}

#[test]
fn monochrome_draws_no_color_and_still_shows_the_selection() {
    use ratatui::style::{Color, Modifier};
    let mut app = app();
    app.theme = Theme::Monochrome;
    app.open_screen(Screen::Area(0));
    app.focus = Focus::Main;
    for (width, height) in [(120, 40), (100, 25)] {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal.draw(|frame| app.draw(frame)).unwrap();
        let buffer = terminal.backend().buffer();
        let mut reversed = 0;
        for cell in buffer.content() {
            assert_eq!(cell.fg, Color::Reset, "{cell:?}");
            assert_eq!(cell.bg, Color::Reset, "{cell:?}");
            if cell.modifier.contains(Modifier::REVERSED) {
                reversed += 1;
            }
        }
        assert!(reversed > 0, "the selection shows in reverse video");
    }
    // The confirmation's buttons too.
    press(&mut app, KeyCode::Char('/'));
    type_text(&mut app, "System Report (script)");
    press(&mut app, KeyCode::Enter);
    press(&mut app, KeyCode::Enter);
    assert!(matches!(app.popup, Some(Popup::Confirm(_))));
    let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
    terminal.draw(|frame| app.draw(frame)).unwrap();
    assert!(
        terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .all(|cell| cell.fg == Color::Reset && cell.bg == Color::Reset)
    );
}

#[test]
fn no_color_is_for_one_run_and_the_chosen_theme_is_remembered() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("settings.toml");
    let mut app = app();
    app.saved_theme = Theme::Transparent;
    app.theme = Theme::Monochrome;
    app.settings_path = Some(path.clone());
    app.icons = false;
    app.save_look();
    assert_eq!(settings::load(&path).theme, Theme::Transparent);
    // Choosing a theme remembers it.
    press(&mut app, KeyCode::Char('t'));
    assert_eq!(app.theme, Theme::Solid);
    assert_eq!(settings::load(&path).theme, Theme::Solid);
}

fn arch_packages(names: &[&str]) -> Inventory {
    Inventory::from_parts(
        Some(helix_toolbox_core::PackageManager::Arch),
        Some(
            names
                .iter()
                .map(|name| (name.to_string(), "1-1".to_string()))
                .collect(),
        ),
        HashMap::new(),
        HashMap::new(),
        Default::default(),
    )
}

#[test]
fn drift_compares_with_a_saved_baseline_and_plans_what_is_missing() {
    let directory = tempfile::tempdir().unwrap();
    let mut app = app();
    app.baselines_dir = Some(directory.path().to_path_buf());
    app.inventory = Some(arch_packages(&["git", "ripgrep"]));
    app.open_screen(Screen::Drift);
    app.focus = Focus::Main;
    // Before any baseline, the profiles are there to compare with.
    let text = screen(&mut app, 140, 40);
    assert!(text.contains("Profile  Developer Basics"), "{text}");
    assert!(text.contains("Missing"), "{text}");
    press(&mut app, KeyCode::Char('s'));
    type_text(&mut app, "Old box");
    press(&mut app, KeyCode::Enter);
    assert!(screen(&mut app, 140, 40).contains("Baseline saved"));
    assert!(directory.path().join("old-box.toml").is_file());
    press(&mut app, KeyCode::Esc);
    let text = screen(&mut app, 140, 40);
    assert!(text.contains("Baseline  Old box"), "{text}");
    assert!(text.contains("No differences"), "{text}");
    assert!(text.contains("b: another baseline or profile (1 of"));

    // Ripgrep goes away; Drift names it and plans it back.
    app.inventory = Some(arch_packages(&["git"]));
    press(&mut app, KeyCode::Char('r'));
    let text = screen(&mut app, 140, 40);
    assert!(text.contains("Missing   Ripgrep"), "{text}");
    assert!(text.contains("a Plan all missing"), "{text}");
    press(&mut app, KeyCode::Enter);
    assert!(screen(&mut app, 140, 40).contains("The Ripgrep entry puts it back"));
    press(&mut app, KeyCode::Esc);
    press(&mut app, KeyCode::Char(' '));
    assert!(screen(&mut app, 140, 40).contains("· planned"));
    press(&mut app, KeyCode::Char(' '));
    assert!(app.queue.is_empty());
    press(&mut app, KeyCode::Char('a'));
    assert_eq!(app.screen, Screen::Plan);
    let ids: Vec<&str> = app.queue.iter().map(|entry| entry.id.as_str()).collect();
    assert_eq!(ids, ["ripgrep"]);
    assert!(screen(&mut app, 140, 40).contains("Nothing has run"));
    // b moves on to the profiles.
    app.popup = None;
    app.open_screen(Screen::Drift);
    app.focus = Focus::Main;
    press(&mut app, KeyCode::Char('b'));
    assert!(screen(&mut app, 140, 40).contains("Profile  "));
}

fn queue_ids(app: &App) -> Vec<String> {
    app.queue.iter().map(|entry| entry.id.clone()).collect()
}

#[test]
fn the_sidebar_counts_what_is_pinned_queued_and_waiting() {
    let mut app = app();
    for id in ["git", "ripgrep"] {
        let entry = app.catalog.find(id).unwrap().clone();
        app.toggle_queued(entry);
    }
    app.favorites.push("fzf".to_string());
    app.statuses.insert(
        "fzf".to_string(),
        Status::Update {
            source: "pacman".to_string(),
            version: Some("0.60".to_string()),
            latest: Some("0.61".to_string()),
            shadowed: None,
        },
    );
    let text = screen(&mut app, 120, 40);
    let line = |label: &str| {
        text.lines()
            .find(|line| line.contains(label))
            .unwrap_or_default()
            .to_string()
    };
    let sidebar = |line: String| {
        line.chars()
            .take(SIDEBAR_WIDTH as usize)
            .collect::<String>()
    };
    assert!(sidebar(line("Install Plan")).contains(" 2 │"), "{text}");
    assert!(sidebar(line("Favorites")).contains(" 1"), "{text}");
    assert!(sidebar(line("Updates")).contains(" 1"), "{text}");
    // Nothing to count, nothing shown.
    assert!(
        !sidebar(line("History")).chars().any(|c| c.is_ascii_digit()),
        "{text}"
    );
}

#[test]
fn the_header_shows_the_version_where_it_fits() {
    let mut app = app();
    let wide = screen(&mut app, 120, 40);
    assert!(
        wide.contains(concat!("v", env!("CARGO_PKG_VERSION"))),
        "{wide}"
    );
    app.size_bypass = true;
    let narrow = screen(&mut app, 40, 30);
    assert!(
        narrow.contains("HELIX TOOLBOX") && narrow.contains("SYSTEM READY"),
        "{narrow}"
    );
}

#[test]
fn the_footer_says_how_the_check_stands_and_ctrl_r_checks_again() {
    let mut app = app();
    assert!(!screen(&mut app, 120, 40).contains("package entries checked"));
    app.statuses.insert(
        "fzf".to_string(),
        Status::Absent {
            replaces: Vec::new(),
        },
    );
    let text = screen(&mut app, 120, 40);
    assert!(
        text.contains("1 package entries checked · Ctrl-R check again"),
        "{text}"
    );
    let (_sender, receiver) = mpsc::channel();
    app.checking = Some(receiver);
    assert!(screen(&mut app, 120, 40).contains("Checking what is installed…"));
    // Off in tests, Ctrl-R starts nothing, and the palette offers it too.
    app.checking = None;
    app.handle_key(&KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL));
    assert!(app.checking.is_none() && app.popup.is_none());
    let (choices, _) = app.palette_choices();
    assert!(
        choices
            .iter()
            .any(|choice| choice.label == "Check again what is installed")
    );
}

#[test]
fn area_rows_say_what_entries_and_groups_hold_and_results_where_they_are() {
    let mut app = app();
    app.open_screen(Screen::Area(app.first_area()));
    app.focus = Focus::Main;
    let text = screen(&mut app, 160, 40);
    let group = app
        .rows
        .iter()
        .find_map(|row| match row {
            Row::Group { name, count, .. } => Some((name.clone(), *count)),
            _ => None,
        })
        .unwrap();
    let line = text.lines().find(|line| line.contains(&group.0)).unwrap();
    assert!(
        line.trim_end()
            .trim_end_matches('│')
            .trim_end()
            .ends_with(&group.1.to_string()),
        "{line}"
    );
    press(&mut app, KeyCode::Char('/'));
    type_text(&mut app, "ripgrep");
    let text = screen(&mut app, 160, 40);
    let row = text.lines().find(|line| line.contains("Ripgrep")).unwrap();
    assert!(row.contains("Applications › "), "{row}");
    // A row that cannot be queued is marked as running on its own.
    let alone = app
        .catalog
        .entries()
        .into_iter()
        .find(|entry| !entry.multi_select)
        .unwrap()
        .clone();
    press(&mut app, KeyCode::Esc);
    app.rows = vec![Row::Entry(Box::new(alone.clone()))];
    let text = screen(&mut app, 160, 40);
    assert!(text.contains(&format!("[>]  {}", alone.name)), "{text}");
}

#[test]
fn details_say_where_an_entry_is_its_kind_and_its_flags() {
    let mut app = app();
    app.open_screen(Screen::Area(app.first_area()));
    app.focus = Focus::Main;
    press(&mut app, KeyCode::Char('/'));
    type_text(&mut app, "ripgrep");
    press(&mut app, KeyCode::Enter);
    press(&mut app, KeyCode::Char('d'));
    let text = screen(&mut app, 140, 45);
    assert!(text.contains("Applications › "), "{text}");
    assert!(text.contains("Package"), "{text}");
    assert!(text.contains("What it does:"), "{text}");
    assert!(text.contains("installation (privileged)"), "{text}");
    assert_eq!(
        flag_meanings("PI FM X"),
        vec![
            (
                "PI".to_string(),
                "installation (privileged), privileged".to_string()
            ),
            ("FM".to_string(), "file changes".to_string()),
            ("X".to_string(), "unknown flag".to_string()),
        ]
    );
}

#[test]
fn a_profile_shows_what_it_runs_and_e_queues_its_entries() {
    let mut app = app();
    let profile = app.catalog.find("developer-basics").unwrap().clone();
    let Action::Profile { entries } = &profile.action else {
        panic!("not a profile");
    };
    let entries = entries.clone();
    app.rows = vec![Row::Entry(Box::new(profile.clone()))];
    app.screen = Screen::Area(app.first_area());
    app.focus = Focus::Main;
    app.main_state.select(Some(0));
    press(&mut app, KeyCode::Char('d'));
    let text = screen(&mut app, 140, 45);
    assert!(text.contains("Runs, in order:"), "{text}");
    assert!(text.contains("1. Git") || text.contains("1. git"), "{text}");
    app.popup = None;
    // One entry already queued stays once.
    let git = app.catalog.find("git").unwrap().clone();
    app.queue.push(git);
    app.rows = vec![Row::Entry(Box::new(profile))];
    app.main_state.select(Some(0));
    press(&mut app, KeyCode::Char('e'));
    assert_eq!(app.screen, Screen::Plan);
    assert_eq!(queue_ids(&app), entries);
    assert!(screen(&mut app, 140, 40).contains("3 of its entries were added"));
    app.popup = None;
    app.open_screen(Screen::Area(app.first_area()));
    app.rows = vec![Row::Entry(Box::new(
        app.catalog.find("developer-basics").unwrap().clone(),
    ))];
    app.main_state.select(Some(0));
    app.focus = Focus::Main;
    press(&mut app, KeyCode::Char('e'));
    assert!(screen(&mut app, 140, 40).contains("Its entries are already in the Install Plan"));
}

#[test]
fn the_install_plan_moves_entries_and_says_what_each_is() {
    let mut app = app();
    for id in ["git", "ripgrep", "fzf"] {
        let entry = app.catalog.find(id).unwrap().clone();
        app.toggle_queued(entry);
    }
    app.open_screen(Screen::Plan);
    app.focus = Focus::Main;
    app.main_state.select(Some(0));
    press(&mut app, KeyCode::Char('J'));
    assert_eq!(queue_ids(&app), ["ripgrep", "git", "fzf"]);
    assert_eq!(app.main_state.selected(), Some(1));
    app.handle_key(&KeyEvent::new(KeyCode::Down, KeyModifiers::SHIFT));
    assert_eq!(queue_ids(&app), ["ripgrep", "fzf", "git"]);
    press(&mut app, KeyCode::Char('K'));
    press(&mut app, KeyCode::Char('K'));
    assert_eq!(queue_ids(&app), ["git", "ripgrep", "fzf"]);
    // At the top, it stays.
    press(&mut app, KeyCode::Char('K'));
    assert_eq!(queue_ids(&app), ["git", "ripgrep", "fzf"]);
    app.statuses.insert(
        "fzf".to_string(),
        Status::Absent {
            replaces: Vec::new(),
        },
    );
    let text = screen(&mut app, 140, 50);
    assert!(text.contains("Queued entries · 3"), "{text}");
    assert!(text.contains("1. Git") || text.contains("1. git"), "{text}");
    assert!(text.contains("Applications › "), "{text}");
    assert!(text.contains(" · Package"), "{text}");
    assert!(text.contains("Not installed"), "{text}");
    assert!(text.contains("K/J Move"), "{text}");
}

#[test]
fn screens_are_found_by_name_for_the_command_line() {
    let app = app();
    assert_eq!(app.screen_named("updates"), Some(Screen::Updates));
    assert_eq!(app.screen_named("Home-Manager"), Some(Screen::HomeManager));
    assert_eq!(
        app.screen_named("applications"),
        Some(Screen::Area(app.first_area()))
    );
    assert_eq!(app.screen_named("nowhere"), None);
    assert!(app.screen_names().contains("doctor, updates, drift"));
    assert!(
        app.screen_names().ends_with("profiles") || app.screen_names().contains("applications")
    );
    let mut app = app;
    app.open_on(Screen::Doctor);
    assert_eq!((app.screen, app.focus), (Screen::Doctor, Focus::Main));
}

#[test]
fn ctrl_k_finds_screens_actions_and_entries() {
    let mut app = app();
    let control_k = KeyEvent::new(KeyCode::Char('k'), KeyModifiers::CONTROL);
    app.handle_key(&control_k);
    assert!(matches!(app.popup, Some(Popup::Palette(_))));
    let text = screen(&mut app, 120, 40);
    assert!(text.contains("COMMANDS"), "{text}");
    assert!(text.contains("Check updates online"), "{text}");
    type_text(&mut app, "drift");
    press(&mut app, KeyCode::Enter);
    assert!(app.popup.is_none());
    assert_eq!(app.screen, Screen::Drift);
    assert_eq!(app.focus, Focus::Main);

    app.handle_key(&control_k);
    type_text(&mut app, "run ghostty");
    assert!(screen(&mut app, 120, 40).contains("Run Ghostty"));
    press(&mut app, KeyCode::Enter);
    assert!(matches!(app.popup, Some(Popup::Confirm(_))));
    press(&mut app, KeyCode::Char('n'));

    app.handle_key(&control_k);
    type_text(&mut app, "dns");
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.screen, Screen::Doctor);
    assert_eq!(app.doctor_wanted, Some("dns"));
    // The check's details open once the checks have run.
    app.diagnoses = vec![doctor::Diagnosis {
        id: "dns".into(),
        title: "Name resolution".into(),
        health: doctor::Health::Good,
        summary: "github.com resolves".into(),
        evidence: Vec::new(),
        advice: None,
        repair: None,
        entry: None,
    }];
    let wanted = app.doctor_wanted.take().unwrap();
    app.show_check(wanted);
    assert!(screen(&mut app, 120, 40).contains("github.com resolves"));

    // Escape closes the palette without doing anything; so does Ctrl-K.
    app.popup = None;
    app.handle_key(&control_k);
    press(&mut app, KeyCode::Esc);
    assert!(app.popup.is_none());
    app.handle_key(&control_k);
    app.handle_key(&control_k);
    assert!(app.popup.is_none());
}

#[test]
fn reliability_lists_critical_entries_first_and_explains_each() {
    let mut app = app();
    app.open_screen(Screen::Reliability);
    app.focus = Focus::Main;
    let text = screen(&mut app, 140, 40);
    // A release build carries the Distributions record; others say they have none.
    assert!(
        text.contains("No container record")
            || text.contains("Container record: Distributions run"),
        "{text}"
    );
    assert!(text.contains("Stand-in Tested"), "{text}");
    // The first rows are critical: boot, login, drivers, whole systems.
    assert!(app.reliability_rows[0].2.critical);
    assert!(text.contains("! "), "{text}");
    press(&mut app, KeyCode::Char('i'));
    assert_eq!(app.coverage, Coverage::Critical);
    assert!(app.reliability_rows.iter().all(|(_, _, row)| row.critical));
    press(&mut app, KeyCode::Char('i'));
    assert!(
        app.reliability_rows
            .iter()
            .all(|(_, _, row)| row.level >= reliability::Level::StandInTested)
    );
    press(&mut app, KeyCode::Char('i'));
    let sddm = app
        .reliability_rows
        .iter()
        .position(|(id, _, _)| id == "sddm")
        .unwrap();
    app.main_state.select(Some(sddm));
    press(&mut app, KeyCode::Enter);
    let text = screen(&mut app, 140, 40);
    assert!(text.contains("A critical entry"), "{text}");
    assert!(text.contains("test_sddm"), "{text}");
    press(&mut app, KeyCode::Esc);
    press(&mut app, KeyCode::Char('v'));
    let text = screen(&mut app, 140, 40);
    assert!(text.contains("Trust: SDDM Theme"), "{text}");
    assert!(text.contains("Pinned revision"), "{text}");
}

#[test]
fn v_inspects_an_entry_and_the_confirmation_warns_of_untested_boot_changes() {
    let mut app = app();
    press(&mut app, KeyCode::Char('/'));
    type_text(&mut app, "ripgrep");
    press(&mut app, KeyCode::Enter);
    assert!(screen(&mut app, 140, 40).contains("v Trust"));
    press(&mut app, KeyCode::Char('v'));
    let text = screen(&mut app, 140, 40);
    assert!(text.contains("Package integrity"), "{text}");
    assert!(text.contains("built from source"), "{text}");
    press(&mut app, KeyCode::Esc);
    app.popup = None;
    // This build's own claims without a record, so a release build's
    // record (where the theme may be VM Tested) does not change the test.
    let mut evidence = reliability::Evidence::load(app.catalog.root());
    evidence.results = None;
    app.evidence = Some(evidence);
    let grub = app.catalog.find("grub-dracula").unwrap().clone();
    app.confirm(vec![grub]);
    assert!(matches!(app.popup, Some(Popup::Confirm(_))));
    let text = screen(&mut app, 140, 40);
    assert!(text.contains("only tested with stand-ins"), "{text}");
}

#[test]
fn the_change_preview_opens_from_the_plan_the_confirmation_and_backups() {
    let mut app = app();
    app.queue
        .push(app.catalog.find("grub-dracula").unwrap().clone());
    app.open_screen(Screen::Plan);
    app.focus = Focus::Main;
    assert!(screen(&mut app, 140, 40).contains("e Changes"));
    press(&mut app, KeyCode::Char('e'));
    let text = screen(&mut app, 140, 40);
    assert!(text.contains("CHANGES"), "{text}");
    assert!(text.contains("Dracula GRUB Theme"), "{text}");
    assert!(text.contains("not known in advance"), "{text}");
    assert!(text.contains("grub-mkconfig"), "{text}");
    press(&mut app, KeyCode::Esc);
    assert!(app.popup.is_none());

    // From the confirmation, and back to it.
    press(&mut app, KeyCode::Enter);
    assert!(matches!(app.popup, Some(Popup::Confirm(_))));
    assert!(screen(&mut app, 140, 40).contains("[E] Changes"));
    press(&mut app, KeyCode::Char('e'));
    assert!(matches!(app.popup, Some(Popup::Effects(_))));
    assert!(screen(&mut app, 140, 40).contains("Back to the confirmation"));
    press(&mut app, KeyCode::Esc);
    assert!(matches!(app.popup, Some(Popup::Confirm(_))));
    assert!(app.pending.is_some());
    press(&mut app, KeyCode::Char('n'));

    // A backup's restore, side by side with the file there now.
    let directory = tempfile::tempdir().unwrap();
    let original = directory.path().join("app.conf");
    std::fs::write(&original, "color = red\nsize = 2\n").unwrap();
    let copy = directory
        .path()
        .join("app.conf.helix-toolbox-backup-20261016120000");
    std::fs::write(&copy, "color = blue\nsize = 2\n").unwrap();
    app.backups = vec![backups::Backup {
        backup: copy,
        original,
        taken: None,
        system: false,
        directory: false,
    }];
    app.screen = Screen::Backups;
    app.main_state.select(Some(0));
    press(&mut app, KeyCode::Char('e'));
    let text = screen(&mut app, 140, 40);
    assert!(text.contains("[side by side]"), "{text}");
    assert!(text.contains("Enter Side by side"), "{text}");
    press(&mut app, KeyCode::Enter);
    let text = screen(&mut app, 140, 40);
    assert!(text.contains("SIDE BY SIDE"), "{text}");
    assert!(text.contains("color = red"), "{text}");
    assert!(text.contains("color = blue"), "{text}");
    press(&mut app, KeyCode::Esc);
    assert!(screen(&mut app, 140, 40).contains("CHANGES"));
}

#[test]
fn hardware_lists_firmware_disks_and_batteries_and_asks_before_changing() {
    use helix_toolbox_engine::hardware::{Battery, Disk, Firmware, FirmwareUpdate, Hardware};
    let found = Hardware {
        firmware: Firmware {
            available: true,
            updates: vec![FirmwareUpdate {
                device_id: "9ea4da9853eae668f5bd65b10d14e4363f619406".into(),
                device: "Bolt Receiver".into(),
                from: Some("MPR05.04".into()),
                to: "MPR05.06".into(),
                summary: Some("Fixes pairing".into()),
                urgency: Some("medium".into()),
            }],
            ..Firmware::default()
        },
        disks: vec![Disk {
            name: "nvme0n1".into(),
            model: Some("Samsung SSD 990 PRO 1TB".into()),
            bytes: Some(1_000_204_886_016),
            kind: "NVMe".into(),
            firmware: Some("8B2QJXD7".into()),
            celsius: Some(38.0),
        }],
        batteries: vec![Battery {
            name: "BAT0".into(),
            capacity: Some(100),
            health: Some(65),
            cycles: Some(662),
            ..Battery::default()
        }],
        ..Hardware::default()
    };
    let mut app = app();
    app.hardware_rows = hardware_rows(&found);
    app.hardware = Some(found);
    app.open_screen(Screen::Hardware);
    app.focus = Focus::Main;
    let text = screen(&mut app, 140, 40);
    assert!(
        text.contains("Update for Bolt Receiver: MPR05.04 → MPR05.06"),
        "{text}"
    );
    assert!(
        text.contains("nvme0n1  NVMe  1.0 TB  Samsung SSD 990 PRO 1TB"),
        "{text}"
    );
    assert!(text.contains("holds 65% of its design capacity"), "{text}");
    let update = app
        .hardware_rows
        .iter()
        .position(|row| {
            matches!(
                row,
                HardwareRow::Item {
                    act: Some(HardwareAct::Firmware(_)),
                    ..
                }
            )
        })
        .unwrap();
    app.main_state.select(Some(update));
    assert!(screen(&mut app, 140, 40).contains("f Update firmware"));
    // A firmware update needs the word before its plan is even shown.
    press(&mut app, KeyCode::Char('f'));
    assert!(matches!(app.popup, Some(Popup::FirmwareWord(_))));
    type_text(&mut app, "yes");
    press(&mut app, KeyCode::Enter);
    assert!(screen(&mut app, 140, 40).contains("Not confirmed"));
    assert!(app.pending.is_none());
    app.popup = None;
    press(&mut app, KeyCode::Char('f'));
    type_text(&mut app, "FIRMWARE");
    press(&mut app, KeyCode::Enter);
    assert!(matches!(app.popup, Some(Popup::Confirm(_))));
    let text = screen(&mut app, 140, 40);
    assert!(text.contains("Risk: high"), "{text}");
    assert!(text.contains("sudo fwupdmgr update 9ea4da98"), "{text}");
    press(&mut app, KeyCode::Char('n'));
    let disk = app
        .hardware_rows
        .iter()
        .position(|row| {
            matches!(
                row,
                HardwareRow::Item {
                    act: Some(HardwareAct::Smart(_)),
                    ..
                }
            )
        })
        .unwrap();
    app.main_state.select(Some(disk));
    press(&mut app, KeyCode::Char('s'));
    assert!(screen(&mut app, 140, 40).contains("sudo smartctl --health --attributes /dev/nvme0n1"));
}

#[test]
fn the_wizard_suggests_a_workstation_and_queues_it_for_review() {
    let directory = tempfile::tempdir().unwrap();
    let mut app = app();
    app.settings_path = Some(directory.path().join("settings.toml"));
    app.set_up = false;
    assert!(app.first_run());
    app.open_wizard(true);
    let text = screen(&mut app, 140, 40);
    assert!(text.contains("SET UP THIS MACHINE"), "{text}");
    assert!(text.contains("Welcome to Helix Toolbox"), "{text}");
    assert!(text.contains("Garuda Linux"), "{text}");
    press(&mut app, KeyCode::Enter);
    let text = screen(&mut app, 140, 40);
    assert!(text.contains("Developer"), "{text}");
    assert!(text.contains("Privacy"), "{text}");
    press(&mut app, KeyCode::Enter);
    let text = screen(&mut app, 140, 40);
    assert!(text.contains("WHAT IT ADDS"), "{text}");
    assert!(text.contains("[x] Git"), "{text}");
    // Untick the first, then queue the rest.
    press(&mut app, KeyCode::Char(' '));
    assert!(screen(&mut app, 140, 40).contains("[ ] Git"));
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.screen, Screen::Plan);
    assert!(!app.queue.iter().any(|entry| entry.id == "git"));
    assert!(app.queue.iter().any(|entry| entry.id == "ripgrep"));
    assert!(screen(&mut app, 140, 40).contains("Nothing has run"));
    // Offered once: it is remembered.
    assert!(!app.first_run());
    assert!(settings::load(&directory.path().join("settings.toml")).set_up);
    // Custom starts with nothing ticked.
    app.popup = None;
    app.open_wizard(false);
    press(&mut app, KeyCode::Enter);
    for _ in 0..4 {
        press(&mut app, KeyCode::Char('j'));
    }
    press(&mut app, KeyCode::Enter);
    let text = screen(&mut app, 140, 40);
    assert!(text.contains("Custom"), "{text}");
    assert!(!text.contains("[x]"), "{text}");
    press(&mut app, KeyCode::Esc);
    press(&mut app, KeyCode::Esc);
    press(&mut app, KeyCode::Esc);
    assert!(app.popup.is_none());
}

#[test]
fn the_desktop_screen_offers_its_entries_and_puts_a_saved_look_back() {
    use helix_toolbox_engine::desktop::{Desktop, Kind, Value};
    let directory = tempfile::tempdir().unwrap();
    let mut app = app();
    app.looks_dir = Some(directory.path().to_path_buf());
    app.desktop = Some(Desktop {
        kind: Some(Kind::Kde),
        name: Some("KDE Plasma 6.8.0".into()),
        session: Some("wayland".into()),
        appearance: vec![Value {
            key: "color-scheme".into(),
            label: "Color scheme".into(),
            value: Some("BreezeDark".into()),
        }],
        owners: vec!["Home Manager runs a Plasma configuration step at every switch".into()],
        ..Desktop::default()
    });
    app.open_screen(Screen::Desktop);
    app.focus = Focus::Main;
    let text = screen(&mut app, 140, 40);
    assert!(text.contains("KDE Plasma 6.8.0, wayland session"), "{text}");
    assert!(text.contains("Color scheme    BreezeDark"), "{text}");
    assert!(text.contains("Note: Home Manager"), "{text}");
    assert!(text.contains("Commander Konsole Config"), "{text}");
    press(&mut app, KeyCode::Char('s'));
    type_text(&mut app, "Dark");
    press(&mut app, KeyCode::Enter);
    assert!(screen(&mut app, 140, 40).contains("Look saved"));
    app.popup = None;
    // The look now differs from the desktop: applying it plans the change.
    let mut changed = app.desktop.clone().unwrap();
    changed.appearance[0].value = Some("Sweet".into());
    app.desktop = Some(changed);
    app.reload_desktop();
    let look = app
        .desktop_rows
        .iter()
        .position(|row| matches!(row, DesktopRow::Look { .. }))
        .unwrap();
    app.main_state.select(Some(look));
    press(&mut app, KeyCode::Enter);
    let text = screen(&mut app, 140, 40);
    assert!(
        text.contains("plasma-apply-colorscheme BreezeDark"),
        "{text}"
    );
    press(&mut app, KeyCode::Char('n'));
    let entry = app
        .desktop_rows
        .iter()
        .position(|row| matches!(row, DesktopRow::Entry { id, .. } if id == "plymouth-on"))
        .unwrap();
    app.main_state.select(Some(entry));
    press(&mut app, KeyCode::Enter);
    assert!(matches!(app.popup, Some(Popup::Confirm(_))));
}

#[test]
fn the_sidebar_keeps_every_screen_reachable_in_a_small_terminal() {
    let mut app = app();
    let text = screen(&mut app, 100, 25);
    assert!(text.contains("Dashboard"), "{text}");
    assert!(text.contains("MORE TOOLS"), "{text}");
    app.open_screen(Screen::Settings);
    let text = screen(&mut app, 100, 25);
    assert!(text.contains("Settings"), "{text}");
    assert!(text.contains("Backups"), "{text}");
    // With room, the main menu keeps its cards.
    app.open_screen(Screen::Dashboard);
    assert!(screen(&mut app, 120, 40).contains("╭"));
}

#[test]
fn timestamps_and_escape_stripping_are_sane() {
    assert_eq!(timestamp(0), "1970-01-01 00:00 UTC");
    assert_eq!(timestamp(1_700_000_000), "2023-11-14 22:13 UTC");
    assert_eq!(strip_escapes("a\u{1b}[31mb\u{1b}[0m\r\n"), "ab\n");
}

/// Home Manager as the tests' screens see it: a flake, two generations
/// and a configuration file it links.
fn home_status() -> home_manager::Status {
    home_manager::Status {
        installed: true,
        version: Some("26.11-pre".into()),
        config: Some(home_manager::Config {
            folder: "/home/me/dotfiles/home-manager".into(),
            flake: true,
            profile: Some("me".into()),
            origin: home_manager::Origin::Settings,
        }),
        problem: None,
        generations: vec![
            home_manager::Generation {
                id: 201,
                built: "2026-10-20 12:01".into(),
                path: "/nix/store/aaa-home-manager-generation".into(),
                current: true,
            },
            home_manager::Generation {
                id: 200,
                built: "2026-10-16 13:23".into(),
                path: "/nix/store/no-such-home-manager-generation".into(),
                current: false,
            },
        ],
        repository: Some(Summary {
            name: "dotfiles".into(),
            path: "/home/me/dotfiles".into(),
            branch: Some("main".into()),
            changed: 2,
            ahead: Some(1),
            behind: Some(0),
            ..Summary::default()
        }),
        links: vec![home_manager::Link {
            name: "Ghostty".into(),
            path: "/home/me/.config/ghostty/config".into(),
            target: Some("/nix/store/xyz-home-manager-files/.config/ghostty/config".into()),
            managed: true,
        }],
    }
}

#[test]
fn the_home_manager_screen_shows_generations_its_repository_and_links() {
    let mut app = app();
    app.home_status = Some(home_status());
    app.open_screen(Screen::HomeManager);
    app.focus = Focus::Main;
    let shown = screen(&mut app, 140, 40);
    for text in [
        "Home Manager 26.11-pre",
        "Configuration: /home/me/dotfiles/home-manager#me (a flake, from Helix Toolbox's settings)",
        "Repository /home/me/dotfiles: main, 2 changed, 1 ahead and 0 behind",
        "Generations (2)",
        "201  2026-10-20 12:01  active",
        "Ghostty",
        "linked by Home Manager",
        "b Build",
        "p Pull",
    ] {
        assert!(shown.contains(text), "{text}: {shown}");
    }
    // d tells of the linked file.
    let link = app
        .home_rows
        .iter()
        .position(|row| matches!(row, HomeRow::Link(0)))
        .unwrap();
    app.main_state.select(Some(link));
    press(&mut app, KeyCode::Char('d'));
    assert!(screen(&mut app, 140, 40).contains("Home Manager manages it"));
    // Not installed: building says so instead of planning.
    press(&mut app, KeyCode::Esc);
    app.home_status = Some(home_manager::Status::default());
    app.build_home_rows();
    press(&mut app, KeyCode::Char('b'));
    assert!(screen(&mut app, 140, 40).contains("home-manager is not installed here"));
}

#[test]
fn the_home_manager_screen_reviews_build_switch_and_going_back() {
    let mut app = app();
    app.home_status = Some(home_status());
    app.open_screen(Screen::HomeManager);
    app.focus = Focus::Main;
    press(&mut app, KeyCode::Char('b'));
    assert!(matches!(app.popup, Some(Popup::Confirm(_))));
    let shown = screen(&mut app, 160, 40);
    assert!(
        shown.contains("home-manager build --flake '/home/me/dotfiles/home-manager#me'"),
        "{shown}"
    );
    press(&mut app, KeyCode::Char('n'));
    assert!(!matches!(app.popup, Some(Popup::Running(_))));
    press(&mut app, KeyCode::Char('s'));
    assert!(matches!(app.popup, Some(Popup::Confirm(_))));
    assert!(screen(&mut app, 160, 40).contains("home-manager switch --flake"));
    press(&mut app, KeyCode::Char('n'));
    // The earlier generation was collected: there is no script to run.
    let earlier = app
        .home_rows
        .iter()
        .position(|row| matches!(row, HomeRow::Generation(1)))
        .unwrap();
    app.main_state.select(Some(earlier));
    press(&mut app, KeyCode::Enter);
    assert!(screen(&mut app, 160, 40).contains("has no activate script"));
    // Enter on the active one only tells of it.
    press(&mut app, KeyCode::Esc);
    app.main_state.select(Some(earlier - 1));
    press(&mut app, KeyCode::Enter);
    assert!(screen(&mut app, 160, 40).contains("It is the active one."));
}

#[test]
fn the_doctor_counts_its_checks_shows_repair_commands_and_its_reports_are_entries() {
    let mut app = app();
    let check = |id: &str, health: doctor::Health| doctor::Diagnosis {
        id: id.into(),
        title: id.into(),
        health,
        summary: "found".into(),
        evidence: Vec::new(),
        advice: None,
        repair: None,
        entry: None,
    };
    app.diagnoses = vec![
        check("one", doctor::Health::Problem),
        check("two", doctor::Health::Notice),
        check("three", doctor::Health::Good),
        check("four", doctor::Health::Good),
    ];
    app.diagnoses[0].repair = Some(doctor::Repair {
        label: "fix it".into(),
        commands: vec![vec!["sudo".into(), "true".into()]],
    });
    app.open_screen(Screen::Doctor);
    app.focus = Focus::Main;
    let shown = screen(&mut app, 160, 40);
    assert!(
        shown.contains("Checks · 1 problem · 1 notice · 2 OK"),
        "{shown}"
    );
    // The repair's commands are shown before f runs them.
    press(&mut app, KeyCode::Char('d'));
    let shown = screen(&mut app, 160, 40);
    assert!(
        shown.contains("Repair: fix it.") && shown.contains("sudo true"),
        "{shown}"
    );
    app.popup = None;
    // A report row queues, pins and describes like any entry.
    app.main_state.select(Some(app.diagnoses.len()));
    let report = app.selected_entry().cloned().expect("a report entry");
    press(&mut app, KeyCode::Char('d'));
    assert!(matches!(app.popup, Some(Popup::Text(_))));
    app.popup = None;
    press(&mut app, KeyCode::Char('*'));
    assert!(app.favorites.contains(&report.id));
    if report.multi_select {
        press(&mut app, KeyCode::Char(' '));
        assert!(app.is_queued(&report));
        assert!(screen(&mut app, 160, 40).contains(&format!("{} *", report.name)));
    }
}

#[test]
fn the_updates_screen_lists_entries_with_an_update_below_the_channels() {
    let mut app = app();
    app.channels = vec![channel("pacman", "Arch packages", Some(Vec::new()))];
    let inventory = Inventory::from_parts(
        Some(helix_toolbox_core::PackageManager::Arch),
        Some([("ripgrep".to_string(), "15.2.0-1".to_string())].into()),
        [("ripgrep".to_string(), "15.3.0-1".to_string())].into(),
        HashMap::new(),
        Default::default(),
    );
    let ripgrep = app.catalog.find("ripgrep").unwrap().clone();
    let status = inventory
        .status(&ripgrep, &app.platform, &|_| None)
        .unwrap();
    app.statuses.insert("ripgrep".into(), status);
    app.inventory = Some(inventory);
    app.open_screen(Screen::Updates);
    app.focus = Focus::Main;
    assert_eq!(app.main_len(), 3, "the channel, the entry and update all");
    let shown = screen(&mut app, 140, 40);
    assert!(shown.contains("Entry   Ripgrep"), "{shown}");
    assert!(shown.contains("Update every entry (1)"), "{shown}");
    app.main_state.select(Some(1));
    assert!(screen(&mut app, 140, 40).contains("Update it"));
    press(&mut app, KeyCode::Enter);
    assert!(matches!(app.popup, Some(Popup::Confirm(_))));
    assert!(screen(&mut app, 140, 40).contains("sudo pacman -Syu"));
    press(&mut app, KeyCode::Char('n'));
    app.popup = None;
    app.main_state.select(Some(2));
    press(&mut app, KeyCode::Enter);
    assert!(matches!(app.popup, Some(Popup::Confirm(_))));
}

#[test]
fn drift_shows_was_and_now_what_is_not_compared_and_runs_the_missing_after_review() {
    let mut app = app();
    // Opening the screen reads the baselines; the comparison is set after.
    app.open_screen(Screen::Drift);
    app.focus = Focus::Main;
    app.drift_targets = vec![DriftTarget::Profile {
        id: "profile:work".into(),
        name: "Work".into(),
        entries: vec!["ripgrep".into(), "gone-entry".into()],
    }];
    app.drift_target = 0;
    app.drift_report = Some((
        "profile:work".into(),
        drift::Report {
            against: "Work".into(),
            differences: vec![drift::Difference {
                kind: drift::Kind::Missing,
                subject: "ripgrep".into(),
                was: Some("15.2.0".into()),
                now: None,
                entry: Some("ripgrep".into()),
                note: "not installed".into(),
            }],
            matched: 3,
            unknown: vec!["gone-entry".into()],
        },
    ));
    app.main_state.select(Some(0));
    let shown = screen(&mut app, 160, 40);
    assert!(shown.contains("15.2.0 → –"), "{shown}");
    assert!(shown.contains("Not compared: gone-entry"), "{shown}");
    assert!(shown.contains("R Run them now"), "{shown}");
    // Reviewed like any run; where it is installed already, there is
    // nothing to run.
    press(&mut app, KeyCode::Char('R'));
    assert!(
        matches!(app.popup, Some(Popup::Confirm(_)))
            || screen(&mut app, 160, 40).contains("Nothing to run"),
        "{}",
        screen(&mut app, 160, 40)
    );
    press(&mut app, KeyCode::Char('n'));
    assert!(!matches!(app.popup, Some(Popup::Running(_))));
}

#[test]
fn reliability_says_what_of_the_record_could_not_be_read() {
    let mut app = app();
    let mut evidence = reliability::Evidence::default();
    evidence
        .problems
        .push("validation.toml cannot be read: broken".into());
    app.evidence = Some(evidence);
    app.open_screen(Screen::Reliability);
    let shown = screen(&mut app, 160, 40);
    assert!(
        shown.contains("⚠ validation.toml cannot be read: broken"),
        "{shown}"
    );
}

#[test]
fn a_login_application_is_described_before_it_is_switched() {
    let mut app = app();
    let mut found = stand_in_services();
    found.autostart[0].comment = "Updates in the background".into();
    found.autostart[0].own = Some("/home/me/.config/autostart/discover.desktop".into());
    app.services_found = Some(found);
    app.open_screen(Screen::Services);
    app.focus = Focus::Main;
    // The view of the applications started at login.
    for _ in 0..3 {
        press(&mut app, KeyCode::Char('i'));
    }
    let row = app
        .service_rows
        .iter()
        .position(|row| matches!(row, ServiceRow::Autostart(_)))
        .unwrap();
    app.main_state.select(Some(row));
    press(&mut app, KeyCode::Char('d'));
    let shown = screen(&mut app, 160, 40);
    for text in [
        "Updates in the background",
        "Runs: discover --notifier",
        "From: the system · your file overrides it",
        "At login: started",
        "For you only",
    ] {
        assert!(shown.contains(text), "{text}: {shown}");
    }
}

#[test]
fn migration_says_when_packages_cannot_be_told_apart() {
    let mut app = app();
    app.migration_here = Some(migration::Migration {
        version: migration::VERSION,
        from: "Solus".into(),
        ..Default::default()
    });
    app.migration_packages_known = false;
    app.open_screen(Screen::Migration);
    let shown = screen(&mut app, 200, 40);
    assert!(
        shown.contains("The packages you installed yourself cannot be told apart here"),
        "{shown}"
    );
}

#[test]
fn restic_restores_what_is_typed_shows_what_it_leaves_out_and_ignores_case() {
    let mut app = restic_app(Vec::new());
    if let Some(ResticReading {
        found: restic::Found::Ready(config),
        ..
    }) = &mut app.restic_reading
    {
        config.excludes = vec!["/home/me/.cache".into()];
    }
    app.open_screen(Screen::Restic);
    app.focus = Focus::Main;
    let shown = screen(&mut app, 160, 40);
    assert!(shown.contains("Leaves out /home/me/.cache"), "{shown}");
    press(&mut app, KeyCode::Char('F'));
    assert!(matches!(
        app.popup,
        Some(Popup::Ask(_, Asked::ResticRestoreFrom))
    ));
    type_text(&mut app, " /home/me/Documents");
    press(&mut app, KeyCode::Enter);
    assert!(
        matches!(
            &app.popup,
            Some(Popup::Ask(_, Asked::ResticRestore(snapshot, Some(include))))
                if snapshot == "latest" && include == "/home/me/Documents"
        ),
        "the snapshot, then the path"
    );
    app.popup = None;
    press(&mut app, KeyCode::Char('s'));
    answer(&mut app, "NOTES.txt");
    press(&mut app, KeyCode::Char('i'));
    assert!(app.restic_ignore_case);
    assert!(screen(&mut app, 160, 40).contains("ignoring upper and lower case"));
}

#[test]
fn a_task_of_yours_can_name_its_shell() {
    assert_eq!(
        task_shell(" fish: npm run dev "),
        (workspaces::Shell::Fish, "npm run dev".to_string())
    );
    assert_eq!(
        task_shell("bash:make"),
        (workspaces::Shell::Bash, "make".to_string())
    );
    assert_eq!(
        task_shell("sh -c 'echo hi'"),
        (workspaces::Shell::Direct, "sh -c 'echo hi'".to_string())
    );
    assert_eq!(
        task_shell("fish:"),
        (workspaces::Shell::Direct, "fish:".to_string()),
        "a shell with no command is the command"
    );
}

#[test]
fn a_configuration_draft_is_checked_and_discarded() {
    let directory = tempfile::tempdir().unwrap();
    let home = directory.path().canonicalize().unwrap();
    let mut app = configs_app(&home);
    app.open_screen(Screen::Configs);
    app.focus = Focus::Main;
    app.set_config(configs::Kind::Ghostty, "font-size", "13");
    let shown = screen(&mut app, 160, 50);
    assert!(shown.contains("Draft checks: not checked yet"), "{shown}");
    let field = app
        .config_rows
        .iter()
        .position(|row| matches!(row, ConfigRow::Field(configs::Kind::Ghostty, 1)))
        .unwrap();
    app.main_state.select(Some(field));
    press(&mut app, KeyCode::Char('c'));
    assert!(
        !screen(&mut app, 160, 50).contains("Draft checks: not checked yet"),
        "it was checked"
    );
    press(&mut app, KeyCode::Char('x'));
    let shown = screen(&mut app, 160, 50);
    assert!(!shown.contains("Draft:"), "{shown}");
    assert_eq!(
        std::fs::read_to_string(home.join(".config/ghostty/config")).unwrap(),
        "font-size = 11\n"
    );
}

#[test]
fn settings_say_where_they_are_kept() {
    let mut app = app();
    app.settings_path = Some("/home/me/.config/helix-toolbox/settings.toml".into());
    app.open_screen(Screen::Settings);
    let shown = screen(&mut app, 160, 40);
    assert!(
        shown.contains("Kept in /home/me/.config/helix-toolbox/settings.toml"),
        "{shown}"
    );
}
