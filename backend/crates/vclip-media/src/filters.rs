//! FFmpeg video filter definitions.
//!
//! These filters match the Python implementation exactly.

use vclip_models::Style;

/// Split view filter (left and right halves stacked vertically).
pub const FILTER_SPLIT: &str = concat!(
    "scale=1920:-2,split=2[full][full2];",
    "[full]crop=910:1080:0:0[left];",
    "[full2]crop=960:1080:960:0[right];",
    "[left]scale=1080:-2,crop=1080:960[left_scaled];",
    "[right]scale=1080:-2,crop=1080:960[right_scaled];",
    "[left_scaled][right_scaled]vstack=inputs=2"
);

/// Left focus filter (left half expanded to portrait).
pub const FILTER_LEFT_FOCUS: &str = concat!(
    "scale=1920:-2,",
    "crop=910:1080:0:0,",
    "scale=1080:1920:force_original_aspect_ratio=decrease,",
    // Anchor video to the top; pad only below
    "pad=1080:1920:(ow-iw)/2:0"
);

/// Right focus filter (right half expanded to portrait).
pub const FILTER_RIGHT_FOCUS: &str = concat!(
    "scale=1920:-2,",
    "crop=960:1080:960:0,",
    "scale=1080:1920:force_original_aspect_ratio=decrease,",
    // Anchor video to the top; pad only below
    "pad=1080:1920:(ow-iw)/2:0"
);

/// Center focus filter (center vertical slice expanded to portrait).
/// Uses a 9:16 crop anchored at the horizontal center, clamped to avoid negative offsets.
pub const FILTER_CENTER_FOCUS: &str = concat!(
    "scale=1920:-2,",
    "crop=ih*9/16:ih:max((iw-ih*9/16)/2\\,0):0,",
    "scale=1080:1920:force_original_aspect_ratio=decrease,",
    // Anchor video to the top; pad only below
    "pad=1080:1920:(ow-iw)/2:0"
);

/// Default portrait crop filter.
pub const FILTER_DEFAULT_PORTRAIT: &str = "scale=-2:1920,crop=1080:1920";

/// Build video filter for a style.
pub fn build_video_filter(style: Style) -> Option<String> {
    match style {
        Style::Split => Some(FILTER_SPLIT.to_string()),
        Style::LeftFocus => Some(FILTER_LEFT_FOCUS.to_string()),
        Style::RightFocus => Some(FILTER_RIGHT_FOCUS.to_string()),
        Style::CenterFocus => Some(FILTER_CENTER_FOCUS.to_string()),
        Style::Original => None, // No filter for original
        // SplitFast uses FastSplitEngine - no filter here
        Style::SplitFast => None,
        // All intelligent styles are handled separately with face detection
        Style::Intelligent
        | Style::IntelligentSplit
        | Style::IntelligentSpeaker
        | Style::IntelligentSplitSpeaker
        | Style::IntelligentMotion
        | Style::IntelligentSplitMotion
        | Style::IntelligentSplitActivity
        | Style::IntelligentCinematic => None,
    }
}

/// Build filter for cropping left half of video.
pub fn filter_crop_left_half() -> &'static str {
    "crop=iw/2:ih:0:0"
}

/// Build filter for cropping right half of video.
pub fn filter_crop_right_half() -> &'static str {
    "crop=iw/2:ih:iw/2:0"
}

/// Build filter for stacking two videos vertically.
pub fn filter_vstack(top_width: u32, top_height: u32, bottom_width: u32, bottom_height: u32) -> String {
    format!(
        "[0:v]scale={}:{}:force_original_aspect_ratio=decrease,pad={}:{}:(ow-iw)/2:(oh-ih)/2[top];\
         [1:v]scale={}:{}:force_original_aspect_ratio=decrease,pad={}:{}:(ow-iw)/2:(oh-ih)/2[bottom];\
         [top][bottom]vstack",
        top_width, top_height, top_width, top_height,
        bottom_width, bottom_height, bottom_width, bottom_height
    )
}

/// Build filter for thumbnail generation.
pub fn filter_thumbnail(width: u32) -> String {
    format!("scale={}:-2", width)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_video_filter() {
        assert!(build_video_filter(Style::Split).is_some());
        assert!(build_video_filter(Style::Original).is_none());
        assert!(build_video_filter(Style::CenterFocus).is_some());
    }

    #[test]
    fn test_vstack_filter() {
        let filter = filter_vstack(1080, 960, 1080, 960);
        assert!(filter.contains("vstack"));
        assert!(filter.contains("1080"));
    }
}
