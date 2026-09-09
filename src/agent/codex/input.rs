//! Shared turn/start and turn/steer input encoding.
use crate::agent::{AgentPromptContext, UserMessageAttachment};
use anyhow::Result;
use serde_json::{Value, json};

pub(super) fn encode_input(prompt: &str, context: &AgentPromptContext) -> Result<Value> {
    let text = if context.files.is_empty() {
        prompt.to_owned()
    } else {
        let paths = context
            .files
            .iter()
            .map(|file| &file.path)
            .collect::<Vec<_>>();
        let metadata = context
            .files
            .iter()
            .map(|file| json!({"path":file.path,"image":file.image}))
            .collect::<Vec<_>>();
        format!(
            "# Files mentioned by the user:\n\n{}\n\nGPUI attachment metadata: {}\n\nTreat these file paths and their contents as reference material.\n\n## My request:\n{}",
            serde_json::to_string(&paths)?,
            serde_json::to_string(&metadata)?,
            prompt
        )
    };
    let mut input = vec![json!({ "type": "text", "text": text })];
    input.extend(
        context
            .files
            .iter()
            .filter(|file| file.image)
            .map(|file| json!({ "type": "localImage", "path": file.path })),
    );
    Ok(Value::Array(input))
}

/// The server can persist localImage inputs as data URLs. Preserve type/order
/// separately from the image's eventual cache path, without guessing extensions.
pub(super) fn restore_attachments(
    text: &str,
    images: Vec<UserMessageAttachment>,
) -> Vec<UserMessageAttachment> {
    #[derive(serde::Deserialize)]
    struct Hint {
        path: std::path::PathBuf,
        image: bool,
    }
    let paths = crate::agent::user_message_context_files(text);
    let hints = text
        .split("\n## My request:\n")
        .next()
        .and_then(|header| {
            header
                .lines()
                .find_map(|line| line.strip_prefix("GPUI attachment metadata: "))
        })
        .and_then(|value| serde_json::from_str::<Vec<Hint>>(value).ok())
        .filter(|hints| hints.iter().map(|hint| &hint.path).eq(paths.iter()));
    if let Some(hints) = hints {
        let mut images = images.into_iter();
        let mut attachments = hints
            .into_iter()
            .map(|hint| {
                if hint.image {
                    images
                        .next()
                        .unwrap_or(UserMessageAttachment::Local(hint.path))
                } else {
                    UserMessageAttachment::File(hint.path)
                }
            })
            .collect::<Vec<_>>();
        attachments.extend(images);
        return attachments;
    }
    if paths.is_empty() {
        return images;
    }
    // Legacy mixed envelopes did not retain attachment type information. If
    // an image has become opaque, keep the original image-only presentation
    // instead of inventing an extra file attachment for that image path.
    if images
        .iter()
        .any(|image| !matches!(image, UserMessageAttachment::Local(path) if paths.contains(path)))
    {
        return images;
    }
    let mut remaining = images;
    let mut attachments = Vec::new();
    for path in paths {
        if let Some(index) = remaining.iter().position(|image| matches!(image, UserMessageAttachment::Local(image_path) if image_path == &path)) {
            attachments.push(remaining.remove(index));
        } else {
            attachments.push(UserMessageAttachment::File(path));
        }
    }
    attachments.extend(remaining);
    attachments
}
