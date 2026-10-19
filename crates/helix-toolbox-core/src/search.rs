//! Fuzzy search over entries: how well a query matches an entry's name, id
//! and description, so results can be ranked with the best match first.

/// A score for how well `query` matches, higher is better; None when it does
/// not match at all. Case does not matter.
pub fn score(query: &str, name: &str, id: &str, description: &str) -> Option<u32> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return Some(0);
    }
    let name = name.to_lowercase();
    let id = id.to_lowercase();
    let description = description.to_lowercase();
    let words = |text: &str| {
        text.split(|character: char| !character.is_alphanumeric())
            .filter(|word| !word.is_empty())
            .map(String::from)
            .collect::<Vec<_>>()
    };
    if name == query {
        return Some(1000);
    }
    if name.starts_with(&query) {
        return Some(900);
    }
    if words(&name).iter().any(|word| word.starts_with(&query)) {
        return Some(800);
    }
    if name.contains(&query) {
        return Some(700);
    }
    if id.contains(&query) {
        return Some(650);
    }
    // Several words, each found somewhere: "nvidia driver".
    let parts = words(&query);
    if parts.len() > 1 {
        let in_name = parts.iter().all(|part| name.contains(part.as_str()));
        let anywhere = parts.iter().all(|part| {
            name.contains(part.as_str())
                || id.contains(part.as_str())
                || description.contains(part.as_str())
        });
        if in_name {
            return Some(600);
        }
        if anywhere {
            return Some(500);
        }
    }
    // Letters in order, as in "vsc" for VS Code or "rgp" for ripgrep; the
    // closer together, the better.
    let compact: String = query
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect();
    // Letters scattered across a long name are a coincidence, not a match.
    if let Some((first, spread)) = subsequence(&compact, &name)
        && spread <= compact.chars().count() * 2
    {
        // Starting where the name or one of its words starts counts for more.
        let starts_word = first == 0
            || name
                .chars()
                .nth(first - 1)
                .is_some_and(|before| !before.is_alphanumeric());
        let bonus = if first == 0 { 50 } else { 0 } + if starts_word { 30 } else { 0 };
        return Some(400u32.saturating_sub(spread as u32 * 10) + bonus);
    }
    if description.contains(&query) {
        return Some(90);
    }
    None
}

/// Where `needle`'s characters start in `haystack`, in order, and how spread
/// out they are; None when they are not all there.
fn subsequence(needle: &str, haystack: &str) -> Option<(usize, usize)> {
    let mut positions = Vec::new();
    let mut from = 0;
    let haystack: Vec<char> = haystack.chars().collect();
    for wanted in needle.chars() {
        let found = haystack[from..]
            .iter()
            .position(|&character| character == wanted)?;
        positions.push(from + found);
        from += found + 1;
    }
    let first = *positions.first()?;
    let last = *positions.last()?;
    Some((first, last - first + 1 - positions.len()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rank<'a>(query: &str, names: &[(&'a str, &'a str)]) -> Vec<&'a str> {
        let mut scored: Vec<(u32, &str)> = names
            .iter()
            .filter_map(|(name, description)| Some((score(query, name, "", description)?, *name)))
            .collect();
        scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(b.1)));
        scored.into_iter().map(|(_, name)| name).collect()
    }

    #[test]
    fn better_matches_come_first() {
        let names = [
            ("Ripgrep", "Fast recursive search"),
            ("Neovim with Titus Config", "Neovim, ripgrep and more"),
            ("VS Code", "Visual Studio Code"),
            ("VS Codium", "Code without telemetry"),
            ("Nvidia Drivers & Hardware Acceleration", "NVIDIA's driver"),
            ("General GPU Drivers", "Drivers for any card"),
        ];
        assert_eq!(
            rank("ripgrep", &names),
            ["Ripgrep", "Neovim with Titus Config"]
        );
        assert_eq!(rank("code", &names)[0], "VS Code");
        assert_eq!(rank("vsc", &names), ["VS Code", "VS Codium"]);
        assert_eq!(rank("rgp", &names)[0], "Ripgrep");
        assert_eq!(
            rank("nvidia driver", &names),
            ["Nvidia Drivers & Hardware Acceleration"]
        );
        assert_eq!(rank("drivers", &names).len(), 2);
        assert!(rank("zzz", &names).is_empty());
        assert_eq!(score("  ", "Anything", "", ""), Some(0));
        assert_eq!(score("RIP", "ripgrep", "", ""), Some(900));
        assert_eq!(
            score("titus", "Neovim with Titus Config", "", ""),
            Some(800)
        );
    }

    #[test]
    fn ids_and_descriptions_count_less_than_names() {
        assert_eq!(
            score("pupgui", "ProtonUp-Qt", "net-davidotek-pupgui2", ""),
            Some(650)
        );
        assert!(score("telemetry", "VS Codium", "", "Code without telemetry").unwrap() < 100);
    }
}
