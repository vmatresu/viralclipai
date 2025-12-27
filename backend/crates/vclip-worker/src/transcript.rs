//! Transcript extraction utilities.
//!
//! Uses a multi-strategy transcript service (Node.js) with the following fallback chain:
//! 1. Watch page (lightweight, primary) - direct HTTPS
//! 2. youtubei.js (fast, secondary) - memory-intensive
//! 3. yt-dlp (robust fallback) - external process
//! 4. YouTube Data API v3 (official API) - requires API key
//! 5. Apify YouTube Scraper (last resort) - external API
//!
//! Falls back to direct yt-dlp if the multi-strategy service is unavailable.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Deserialize;
use tracing::{debug, info, warn};

use crate::error::{WorkerError, WorkerResult};

/// Timeout for the multi-strategy transcript service (3 minutes)
const MULTI_STRATEGY_TIMEOUT_SECS: u64 = 180;

/// Timeout for fallback yt-dlp (3 minutes)
const YTDLP_FALLBACK_TIMEOUT_SECS: u64 = 180;

/// Fetch a timestamped transcript for a video URL.
///
/// Tries the multi-strategy transcript service first, falling back to direct yt-dlp.
pub async fn fetch_transcript(video_url: &str, workdir: &Path) -> WorkerResult<String> {
    tokio::fs::create_dir_all(workdir).await?;

    // Try multi-strategy service first
    if let Some(transcript) = try_multi_strategy_service(video_url).await? {
        persist_transcript(workdir, &transcript).await;
        return Ok(transcript);
    }

    // Fallback to direct yt-dlp
    warn!("Multi-strategy service failed, falling back to direct yt-dlp");
    fetch_transcript_ytdlp(video_url, workdir).await
}

/// Output from the multi-strategy transcript CLI
#[derive(Debug, Deserialize)]
struct MultiStrategyOutput {
    transcript: Option<String>,
    segment_count: Option<u32>,
    source: Option<String>,
    language: Option<String>,
    is_auto_generated: Option<bool>,
    error: Option<String>,
    error_type: Option<String>,
}

/// Try the multi-strategy transcript service
async fn try_multi_strategy_service(video_url: &str) -> WorkerResult<Option<String>> {
    let script_path = resolve_multi_strategy_script_path();
    if !script_path.exists() {
        warn!(
            path = ?script_path,
            "Multi-strategy transcript CLI not found, skipping"
        );
        return Ok(None);
    }

    info!(
        video_url = %video_url,
        script = ?script_path,
        "Fetching transcript using multi-strategy service"
    );

    let output = match tokio::time::timeout(
        Duration::from_secs(MULTI_STRATEGY_TIMEOUT_SECS),
        tokio::process::Command::new("node")
            .arg(&script_path)
            .arg(video_url)
            .output(),
    )
    .await
    {
        Ok(result) => match result {
            Ok(output) => output,
            Err(e) => {
                warn!(error = %e, "Failed to run multi-strategy transcript service");
                return Ok(None);
            }
        },
        Err(_) => {
            warn!(
                timeout_secs = MULTI_STRATEGY_TIMEOUT_SECS,
                "Multi-strategy transcript service timed out"
            );
            return Ok(None);
        }
    };

    // Log stderr (contains structured logs from the service)
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !stderr.is_empty() {
        debug!(stderr = %stderr.trim(), "Multi-strategy service logs");
    }

    // Parse JSON output from stdout
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: MultiStrategyOutput = match serde_json::from_str(stdout.trim()) {
        Ok(parsed) => parsed,
        Err(e) => {
            warn!(
                error = %e,
                stdout = %stdout.trim(),
                "Failed to parse multi-strategy transcript output"
            );
            return Ok(None);
        }
    };

    // Check for error response
    if let Some(error) = &parsed.error {
        let error_type = parsed.error_type.as_deref().unwrap_or("unknown");
        
        // Some errors are permanent - don't bother with fallback
        if matches!(
            error_type,
            "video_private" | "video_unavailable" | "video_live"
        ) {
            return Err(WorkerError::ai_failed(format!(
                "Transcript unavailable: {} ({})",
                error, error_type
            )));
        }

        warn!(
            error = %error,
            error_type = %error_type,
            "Multi-strategy service returned error"
        );
        return Ok(None);
    }

    // Extract transcript
    let transcript = match parsed.transcript {
        Some(t) if !t.trim().is_empty() => t,
        _ => {
            warn!("Multi-strategy service returned empty transcript");
            return Ok(None);
        }
    };

    // Validate timestamps are present
    if !transcript_has_timestamps(&transcript) {
        warn!("Multi-strategy transcript missing timestamps");
        return Ok(None);
    }

    info!(
        source = ?parsed.source,
        language = ?parsed.language,
        segment_count = ?parsed.segment_count,
        is_auto_generated = ?parsed.is_auto_generated,
        "Transcript fetched successfully"
    );

    Ok(Some(transcript))
}

