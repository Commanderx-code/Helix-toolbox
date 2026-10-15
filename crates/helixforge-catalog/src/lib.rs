//! Trusted, bundled TOML catalog. External executables/plugins are intentionally unsupported.

use helixforge_core::{Tool, valid_package_name};
use std::collections::HashSet;
use thiserror::Error;

const BUILTIN: &[&str] = &[
    include_str!("../../../catalog/applications/git.toml"),
    include_str!("../../../catalog/applications/ripgrep.toml"),
    include_str!("../../../catalog/applications/fzf.toml"),
    include_str!("../../../catalog/applications/neovim.toml"),
    include_str!("../../../catalog/applications/htop.toml"),
    include_str!("../../../catalog/applications/btop.toml"),
];

#[derive(Debug, Error)]
pub enum CatalogError {
    #[error("invalid TOML: {0}")]
    Toml(#[from] toml::de::Error),
    #[error("invalid tool entry: {0}")]
    Invalid(String),
}

#[derive(Debug, Clone)]
pub struct Catalog {
    tools: Vec<Tool>,
}

impl Catalog {
    pub fn builtin() -> Result<Self, CatalogError> {
        Self::parse_many(BUILTIN)
    }

    pub fn parse_many(items: &[&str]) -> Result<Self, CatalogError> {
        let mut seen = HashSet::new();
        let mut tools = Vec::new();
        for text in items {
            let tool: Tool = toml::from_str(text)?;
            validate(&tool)?;
            if !seen.insert(tool.id.clone()) {
                return Err(CatalogError::Invalid(format!("duplicate id {}", tool.id)));
            }
            tools.push(tool);
        }
        tools.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(Self { tools })
    }

    pub fn all(&self) -> &[Tool] {
        &self.tools
    }

    pub fn find(&self, id: &str) -> Option<&Tool> {
        self.tools.iter().find(|tool| tool.id == id)
    }
}

fn validate(tool: &Tool) -> Result<(), CatalogError> {
    if !valid_package_name(&tool.id)
        || tool.name.trim().is_empty()
        || tool.description.trim().is_empty()
        || tool.category.trim().is_empty()
        || !valid_package_name(&tool.detect.binary)
        || tool.install.kind != "package"
    {
        return Err(CatalogError::Invalid(format!("bad entry: {}", tool.id)));
    }
    let entries = [
        tool.install.packages.arch.as_deref(),
        tool.install.packages.debian.as_deref(),
        tool.install.packages.fedora.as_deref(),
    ];
    if entries.iter().all(Option::is_none)
        || entries.into_iter().flatten().any(|name| !valid_package_name(name))
    {
        return Err(CatalogError::Invalid(format!(
            "invalid package mapping: {}",
            tool.id
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_is_valid_and_sorted() {
        let c = Catalog::builtin().unwrap();
        assert!(c.all().len() >= 6);
        assert!(c.find("git").is_some());
        assert!(c.all().windows(2).all(|w| w[0].name <= w[1].name));
    }

    #[test]
    fn duplicate_is_rejected() {
        assert!(Catalog::parse_many(&[BUILTIN[0], BUILTIN[0]]).is_err());
    }
}
