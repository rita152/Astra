#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ControlSpec {
    None,
    Switch(bool),
    Button(&'static str),
    Select(&'static str),
    Value(&'static str),
    Shortcut(&'static str),
    Segmented(&'static [&'static str], usize),
    Danger(&'static str),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RowSpec {
    pub title: &'static str,
    pub subtitle: &'static str,
    pub control: ControlSpec,
}

impl RowSpec {
    pub const fn new(title: &'static str, subtitle: &'static str, control: ControlSpec) -> Self {
        Self {
            title,
            subtitle,
            control,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SectionSpec {
    pub title: &'static str,
    pub subtitle: &'static str,
    pub rows: &'static [RowSpec],
}

impl SectionSpec {
    pub const fn new(
        title: &'static str,
        subtitle: &'static str,
        rows: &'static [RowSpec],
    ) -> Self {
        Self {
            title,
            subtitle,
            rows,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PageKind {
    Standard,
    Profile,
    Pets,
    KeyboardShortcuts,
    Usage,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PageSpec {
    pub slug: &'static str,
    pub label: &'static str,
    pub intro: &'static str,
    pub kind: PageKind,
    pub sections: &'static [SectionSpec],
}

impl PageSpec {
    pub const fn new(
        slug: &'static str,
        label: &'static str,
        intro: &'static str,
        kind: PageKind,
        sections: &'static [SectionSpec],
    ) -> Self {
        Self {
            slug,
            label,
            intro,
            kind,
            sections,
        }
    }
}
