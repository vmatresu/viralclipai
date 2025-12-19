//! Transcript extraction utilities.
//!
//! Attempts youtubei.js first for speed and reliability, then falls back to yt-dlp.
//! Ensures transcripts include timestamps for downstream scene detection.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Deserialize;
use tracing::{info, warn};

use crate::error::{WorkerError, WorkerResult};

/// Fetch a timestamped transcript for a video URL.
pub async fn fetch_transcript(video_url: &str, workdir: &Path) -> WorkerResult<String> {
    tokio::fs::create_dir_all(workdir).await?;

    if let Some(transcript) = try_get_transcript_youtubei(video_url).await? {
        persist_transcript(workdir, &transcript).await;
        return Ok(transcript);
    }

    fetch_transcript_ytdlp(video_url, workdir).await
}

async fn try_get_transcript_youtubei(video_url: &str) -> WorkerResult<Option<String>> {
    let script_path = resolve_youtubei_script_path();
    if !script_path.exists() {
        warn!(
            path = ?script_path,
            "youtubei.js transcript script not found, skipping"
        );
        return Ok(None);
    }

    info!("Fetching transcript for {} using youtubei.js", video_url);

    let output = match tokio::time::timeout(
        Duration::from_secs(25),
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
                warn!(error = %e, "Failed to run youtubei.js transcript script");
                return Ok(None);
            }
        },
        Err(_) => {
            warn!("youtubei.js transcript script timed out");
            return Ok(None);
        }
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        warn!(
            status = ?output.status.code(),
            error = %stderr.trim(),
            "youtubei.js transcript script failed"
        );
        return Ok(None);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: YoutubeITranscriptOutput = match serde_json::from_str(stdout.trim()) {
        Ok(parsed) => parsed,
        Err(e) => {
            warn!(error = %e, "Failed to parse youtubei.js transcript output");
            return Ok(None);
        }
    };

    let transcript = parsed.transcript.trim();
    if transcript.is_empty() {
        warn!("youtubei.js transcript output was empty");
        return Ok(None);
    }

    if !transcript_has_timestamps(transcript) {
        warn!("youtubei.js transcript missing timestamps, falling back");
        return Ok(None);
    }

    Ok(Some(transcript.to_string()))
}

async fn fetch_transcript_ytdlp(video_url: &str, workdir: &Path) -> WorkerResult<String> {
    info!("Fetching transcript for {} using yt-dlp", video_url);

    let output_template = workdir.join("%(id)s");

    // Run yt-dlp to download subtitles
    // Use cookies file if available for YouTube authentication (copy to writable location)
    let cookies_path = vclip_media::get_writable_cookies_path().await;
    let output_template_str = output_template.to_string_lossy();
    let mut args = vec![
        "--remote-components",
        "ejs:github",
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

    let output = tokio::process::Command::new("yt-dlp")
        .args(&args)
        .output()
        .await
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

#[derive(Debug, Deserialize)]
struct YoutubeITranscriptOutput {
    transcript: String,
}

fn resolve_youtubei_script_path() -> PathBuf {
    if let Ok(path) = std::env::var("YOUTUBEI_TRANSCRIPT_SCRIPT") {
        return PathBuf::from(path);
    }

    let candidates = [
        PathBuf::from("backend/tools/transcript-extractor/youtubei-transcript.mjs"),
        PathBuf::from("tools/transcript-extractor/youtubei-transcript.mjs"),
        PathBuf::from("/app/tools/transcript-extractor/youtubei-transcript.mjs"),
    ];

    for candidate in candidates {
        if candidate.exists() {
            return candidate;
        }
    }

    PathBuf::from("/app/tools/transcript-extractor/youtubei-transcript.mjs")
}

fn transcript_has_timestamps(transcript: &str) -> bool {
    transcript
        .lines()
        .any(|line| line.starts_with('[') && line.contains("] "))
}
