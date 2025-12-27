//! Configuration for Streamer style processor.

/// Configuration for the Streamer style processor.
#[derive(Debug, Clone)]
pub struct StreamerConfig {
    /// Output width in pixels (default: 1080 for 9:16 portrait).
    pub output_width: u32,
    /// Output height in pixels (default: 1920 for 9:16 portrait).
    pub output_height: u32,
    /// Background blur sigma (default: 30.0).
    pub background_blur: f32,
    /// Background zoom factor (default: 1.8).
    pub background_zoom: f32,
    /// Countdown font size in pixels (default: 120).
    pub countdown_font_size: u32,
    /// Countdown X position from left edge (default: 60).
    pub countdown_x: u32,
    /// Countdown Y position from top edge (default: 80).
    pub countdown_y: u32,
    /// Maximum number of scenes for Top Scenes compilation (default: 5).
    pub max_top_scenes: usize,
}

impl Default for StreamerConfig {
    fn default() -> Self {
        Self {
            output_width: 1080,
            output_height: 1920,
            background_blur: 30.0,
            background_zoom: 1.8,
            countdown_font_size: 120,
            countdown_x: 60,
            countdown_y: 80,
            max_top_scenes: 5,
        }
    }
}

impl StreamerConfig {
    /// Create a new config with custom output dimensions.
    pub fn with_dimensions(width: u32, height: u32) -> Self {
        Self {
            output_width: width,
            output_height: height,
            ..Default::default()
        }
    }

    /// Calculate the main video dimensions to fit within the output width.
    pub fn calculate_main_video_dimensions(&self, src_width: u32, src_height: u32) -> (u32, u32) {
        let scale_factor = self.output_width as f64 / src_width as f64;
        let main_height = (src_height as f64 * scale_factor) as u32;

        // Ensure even dimensions for h264 encoding
        let main_width = self.output_width - (self.output_width % 2);
        let main_height = main_height - (main_height % 2);

        (main_width, main_height)
    }

    /// Calculate the vertical offset to center the main video.
    pub fn calculate_y_offset(&self, main_height: u32) -> u32 {
        (self.output_height - main_height) / 2
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = StreamerConfig::default();
        assert_eq!(config.output_width, 1080);
        assert_eq!(config.output_height, 1920);
        assert_eq!(config.background_blur, 30.0);
        assert_eq!(config.background_zoom, 1.8);
        assert_eq!(config.max_top_scenes, 5);
    }

    #[test]
    fn test_calculate_dimensions_16_9() {
        let config = StreamerConfig::default();
        // 1920x1080 (16:9) input
        let (w, h) = config.calculate_main_video_dimensions(1920, 1080);
        assert_eq!(w, 1080);
        // 1080 * (1080/1920) = 607.5, cast to u32 = 607, rounded to even = 606
        assert_eq!(h, 606);
    }

    #[test]
    fn test_calculate_y_offset() {
        let config = StreamerConfig::default();
        let y = config.calculate_y_offset(608);
        assert_eq!(y, (1920 - 608) / 2);
    }
}
