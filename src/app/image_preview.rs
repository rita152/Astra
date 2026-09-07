//! Image preview behavior and presentation for the application shell.

use std::{path::PathBuf, process::Command};

use gpui::Context;

use super::ChatApp;

impl ChatApp {
    pub(super) fn download_preview_image(&mut self, source: PathBuf, cx: &mut Context<Self>) {
        let suggested_name = source
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("image.png")
            .to_owned();
        let initial_directory = source
            .parent()
            .map(PathBuf::from)
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
        let destination = cx.prompt_for_new_path(&initial_directory, Some(suggested_name.as_str()));
        cx.spawn(async move |_, _| {
            let Ok(Ok(Some(destination))) = destination.await else {
                return;
            };
            let _ = std::fs::copy(source, destination);
        })
        .detach();
    }
}

pub(super) fn finder_reveal_command(path: &str) -> Command {
    let mut command = Command::new("open");
    command.arg("-R").arg(path);
    command
}
