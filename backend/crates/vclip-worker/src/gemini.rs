//! Gemini AI client for video highlight extraction.
//!
//! This module provides integration with Google's Gemini API to analyze
//! video transcripts and extract viral highlights.

use std::path::Path;

use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use crate::error::{WorkerError, WorkerResult};

/// Gemini API client.
pub struct GeminiClient {
    api_key: String,
    client: Client,
}

/// Gemini API request.
#[derive(Debug, Serialize)]
struct GeminiRequest {
    contents: Vec<Content>,
    #[serde(rename = "generationConfig")]
    generation_config: GenerationConfig,
}

#[derive(Debug, Serialize)]
struct Content {
    parts: Vec<Part>,
}

#[derive(Debug, Serialize)]
struct Part {
    text: String,
}

#[derive(Debug, Serialize)]
struct GenerationConfig {
    #[serde(rename = "responseMimeType")]
    response_mime_type: String,
}

/// Gemini API response.
#[derive(Debug, Deserialize)]
struct GeminiResponse {
    candidates: Vec<Candidate>,
}

#[derive(Debug, Deserialize)]
struct Candidate {
    content: ResponseContent,
}

#[derive(Debug, Deserialize)]
struct ResponseContent {
    parts: Vec<ResponsePart>,
}

#[derive(Debug, Deserialize)]
struct ResponsePart {
    text: String,
}

/// Highlight data from AI analysis.
#[derive(Debug, Deserialize)]
pub struct HighlightsResponse {
    pub video_url: Option<String>,
    pub video_title: Option<String>,
    pub highlights: Vec<Highlight>,
}

#[derive(Debug, Deserialize)]
pub struct Highlight {
    pub id: u32,
    pub title: String,
    pub start: String,
    pub end: String,
    pub duration: u32,
    /// Padding before the start timestamp (seconds)
    #[serde(default = "default_pad_before")]
    pub pad_before_seconds: f64,
    /// Padding after the end timestamp (seconds)
    #[serde(default = "default_pad_after")]
    pub pad_after_seconds: f64,
    pub hook_category: Option<String>,
    pub reason: Option<String>,
    pub description: Option<String>,
}

fn default_pad_before() -> f64 {
    1.0
}

fn default_pad_after() -> f64 {
    1.5
}

impl GeminiClient {
    /// Create a new Gemini client.
    pub fn new() -> WorkerResult<Self> {
        let api_key = std::env::var("GEMINI_API_KEY")
            .map_err(|_| WorkerError::config_error("GEMINI_API_KEY not set"))?;

        Ok(Self {
            api_key,
            client: Client::new(),
        })
    }

    /// Get video metadata (title, URL) using yt-dlp.
    pub async fn get_video_metadata(&self, video_url: &str) -> WorkerResult<(String, String)> {
        info!("Getting video metadata for {} using yt-dlp", video_url);

        let output = tokio::process::Command::new("yt-dlp")
            .args(&[
                "--print", "title",
                "--print", "webpage_url",
                "--no-download",
                "--no-playlist",
                video_url,
            ])
            .output()
            .await
            .map_err(|e| WorkerError::ai_failed(format!("Failed to run yt-dlp for metadata: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(WorkerError::ai_failed(format!(
                "yt-dlp failed to get metadata: {}",
                stderr
            )));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let lines: Vec<&str> = stdout.lines().collect();

        if lines.len() < 2 {
            return Err(WorkerError::ai_failed(
                "yt-dlp did not return expected metadata".to_string(),
            ));
        }

        let title = lines[0].trim().to_string();
        let canonical_url = lines[1].trim().to_string();

        if title.is_empty() || canonical_url.is_empty() {
            return Err(WorkerError::ai_failed(
                "yt-dlp returned empty title or URL".to_string(),
            ));
        }

        info!("Got video metadata: title='{}', url='{}'", title, canonical_url);
        Ok((title, canonical_url))
    }

    /// Get transcript only (without calling Gemini).
    pub async fn get_transcript_only(&self, video_url: &str, workdir: &Path) -> WorkerResult<String> {
        self.get_transcript(video_url, workdir).await
    }

    /// Analyze transcript with Gemini AI.
    pub async fn analyze_transcript(
        &self,
        base_prompt: &str,
        video_url: &str,
        transcript: &str,
    ) -> WorkerResult<HighlightsResponse> {
        // 2. Build prompt
        let prompt = self.build_prompt(base_prompt, transcript);

        // 3. Call Gemini API with fallback models
        let models = vec![
            "gemini-2.5-flash",
            "gemini-2.5-flash-lite",
            "gemini-2.5-pro",
            "gemini-3-pro-preview",
        ];

        let mut last_error = None;

        for model in &models {
            info!("Attempting Gemini API with model: {}", model);
            match self.call_gemini_api(model, &prompt).await {
                Ok(mut data) => {
                    // Ensure video_url is set
                    if data.video_url.is_none() {
                        data.video_url = Some(video_url.to_string());
                    }
                    info!("Successfully got highlights from {}", model);
                    return Ok(data);
                }
                Err(e) => {
                    warn!("Failed with model {}: {}", model, e);
                    last_error = Some(e);
                }
            }
        }

        Err(last_error.unwrap_or_else(|| WorkerError::ai_failed("All Gemini models failed")))
    }

