//! FFmpeg filter graph building for Streamer style.

use super::config::StreamerConfig;

/// Build the FFmpeg filter complex for landscape-in-portrait with blurred background.
///
/// # Arguments
/// * `config` - Streamer configuration
/// * `src_width` - Source video width
/// * `src_height` - Source video height
/// * `countdown_number` - Optional countdown number to overlay (1-5)
///
/// # Returns
/// The filter complex string for FFmpeg
pub fn build_streamer_filter(
    config: &StreamerConfig,
    src_width: u32,
    src_height: u32,
    countdown_number: Option<u8>,
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
        filter = format!(
            "{filter}[composed];\
             [composed]drawtext=text='{num}.':fontsize={size}:fontcolor=white:\
             borderw=4:bordercolor=black:x={x}:y={y}:\
             font=Arial:fontweight=bold[vout]",
            filter = filter,
            num = num,
            size = config.countdown_font_size,
            x = config.countdown_x,
            y = config.countdown_y,
        );
    } else {
        filter = format!("{}[vout]", filter);
    }

    filter
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_filter_without_countdown() {
        let config = StreamerConfig::default();
        let filter = build_streamer_filter(&config, 1920, 1080, None);
        
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
        let filter = build_streamer_filter(&config, 1920, 1080, Some(5));
        
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
        let filter = build_streamer_filter(&config, 1920, 1080, None);
        
        assert!(filter.contains("crop=720:1280"));
        assert!(filter.contains("sigma=20"));
        assert!(filter.contains("scale=iw*2:ih*2"));
    }
}
