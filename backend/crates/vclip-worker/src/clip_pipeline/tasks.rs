use vclip_models::{sanitize_filename_title, AspectRatio, ClipTask, CropMode, Style};

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
                scene_title: sanitize_filename_title(&highlight.title),
                scene_description: highlight.description.clone(),
                start: highlight.start.clone(),
                end: highlight.end.clone(),
                style: *style,
                crop_mode: *crop_mode,
                target_aspect: *target_aspect,
                priority: highlight.id, // Use highlight ID as priority
                pad_before: highlight.pad_before_seconds,
                pad_after: highlight.pad_after_seconds,
                streamer_split_params: None,
                streamer_params: None,
            };
            tasks.push(task);
        }
    }

    tasks
}

/// Generate clip tasks from a subset of highlight entries (for reprocessing).
/// Uses R2 HighlightEntry format (legacy).
pub fn generate_clip_tasks_from_highlights(
    highlights: &[&vclip_storage::operations::HighlightEntry],
    styles: &[Style],
    crop_mode: &CropMode,
    target_aspect: &AspectRatio,
) -> Vec<ClipTask> {
    let mut tasks = Vec::new();

    for highlight in highlights {
        for style in styles {
            tasks.push(ClipTask {
                scene_id: highlight.id,
                scene_title: sanitize_filename_title(&highlight.title),
                scene_description: highlight.description.clone(),
                start: highlight.start.clone(),
                end: highlight.end.clone(),
                style: *style,
                crop_mode: *crop_mode,
                target_aspect: *target_aspect,
                priority: highlight.id,
                pad_before: highlight.pad_before_seconds,
                pad_after: highlight.pad_after_seconds,
                streamer_split_params: None,
                streamer_params: None,
            });
        }
    }

    tasks
}

/// Generate clip tasks from Firestore VideoHighlights (preferred source).
pub fn generate_clip_tasks_from_firestore_highlights(
    highlights: &[&vclip_models::Highlight],
    styles: &[Style],
    crop_mode: &CropMode,
    target_aspect: &AspectRatio,
) -> Vec<ClipTask> {
    generate_clip_tasks_from_firestore_highlights_with_params(
        highlights,
        styles,
        crop_mode,
        target_aspect,
        None,
    )
}

/// Generate clip tasks from Firestore VideoHighlights with optional StreamerSplit params.
pub fn generate_clip_tasks_from_firestore_highlights_with_params(
    highlights: &[&vclip_models::Highlight],
    styles: &[Style],
    crop_mode: &CropMode,
    target_aspect: &AspectRatio,
    streamer_split_params: Option<vclip_models::StreamerSplitParams>,
) -> Vec<ClipTask> {
    let mut tasks = Vec::new();

    for highlight in highlights {
        for style in styles {
            // Only include streamer_split_params for StreamerSplit style
            let params = if *style == Style::StreamerSplit {
                streamer_split_params.clone()
            } else {
                None
            };

            tasks.push(ClipTask {
                scene_id: highlight.id,
                scene_title: sanitize_filename_title(&highlight.title),
                scene_description: highlight.description.clone(),
                start: highlight.start.clone(),
                end: highlight.end.clone(),
                style: *style,
                crop_mode: *crop_mode,
                target_aspect: *target_aspect,
                priority: highlight.id,
                pad_before: highlight.pad_before,
                pad_after: highlight.pad_after,
                streamer_split_params: params,
                streamer_params: None,
            });
        }
    }

    tasks
}