    /// Get video transcript using yt-dlp.
    async fn get_transcript(&self, video_url: &str, workdir: &Path) -> WorkerResult<String> {
        info!("Fetching transcript for {} using yt-dlp", video_url);

        let output_template = workdir.join("%(id)s");

        // Run yt-dlp to download subtitles
        let output = tokio::process::Command::new("yt-dlp")
            .args(&[
                "--write-auto-sub",
                "--write-sub",
                "--sub-lang", "en,en-US,en-GB",
                "--skip-download",
                "--sub-format", "vtt",
                "--output", &output_template.to_string_lossy(),
                video_url,
            ])
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
            .filter(|entry| {
                entry.path().extension().and_then(|s| s.to_str()) == Some("vtt")
            })
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

        // Parse VTT
        let transcript = self.parse_vtt(&content);

        // Save parsed transcript
        let transcript_path = workdir.join("transcript.txt");
        tokio::fs::write(&transcript_path, &transcript)
            .await
            .ok();

        // Cleanup VTT files
        for entry in vtt_files {
            tokio::fs::remove_file(entry.path()).await.ok();
        }

        Ok(transcript)
    }

    /// Parse VTT content into timestamped transcript.
    fn parse_vtt(&self, content: &str) -> String {
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

    /// Build prompt for Gemini.
    fn build_prompt(&self, base_prompt: &str, transcript: &str) -> String {
        format!(
            r#"{base_prompt}

IMPORTANT: You must strictly follow this output format.
Return ONLY a single JSON object with this schema:
{{
  "video_url": "URL",
  "video_title": "Actual title of the YouTube video",
  "highlights": [
    {{
      "id": 1,
      "title": "Viral Title",
      "start": "HH:MM:SS",
      "end": "HH:MM:SS",
      "duration": 0,
      "pad_before_seconds": 1.0,
      "pad_after_seconds": 1.5,
      "hook_category": "Category",
      "reason": "Why this is viral",
      "description": "Engaging social media caption with hashtags"
    }}
  ]
}}

Here is the TRANSCRIPT of the video with timestamps.
Use these exact timestamps for the 'start' and 'end' fields.

TRANSCRIPT:
{transcript}

Additional instructions:
- Return ONLY a single JSON object and nothing else.
- Ensure all timestamps are in "HH:MM:SS" or "HH:MM:SS.mmm" format.
- You MUST verify the quotes exist in the transcript provided above.
- Extract 3 to 10 viral segments that are 20-90 seconds long.
- Calculate duration in seconds for each highlight.
- Set pad_before_seconds to 1.0 and pad_after_seconds to 1.5 for natural clip boundaries.
"#
        )
    }

    /// Call Gemini API.
    async fn call_gemini_api(&self, model: &str, prompt: &str) -> WorkerResult<HighlightsResponse> {
        let url = format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
            model, self.api_key
        );

        let request = GeminiRequest {
            contents: vec![Content {
                parts: vec![Part {
                    text: prompt.to_string(),
                }],
            }],
            generation_config: GenerationConfig {
                response_mime_type: "application/json".to_string(),
            },
        };

        let response = self
            .client
            .post(&url)
            .json(&request)
            .send()
            .await
            .map_err(|e| WorkerError::ai_failed(format!("Gemini API request failed: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(WorkerError::ai_failed(format!(
                "Gemini API returned {}: {}",
                status, error_text
            )));
        }

        let gemini_response: GeminiResponse = response
            .json()
            .await
            .map_err(|e| WorkerError::ai_failed(format!("Failed to parse Gemini response: {}", e)))?;

        let text = gemini_response
            .candidates
            .first()
            .and_then(|c| c.content.parts.first())
            .map(|p| p.text.as_str())
            .ok_or_else(|| WorkerError::ai_failed("No content in Gemini response"))?;

        // Parse JSON, handling markdown code blocks
        let text = text.trim();
        let text = if text.starts_with("```json") {
            &text[7..]
        } else {
            text
        };
        let text = if text.ends_with("```") {
            &text[..text.len() - 3]
        } else {
            text
        };

        serde_json::from_str(text.trim())
            .map_err(|e| WorkerError::ai_failed(format!("Failed to parse highlights JSON: {}", e)))
    }
}
