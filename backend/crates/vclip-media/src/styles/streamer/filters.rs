//! FFmpeg filter graph building for Streamer style.

use super::config::StreamerConfig;

/// Build the FFmpeg filter complex for landscape-in-portrait with blurred background.
///
/// # Arguments
/// * `config` - Streamer configuration
/// * `src_width` - Source video width
/// * `src_height` - Source video height
/// * `countdown_number` - Optional countdown number to overlay (1-5)
/// * `scene_title` - Optional scene title to display next to countdown
///
/// # Returns
/// The filter complex string for FFmpeg
pub fn build_streamer_filter(
    config: &StreamerConfig,
    src_width: u32,
    src_height: u32,
    countdown_number: Option<u8>,
    scene_title: Option<&str>,
) -> String {
    let (main_width, main_height) = config.calculate_main_video_dimensions(src_width, src_height);
    let y_offset = config.calculate_y_offset(main_height);

    // Build filter complex for landscape-in-portrait with blurred background
    let mut filter = format!(
        // Background: zoom, blur, and scale to fill portrait
        "[0:v]scale=iw*{zoom}:ih*{zoom},\
         crop={ow}:{oh}:(iw-{ow})/2:(ih-{oh})/2,\
         gblur=sigma={blur},\
         format=yuv420p[bg];\
         [0:v]scale={mw}:{mh}:flags=lanczos,\
         format=yuv420p[main];\
         [bg][main]overlay=(W-w)/2:{y_offset}:format=auto",
        zoom = config.background_zoom,
        ow = config.output_width,
        oh = config.output_height,
        blur = config.background_blur,
        mw = main_width,
        mh = main_height,
        y_offset = y_offset,
    );

    // Add countdown overlay if provided
    if let Some(num) = countdown_number {
        // Build the display text: "5. Title" or just "5." if no title
        let display_text = if let Some(title) = scene_title {
            // Escape special characters for FFmpeg drawtext
            let escaped_title = escape_drawtext(title);
            // Truncate title to max 35 chars to fit on screen
            let truncated = if escaped_title.chars().count() > 35 {
                format!("{}...", escaped_title.chars().take(32).collect::<String>())
            } else {
                escaped_title
            };
            format!("{}. {}", num, truncated)
        } else {
            format!("{}.", num)
        };
        
        filter = format!(
            "{filter}[composed];\
             [composed]drawtext=text='{text}':fontsize={size}:fontcolor=white:\
             borderw=4:bordercolor=black:x={x}:y={y}:\
             font=Arial[vout]",
            filter = filter,
            text = display_text,
            size = config.countdown_font_size,
            x = config.countdown_x,
            y = config.countdown_y,
        );
    } else {
        filter = format!("{}[vout]", filter);
    }

    filter
}

/// Escape special characters for FFmpeg drawtext filter.
fn escape_drawtext(s: &str) -> String {
    s.replace('\\', "\\\\")
     .replace('\'', "'\\''")
     .replace(':', "\\:")
     .replace('%', "\\%")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_filter_without_countdown() {
        let config = StreamerConfig::default();
        let filter = build_streamer_filter(&config, 1920, 1080, None, None);
        
        assert!(filter.contains("[bg]"));
        assert!(filter.contains("[main]"));
        assert!(filter.contains("[vout]"));
        assert!(filter.contains("gblur"));
        assert!(filter.contains("overlay"));
        assert!(!filter.contains("drawtext"));
    }

    #[test]
    fn test_build_filter_with_countdown() {
        let config = StreamerConfig::default();
        let filter = build_streamer_filter(&config, 1920, 1080, Some(5), None);
        
        assert!(filter.contains("drawtext"));
        assert!(filter.contains("text='5.'"));
        assert!(filter.contains("[vout]"));
    }

    #[test]
    fn test_filter_uses_config_values() {
        let config = StreamerConfig {
            output_width: 720,
            output_height: 1280,
            background_blur: 20.0,
            background_zoom: 2.0,
            ..Default::default()
        };
        let filter = build_streamer_filter(&config, 1920, 1080, None, None);
        
        assert!(filter.contains("crop=720:1280"));
        assert!(filter.contains("sigma=20"));
        assert!(filter.contains("scale=iw*2:ih*2"));
    }

    #[test]
    fn test_build_filter_with_countdown_and_title() {
        let config = StreamerConfig::default();
        let filter = build_streamer_filter(&config, 1920, 1080, Some(5), Some("Test Title"));
        
        assert!(filter.contains("drawtext"));
        assert!(filter.contains("5. Test Title"));
        assert!(filter.contains("[vout]"));
    }
}
