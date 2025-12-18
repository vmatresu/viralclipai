//! Gemini AI client for generating more scenes.
//!
//! This is a simplified Gemini client for the API server to generate additional
//! scenes without requiring the full worker dependencies.

use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use crate::error::{ApiError, ApiResult};

/// Gemini API client for scene generation.
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
    pub highlights: Vec<AIHighlight>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct AIHighlight {
    pub id: u32,
    pub title: String,
    pub start: String,
    pub end: String,
    pub duration: u32,
    #[serde(default = "default_pad")]
    pub pad_before_seconds: f64,
    #[serde(default = "default_pad")]
    pub pad_after_seconds: f64,
    pub hook_category: Option<String>,
    pub reason: Option<String>,
    pub description: Option<String>,
}

fn default_pad() -> f64 {
    1.0
}

impl GeminiClient {
    /// Create a new Gemini client.
    pub fn new() -> ApiResult<Self> {
        let api_key = std::env::var("GEMINI_API_KEY").map_err(|_| {
            ApiError::internal("GEMINI_API_KEY not configured. Cannot generate scenes.")
        })?;

        Ok(Self {
            api_key,
            client: Client::new(),
        })
    }

    /// Generate more scenes based on existing transcript.
    pub async fn generate_more_scenes(
        &self,
        prompt: &str,
        video_url: &str,
    ) -> ApiResult<HighlightsResponse> {
        let models = vec![
            "gemini-2.5-flash",
            "gemini-2.5-flash-lite",
            "gemini-2.5-pro",
        ];

        let mut last_error = None;

        for model in &models {
            info!("Attempting Gemini API with model: {}", model);
            match self.call_gemini_api(model, prompt, video_url).await {
                Ok(mut data) => {
                    if data.video_url.is_none() {
                        data.video_url = Some(video_url.to_string());
                    }
                    info!("Successfully generated scenes from {}", model);
                    return Ok(data);
                }
                Err(e) => {
                    warn!("Failed with model {}: {:?}", model, e);
                    last_error = Some(e);
                }
            }
        }

        Err(last_error.unwrap_or_else(|| {
            ApiError::internal("All Gemini models failed. Please try again later.")
        }))
    }

    /// Call Gemini API.
    async fn call_gemini_api(
        &self,
        model: &str,
        prompt: &str,
        _video_url: &str,
    ) -> ApiResult<HighlightsResponse> {
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
            .map_err(|e| ApiError::internal(format!("Gemini API request failed: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(ApiError::internal(format!(
                "Gemini API returned {}: {}",
                status, error_text
            )));
        }

        let gemini_response: GeminiResponse = response.json().await.map_err(|e| {
            ApiError::internal(format!("Failed to parse Gemini response: {}", e))
        })?;

        let text = gemini_response
            .candidates
            .first()
            .and_then(|c| c.content.parts.first())
            .map(|p| p.text.as_str())
            .ok_or_else(|| ApiError::internal("No content in Gemini response"))?;

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
            .map_err(|e| ApiError::internal(format!("Failed to parse highlights JSON: {}", e)))
    }
}

/// Build the prompt for generating more scenes.
pub fn build_generate_more_prompt(
    admin_prompt: &str,
    user_prompt: &str,
    existing_scenes: &str,
    count: u32,
) -> String {
    let mut prompt = admin_prompt.to_string();

    if !user_prompt.is_empty() {
        prompt.push_str("\n\nADDITIONAL USER INSTRUCTIONS:\n");
        prompt.push_str(user_prompt);
    }

    if !existing_scenes.is_empty() {
        prompt.push_str("\n\n");
        prompt.push_str(existing_scenes);
    }

    prompt.push_str(&format!(
        r#"

IMPORTANT: Generate exactly {} NEW viral moments that are DIFFERENT from the existing scenes listed above.
- Do NOT repeat or overlap with existing scenes
- Find fresh, unique viral moments from other parts of the video
- Ensure at least 30 seconds gap between new scenes and existing ones
- Focus on variety - try different categories and themes

IMPORTANT: You must strictly follow this output format.
Return ONLY a single JSON object with this schema:
{{
  "video_url": "URL",
  "video_title": "Video title",
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

Return ONLY a JSON object with the 'highlights' array containing the new scenes."#,
        count
    ));

    prompt
}

/// Build context string for existing scenes.
pub fn build_existing_scenes_context(
    highlights: &[vclip_models::Highlight],
) -> String {
    if highlights.is_empty() {
        return String::new();
    }

    let mut context = String::from("EXISTING SCENES (do NOT overlap with these):\n");
    for h in highlights {
        context.push_str(&format!(
            "- Scene {}: \"{}\" ({} - {})",
            h.id, h.title, h.start, h.end
        ));
        if let Some(ref reason) = h.reason {
            context.push_str(&format!(" - {}", reason));
        }
        context.push('\n');
    }
    context
}

/// Get fallback base prompt for scene generation.
pub fn get_fallback_base_prompt() -> String {
    r#"You are a viral video expert. Your task is to identify the most engaging, viral-worthy moments from video transcripts.

For each viral moment, provide:
- A catchy, attention-grabbing title (suitable for TikTok/YouTube Shorts)
- Precise timestamps (start and end)
- A category (emotional, educational, controversial, inspirational, humorous, dramatic, surprising)
- A compelling reason why this moment would go viral
- A social media caption with relevant hashtags

Focus on moments with:
- Strong emotional reactions
- Surprising revelations or plot twists
- Controversial or debate-worthy statements
- Inspirational quotes or advice
- Genuine humor or comedic timing
- Dramatic tension or conflict resolution"#.to_string()
}
