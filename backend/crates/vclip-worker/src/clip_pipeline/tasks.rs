use vclip_models::{AspectRatio, ClipTask, CropMode, Style};

use crate::gemini::HighlightsResponse;

/// Generate clip tasks from highlights and styles.
///
/// Creates one `ClipTask` per (highlight, style) combination.
pub fn generate_clip_tasks(
    highlights: &HighlightsResponse,
    styles: &[Style],
    crop_mode: &CropMode,
    target_aspect: &AspectRatio,
) -> Vec<ClipTask> {
    let mut tasks = Vec::new();

    for highlight in &highlights.highlights {
        for style in styles {
            let task = ClipTask {
                scene_id: highlight.id,
                scene_title: sanitize_title(&highlight.title),
                scene_description: highlight.description.clone(),
                start: highlight.start.clone(),
                end: highlight.end.clone(),
                style: *style,
                crop_mode: *crop_mode,
                target_aspect: *target_aspect,
                priority: highlight.id, // Use highlight ID as priority
                pad_before: highlight.pad_before_seconds,
                pad_after: highlight.pad_after_seconds,
            };
            tasks.push(task);
        }
    }

    tasks
}

/// Sanitize a title for use in filenames.
pub fn sanitize_title(title: &str) -> String {
    title
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == ' ' || *c == '-' || *c == '_')
        .collect::<String>()
        .trim()
        .replace(' ', "_")
        .chars()
        .take(50)
        .collect()
}

