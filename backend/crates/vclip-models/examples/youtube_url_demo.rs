//! Demo: YouTube URL Configuration Generator
//!
//! Run with: cargo run -p vclip-models --example youtube_url_demo

use vclip_models::{analyze_youtube_url, LiveCaptureMode, YoutubeUrlInput};

fn main() {
    let test_urls = [
        "https://www.youtube.com/watch?v=dQw4w9WgXcQ",
        "https://youtu.be/dQw4w9WgXcQ?t=30",
        "https://www.youtube.com/shorts/abc123def45",
        "https://www.youtube.com/watch?v=dQw4w9WgXcQ&list=PLrAXtmRdnEQy",
        "https://vimeo.com/123456789",
        "https://www.youtube.com/playlist?list=PLrAXtmRdnEQy",
    ];

    for url in test_urls {
        println!("\n{}", "=".repeat(60));
        println!("INPUT: {}", url);
        println!("{}", "=".repeat(60));

        let input = YoutubeUrlInput {
            raw_url: url.to_string(),
            preferred_sub_langs: vec!["en".to_string()],
            allow_auto_subs: true,
            live_capture_mode: LiveCaptureMode::FromStart,
            max_expected_duration_sec: 21600,
        };

        let config = analyze_youtube_url(&input);

        println!(
            "{}",
            config
                .to_json_pretty()
                .expect("serialization should be infallible")
        );
    }
}
