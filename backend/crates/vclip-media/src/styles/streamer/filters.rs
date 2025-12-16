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
        // Build the display text with the countdown number
        // Title will be displayed on separate lines below the number
        if let Some(title) = scene_title {
            // First line: just the number
            // Following lines: title wrapped to ~25 chars per line
            let escaped_title = escape_drawtext(title);
            let wrapped_title = wrap_text(&escaped_title, 25);
            
            // Use two drawtext filters: one for the number, one for the wrapped title
            filter = format!(
                "{filter}[composed];\
                 [composed]drawtext=text='{num}.':fontsize={size}:fontcolor=white:\
                 borderw=4:bordercolor=black:x={x}:y={y}:\
                 font=Arial[withnum];\
                 [withnum]drawtext=text='{title}':fontsize={title_size}:fontcolor=white:\
                 borderw=3:bordercolor=black:x={x}:y={title_y}:\
                 font=Arial[vout]",
                filter = filter,
                num = num,
                size = config.countdown_font_size,
                x = config.countdown_x,
                y = config.countdown_y,
                title = wrapped_title,
                title_size = config.countdown_font_size * 2 / 3, // Smaller font for title
                title_y = config.countdown_y + config.countdown_font_size + 10, // Below the number
            );
        } else {
            filter = format!(
                "{filter}[composed];\
                 [composed]drawtext=text='{num}.':fontsize={size}:fontcolor=white:\
                 borderw=4:bordercolor=black:x={x}:y={y}:\
                 font=Arial[vout]",
                filter = filter,
                num = num,
                size = config.countdown_font_size,
                x = config.countdown_x,
                y = config.countdown_y,
            );
        }
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

/// Wrap text to multiple lines with max chars per line.
/// Uses word boundaries when possible, respecting the max line count.
fn wrap_text(text: &str, max_chars: usize) -> String {
    let mut lines: Vec<String> = Vec::new();
    let mut current_line = String::new();
    
    for word in text.split_whitespace() {
        if current_line.is_empty() {
            // First word on line
            if word.chars().count() > max_chars {
                // Word too long, truncate it
                current_line = word.chars().take(max_chars - 1).collect::<String>() + "-";
            } else {
                current_line = word.to_string();
            }
        } else if current_line.chars().count() + 1 + word.chars().count() <= max_chars {
            // Word fits on current line
            current_line.push(' ');
            current_line.push_str(word);
        } else {
            // Word doesn't fit, start new line
            lines.push(std::mem::take(&mut current_line));
            if lines.len() >= 3 {
                // Max 3 lines, truncate remaining
                let last = lines.last_mut().unwrap();
                if last.chars().count() > max_chars - 3 {
                    *last = last.chars().take(max_chars - 3).collect::<String>() + "...";
                }
                break;
            }
            if word.chars().count() > max_chars {
                current_line = word.chars().take(max_chars - 1).collect::<String>() + "-";
            } else {
                current_line = word.to_string();
            }
        }
    }
    
    // Add last line if not at max
    if !current_line.is_empty() && lines.len() < 3 {
        lines.push(current_line);
    }
    
    // Join with escaped newline for FFmpeg
    lines.join("\\n")
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
