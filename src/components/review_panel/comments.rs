//! Comment editing and the shared prompt payload.

use super::*;

impl ReviewPanel {
    pub(super) fn begin_comment(&mut self, d: Draft, cx: &mut Context<Self>) {
        if d.file >= self.snapshot.files.len() {
            return;
        }
        self.editing_comment = None;
        self.draft = Some(d);
        self.input.update(cx, |i, cx| {
            i.set_accessible_name("请求更改");
            i.set_placeholder("请求更改", cx);
            i.set_text_silently("", cx);
        });
        self.focus_input = true;
        self.rebuild(cx);
    }
    pub(super) fn save_comment(&mut self, cx: &mut Context<Self>) {
        let text = self.input.read(cx).text().trim().to_owned();
        if text.is_empty() {
            return;
        }
        if let Some(id) = self.editing_comment.take() {
            if let Some(comment) = self.comments.iter_mut().find(|c| c.id == id) {
                comment.text = text;
            }
            self.emit_comments(cx);
            self.rebuild(cx);
            self.focus_pending = true;
            return;
        }
        if let Some(d) = self.draft.take() {
            let path = self.snapshot.files[d.file].path.clone();
            self.comments.push(Comment {
                id: self.next_comment,
                path,
                start: d.start.min(d.end),
                end: d.start.max(d.end),
                old: d.old,
                text,
            });
            self.next_comment += 1;
            self.emit_comments(cx);
            self.rebuild(cx);
            self.focus_pending = true;
        }
    }
    pub(super) fn emit_comments(&self, cx: &mut Context<Self>) {
        cx.emit(ReviewEvent::CommentsChanged(
            self.comments
                .iter()
                .map(|c| {
                    let mut comment = c.clone();
                    let root = if self.snapshot.root.as_os_str().is_empty() {
                        &self.cwd
                    } else {
                        &self.snapshot.root
                    };
                    comment.path = root.join(&c.path).to_string_lossy().into_owned();
                    comment
                })
                .collect(),
        ));
    }
    pub fn restore_comments(&mut self, comments: Vec<Comment>, cx: &mut Context<Self>) {
        let root = if self.snapshot.root.as_os_str().is_empty() {
            &self.cwd
        } else {
            &self.snapshot.root
        };
        for mut comment in comments {
            if self
                .comments
                .iter()
                .any(|existing| existing.id == comment.id)
            {
                continue;
            }
            if let Ok(relative) = std::path::Path::new(&comment.path).strip_prefix(root) {
                comment.path = relative.to_string_lossy().into_owned();
            }
            self.next_comment = self.next_comment.max(comment.id.saturating_add(1));
            self.comments.push(comment);
        }
        self.next_comment = self
            .comments
            .iter()
            .fold(self.next_comment, |next, comment| {
                next.max(comment.id.saturating_add(1))
            });
        self.emit_comments(cx);
        self.rebuild(cx);
    }
    pub fn clear_comments(&mut self, cx: &mut Context<Self>) {
        self.comments.clear();
        self.editing_comment = None;
        self.emit_comments(cx);
        self.rebuild(cx);
    }
    pub(super) fn edit_comment(&mut self, id: u64, cx: &mut Context<Self>) {
        let Some(comment) = self.comments.iter().find(|c| c.id == id) else {
            return;
        };
        let text = comment.text.clone();
        self.draft = None;
        self.editing_comment = Some(id);
        self.input
            .update(cx, |input, cx| input.set_text_silently(&text, cx));
        self.focus_input = true;
        self.rebuild(cx);
    }
    pub fn show_comments(&mut self, cx: &mut Context<Self>) {
        self.query.clear();
        self.filter
            .update(cx, |input, cx| input.set_text_silently("", cx));
        for comment in &self.comments {
            self.collapsed.remove(&comment.path);
        }
        self.rebuild(cx);
        if let Some(index) = self
            .rows
            .iter()
            .position(|row| matches!(row, Row::Comment(_)))
        {
            self.scroll.scroll_to_reveal_item(index);
        }
        self.focus_pending = true;
        cx.notify();
    }
}
