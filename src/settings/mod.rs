mod catalog_account;
mod catalog_coding;
mod catalog_personal;
mod spec;
mod view;

pub use spec::{ControlSpec, PageKind, PageSpec, RowSpec, SectionSpec};
pub use view::{ChangeTheme, CloseSettings, ConfigSaveFinished, SettingsView};

pub fn pages() -> impl Iterator<Item = &'static PageSpec> {
    catalog_personal::PAGES
        .iter()
        .chain(catalog_account::PAGES.iter())
        .chain(catalog_coding::PAGES.iter())
}

pub fn page(slug: &str) -> Option<&'static PageSpec> {
    pages().find(|page| page.slug == slug)
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::pages;

    #[test]
    fn catalog_contains_all_twenty_one_reference_pages_in_navigation_order() {
        let actual: Vec<_> = pages().map(|page| page.slug).collect();
        assert_eq!(
            actual,
            [
                "general-settings",
                "import",
                "profile",
                "appearance",
                "voice",
                "agent",
                "personalization",
                "pets",
                "keyboard-shortcuts",
                "usage",
                "computer-use",
                "chronicle",
                "appshots",
                "plugins-settings",
                "browser-use",
                "hooks-settings",
                "connections",
                "git-settings",
                "local-environments",
                "worktrees",
                "data-controls",
            ]
        );
        assert_eq!(actual.iter().copied().collect::<HashSet<_>>().len(), 21);
        assert!(pages().all(|page| !page.sections.is_empty()));
    }
}
