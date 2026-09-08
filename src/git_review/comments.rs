//! Structured local review comments, shared by the panel and prompt composer.

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReviewComment {
    pub id: u64,
    pub path: String,
    pub start: u32,
    pub end: u32,
    pub old: bool,
    pub text: String,
}

impl ReviewComment {
    pub fn location(&self) -> String {
        let side = if self.old { "L" } else { "R" };
        if self.start == self.end {
            format!("{side}{}", self.start)
        } else {
            format!("{side}{}–{side}{}", self.start, self.end)
        }
    }
}

pub fn comments_prompt(comments: &[ReviewComment]) -> String {
    comments
        .iter()
        .map(|c| format!("{}:{}\n{}", c.path, c.location(), c.text))
        .collect::<Vec<_>>()
        .join("\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_preserves_file_side_range_and_multiline_unicode_text() {
        let mut c = ReviewComment {
            id: 1,
            path: "src/中文.rs".into(),
            start: 10,
            end: 12,
            old: true,
            text: "请修复 🙂\n保留换行".into(),
        };
        assert_eq!(
            comments_prompt(&[c.clone()]),
            "src/中文.rs:L10–L12\n请修复 🙂\n保留换行"
        );
        c.old = false;
        c.end = 10;
        assert_eq!(c.location(), "R10");
    }
}
