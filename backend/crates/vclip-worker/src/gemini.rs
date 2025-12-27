//! Gemini AI client for video highlight extraction.
//!
//! This module provides integration with Google's Gemini API to analyze
//! video transcripts and extract viral highlights.

use std::path::Path;

use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

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
#[derive(Debug, Deserialize, Serialize)]
pub struct HighlightsResponse {
    pub video_url: Option<String>,
    pub video_title: Option<String>,
    pub highlights: Vec<Highlight>,
}

#[derive(Debug, Deserialize, Serialize)]
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
    1.0
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

        // Use cookies file if available for YouTube authentication (copy to writable location)
        let cookies_path = vclip_media::get_writable_cookies_path().await;
        let mut args = vec![
            "--verbose",
            "--remote-components",
            "ejs:github",
            "--print",
            "title",
            "--print",
            "webpage_url",
            "--no-download",
            "--no-playlist",
        ];

        let cookies_ref = cookies_path.as_deref();
        if let Some(cp) = cookies_ref {
            args.push("--cookies");
            args.push(cp);
        }
        args.push(video_url);

        // Create command but don't execute yet
        let mut child = tokio::process::Command::new("yt-dlp")
            .args(&args)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| WorkerError::ai_failed(format!("Failed to spawn yt-dlp: {}", e)))?;

        // Stream stdout and stderr in real-time
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();
        
        let mut stdout_reader = tokio::io::BufReader::new(stdout);
        let mut stderr_reader = tokio::io::BufReader::new(stderr);
        
        let mut stdout_lines = Vec::new();
        let mut stderr_lines = Vec::new();

        // Spawn tasks to read output
        let stdout_handle = tokio::spawn(async move {
            use tokio::io::AsyncBufReadExt;
            let mut lines = Vec::new();
            let mut line = String::new();
            while let Ok(n) = stdout_reader.read_line(&mut line).await {
                if n == 0 { break; }
                let trimmed = line.trim().to_string();
                if !trimmed.is_empty() {
                    debug!("yt-dlp stdout: {}", trimmed);
                    lines.push(trimmed);
                }
                line.clear();
            }
            lines
        });

        let stderr_handle = tokio::spawn(async move {
            use tokio::io::AsyncBufReadExt;
            let mut lines = Vec::new();
            let mut line = String::new();
            while let Ok(n) = stderr_reader.read_line(&mut line).await {
                if n == 0 { break; }
                let trimmed = line.trim().to_string();
                if !trimmed.is_empty() {
                    warn!("yt-dlp stderr: {}", trimmed);
                    lines.push(trimmed);
                }
                line.clear();
            }
            lines
        });

        // Wait for process to finish
        let status = child.wait().await
            .map_err(|e| WorkerError::ai_failed(format!("Failed to wait for yt-dlp: {}", e)))?;
            
        // Collect output
        stdout_lines = stdout_handle.await.unwrap_or_default();
        stderr_lines = stderr_handle.await.unwrap_or_default();

        if !status.success() {
            return Err(WorkerError::ai_failed(format!(
                "yt-dlp failed to get metadata: {}",
                stderr_lines.join("\n")
            )));
        }

        if stdout_lines.len() < 2 {
            return Err(WorkerError::ai_failed(
                format!("yt-dlp did not return expected metadata. Output: {:?}", stdout_lines)
            ));
        }

        let title = stdout_lines[0].trim().to_string();
        let canonical_url = stdout_lines[1].trim().to_string();

        if title.is_empty() || canonical_url.is_empty() {
            return Err(WorkerError::ai_failed(
                "yt-dlp returned empty title or URL".to_string(),
            ));
        }

        info!(
            "Got video metadata: title='{}', url='{}'",
            title, canonical_url
        );
        Ok((title, canonical_url))
    }

    /// Get transcript only (without calling Gemini).
    pub async fn get_transcript_only(
        &self,
        video_url: &str,
        workdir: &Path,
    ) -> WorkerResult<String> {
        crate::transcript::fetch_transcript(video_url, workdir).await
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
            "gemini-3-flash-preview",
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
          "pad_after_seconds": 1.0,
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
- Set pad_before_seconds to 1.0 and pad_after_seconds to 1.0 for natural clip boundaries.
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

        let gemini_response: GeminiResponse = response.json().await.map_err(|e| {
            WorkerError::ai_failed(format!("Failed to parse Gemini response: {}", e))
        })?;

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
