//! Thumbnail generation.

use std::path::Path;

use crate::command::{FfmpegCommand, FfmpegRunner};
use crate::error::MediaResult;
use vclip_models::encoding::{THUMBNAIL_SCALE_WIDTH, THUMBNAIL_TIMESTAMP};

/// Generate a thumbnail from a video file.
pub async fn generate_thumbnail(
    video_path: impl AsRef<Path>,
    output_path: impl AsRef<Path>,
) -> MediaResult<()> {
    let video_path = video_path.as_ref();
    let output_path = output_path.as_ref();

    let filter = format!("scale={}:-2", THUMBNAIL_SCALE_WIDTH);

    let cmd = FfmpegCommand::new(video_path, output_path)
        .input_arg("-ss")
        .input_arg(THUMBNAIL_TIMESTAMP)
        .single_frame()
        .video_filter(&filter)
        .log_level("error");

    FfmpegRunner::new().run(&cmd).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_thumbnail_filter() {
        let filter = format!("scale={}:-2", THUMBNAIL_SCALE_WIDTH);
        assert!(filter.contains("480"));
    }
}