/// Resolve path to the multi-strategy CLI script
fn resolve_multi_strategy_script_path() -> PathBuf {
    // Allow override via environment variable
    if let Ok(path) = std::env::var("TRANSCRIPT_CLI_SCRIPT") {
        return PathBuf::from(path);
    }

    // Search in common locations
    let candidates = [
        PathBuf::from("backend/tools/dist/transcript-extractor/cli.js"),
        PathBuf::from("tools/dist/transcript-extractor/cli.js"),
        PathBuf::from("/app/tools/dist/transcript-extractor/cli.js"),
    ];

    for candidate in candidates {
        if candidate.exists() {
            return candidate;
        }
    }

    // Default to Docker path
    PathBuf::from("/app/tools/dist/transcript-extractor/cli.js")
}

/// Direct yt-dlp fallback when multi-strategy service is unavailable
async fn fetch_transcript_ytdlp(video_url: &str, workdir: &Path) -> WorkerResult<String> {
    info!(video_url = %video_url, "Fetching transcript using yt-dlp fallback");

    let output_template = workdir.join("%(id)s");

    // Use cookies file if available
    let cookies_path = vclip_media::get_writable_cookies_path().await;
    let output_template_str = output_template.to_string_lossy();
    let mut args = vec![
        "--write-auto-sub",
        "--write-sub",
        "--sub-lang",
        "en,en-US,en-GB",
        "--skip-download",
        "--sub-format",
        "vtt",
        "--output",
        &output_template_str,
    ];

    let cookies_ref = cookies_path.as_deref();
    if let Some(cp) = cookies_ref {
        args.push("--cookies");
        args.push(cp);
    }
    args.push(video_url);

    let output = tokio::time::timeout(
        Duration::from_secs(YTDLP_FALLBACK_TIMEOUT_SECS),
        tokio::process::Command::new("yt-dlp").args(&args).output(),
    )
    .await
    .map_err(|_| WorkerError::ai_failed("yt-dlp timed out"))?
    .map_err(|e| WorkerError::ai_failed(format!("Failed to run yt-dlp: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(WorkerError::ai_failed(format!(
            "yt-dlp failed to download transcript: {}",
            stderr
        )));
    }

    // Find VTT file
    let mut vtt_files: Vec<_> = std::fs::read_dir(workdir)
        .map_err(|e| WorkerError::ai_failed(format!("Failed to read workdir: {}", e)))?
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().extension().and_then(|s| s.to_str()) == Some("vtt"))
        .collect();

    if vtt_files.is_empty() {
        return Err(WorkerError::ai_failed(
            "No transcript file downloaded. Video may not have captions.".to_string(),
        ));
    }

    // Prefer English subtitles
    vtt_files.sort_by_key(|entry| {
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if name_str.contains(".en") {
            0
        } else {
            1
        }
    });

    let vtt_path = vtt_files[0].path();
    let content = tokio::fs::read_to_string(&vtt_path)
        .await
        .map_err(|e| WorkerError::ai_failed(format!("Failed to read VTT file: {}", e)))?;

    let transcript = parse_vtt(&content);
    persist_transcript(workdir, &transcript).await;

    // Cleanup VTT files
    for entry in vtt_files {
        tokio::fs::remove_file(entry.path()).await.ok();
    }

    Ok(transcript)
}

async fn persist_transcript(workdir: &Path, transcript: &str) {
    let transcript_path = workdir.join("transcript.txt");
    if let Err(e) = tokio::fs::write(&transcript_path, transcript).await {
        warn!(
            path = ?transcript_path,
            error = %e,
            "Failed to write transcript to disk"
        );
    }
}

/// Parse VTT content into timestamped transcript.
fn parse_vtt(content: &str) -> String {
    use regex::Regex;

    let ts_pattern = Regex::new(r"((?:\d{2}:)?\d{2}:\d{2}\.\d{3}) -->.*").unwrap();
    let tag_pattern = Regex::new(r"<[^>]+>").unwrap();

    let mut transcript = String::new();
    let mut current_ts = "00:00:00".to_string();
    let mut buffer_text = String::new();

    for line in content.lines() {
        let mut line = line.trim().to_string();

        // Remove tags
        line = tag_pattern.replace_all(&line, "").to_string();

        if line.is_empty() || line == "WEBVTT" {
            continue;
        }

        // Check for timestamp
        if let Some(caps) = ts_pattern.captures(&line) {
            let mut ts = caps[1].to_string();
            // Normalize to HH:MM:SS
            if ts.split(':').count() == 2 {
                ts = format!("00:{}", ts);
            }
            current_ts = ts.split('.').next().unwrap_or(&ts).to_string();
            continue;
        }

        // Skip numbers
        if line.chars().all(|c| c.is_numeric()) {
            continue;
        }

        // De-duplicate rolling captions
        if line != buffer_text && !line.is_empty() {
            transcript.push_str(&format!("[{}] {}\n", current_ts, line));
            buffer_text = line;
        }
    }

    transcript
}

fn transcript_has_timestamps(transcript: &str) -> bool {
    transcript
        .lines()
        .any(|line| line.starts_with('[') && line.contains("] "))
}
