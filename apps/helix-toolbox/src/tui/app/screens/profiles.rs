//! Workstation profiles.

use super::super::*;

impl App {
    /// Save the queue as a profile and show it in the Profiles area.
    pub(in crate::tui::app) fn save_profile(&mut self, name: &str) {
        let profile = profiles::from_queue(name, &self.queue);
        let directory = self.profiles_dir.clone().or_else(profiles::dir);
        let saved = directory
            .as_deref()
            .ok_or_else(|| "cannot find the configuration folder".to_string())
            .and_then(|directory| {
                profiles::save(directory, &profile, &self.catalog)
                    .map_err(|error| error.to_string())
            });
        self.popup = Some(Popup::Text(match saved {
            Ok(path) => {
                if let Some(directory) = &directory {
                    profiles::add_to(&mut self.catalog, directory);
                    self.menu = build_menu(&self.catalog);
                    self.update_rows();
                }
                TextPopup::new(
                    "Profile saved",
                    format!(
                        "{} ({} entries) is saved as {}.\n\nIt is under Profiles, Your profiles: Enter runs it, Space queues it, x exports it to a file to share.",
                        profile.name,
                        profile.entries.len(),
                        path.display(),
                    ),
                    true,
                )
            }
            Err(error) => TextPopup::new("Profile not saved", error, true),
        }));
    }

    /// Whether the Profiles area is open.
    pub(in crate::tui::app) fn in_profiles(&self) -> bool {
        self.current_area()
            .is_some_and(|index| self.catalog.tabs[index].id == "profiles")
    }

    fn profiles_directory(&self) -> Option<PathBuf> {
        self.profiles_dir.clone().or_else(profiles::dir)
    }

    /// Read the user's profiles into the catalog again. The Profiles area
    /// stays in the group it was in, wherever that group moved, or goes
    /// back to the top when the group is gone.
    fn refresh_profiles(&mut self, directory: &Path) {
        let group_id = |catalog: &Catalog, index: usize, group: usize| match catalog.tabs[index]
            .items
            .get(group)
        {
            Some(Item::Group(group)) => Some(group.id.clone()),
            _ => None,
        };
        let area = self
            .catalog
            .tabs
            .iter()
            .position(|tab| tab.id == "profiles");
        let inside = area.and_then(|index| {
            let (group, _) = *self.areas[index].path.first()?;
            group_id(&self.catalog, index, group)
        });
        profiles::add_to(&mut self.catalog, directory);
        self.menu = build_menu(&self.catalog);
        if let (Some(index), Some(inside)) = (area, inside) {
            let moved = (0..self.catalog.tabs[index].items.len())
                .find(|&group| group_id(&self.catalog, index, group).as_deref() == Some(&inside));
            match moved {
                Some(group) => self.areas[index].path[0].0 = group,
                None => {
                    self.areas[index].path.clear();
                    self.main_state.select(Some(0));
                }
            }
        }
        self.update_rows();
    }

    /// One of the user's profiles by its catalog id: its file and what it
    /// holds.
    fn read_own_profile(&self, id: &str) -> Result<(PathBuf, profiles::Profile), String> {
        let directory = self
            .profiles_directory()
            .ok_or("cannot find the configuration folder")?;
        let slug = id
            .strip_prefix(profiles::PREFIX)
            .and_then(profiles::slug)
            .filter(|slug| id == format!("{}{slug}", profiles::PREFIX))
            .ok_or_else(|| {
                format!(
                    "{} is not one of your profiles",
                    helix_toolbox_core::printable(id)
                )
            })?;
        let path = directory.join(format!("{slug}.toml"));
        let profile = profiles::read(&path, &self.catalog).map_err(|error| error.to_string())?;
        Ok((path, profile))
    }

    /// Add a profile file to the user's profiles, as `helix-toolbox profile
    /// import` does.
    pub(in crate::tui::app) fn import_profile(&mut self, text: &str) -> Result<String, String> {
        let path = typed_path(text)?;
        let profile = profiles::read(&path, &self.catalog).map_err(|error| error.to_string())?;
        let directory = self
            .profiles_directory()
            .ok_or("cannot find the configuration folder")?;
        let saved = profiles::save(&directory, &profile, &self.catalog)
            .map_err(|error| error.to_string())?;
        self.refresh_profiles(&directory);
        Ok(format!(
            "{} ({} entries) is imported as {}.\n\nIt is under Profiles, Your profiles: Enter runs it, Space queues it.",
            helix_toolbox_core::printable(&profile.name),
            profile.entries.len(),
            saved.display()
        ))
    }

    /// Write one of the user's profiles to a file, as `helix-toolbox profile
    /// export` does.
    pub(in crate::tui::app) fn export_profile(
        &mut self,
        id: &str,
        text: &str,
    ) -> Result<String, String> {
        let (_, profile) = self.read_own_profile(id)?;
        let path = typed_path(text)?;
        if path.extension().is_none_or(|extension| extension != "toml") {
            return Err("A profile is written to a .toml file.".to_string());
        }
        profiles::write(&path, &profile).map_err(|error| error.to_string())?;
        Ok(format!(
            "{} is exported to {}.\n\nOn another machine: o in Profiles imports it, or helix-toolbox profile import.",
            helix_toolbox_core::printable(&profile.name),
            path.display()
        ))
    }

    /// Delete one of the user's profiles, as `helix-toolbox profile delete`
    /// does; it leaves the Install Plan and the favorites too.
    pub(in crate::tui::app) fn delete_profile(&mut self, id: &str) -> Result<String, String> {
        let (path, profile) = self.read_own_profile(id)?;
        std::fs::remove_file(&path)
            .map_err(|error| format!("cannot delete {}: {error}", path.display()))?;
        self.queue.retain(|entry| entry.id != id);
        if let Some(directory) = self.profiles_directory() {
            self.refresh_profiles(&directory);
        }
        Ok(format!(
            "{} is deleted.",
            helix_toolbox_core::printable(&profile.name)
        ))
    }
}
