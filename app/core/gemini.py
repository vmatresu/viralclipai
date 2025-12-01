import os
import textwrap
import json
import logging
from typing import Dict, Any

import google.generativeai as genai

logger = logging.getLogger(__name__)

class GeminiClient:
    """
    Wrapper around the Gemini API with fallback support for multiple models.
    """

    FALLBACK_MODELS = [
        "gemini-3-pro-preview",
        "gemini-2.5-flash",
        "gemini-2.5-flash-lite",
        "gemini-2.5-pro",
    ]

    def __init__(self, api_key: str | None = None):
        api_key = api_key or os.getenv("GEMINI_API_KEY")
        if not api_key:
            raise RuntimeError("GEMINI_API_KEY is not set in environment.")
        genai.configure(api_key=api_key)

    def build_prompt(self, base_prompt: str) -> str:
        return textwrap.dedent(
            f"""
            {base_prompt}

            Additional instructions:
            - Return ONLY a single JSON object and nothing else.
            - Ensure all timestamps are in "HH:MM:SS" format.
            - Analyze the video content provided directly.
            """
        ).strip()

    def get_highlights(self, base_prompt: str, video_url: str) -> Dict[str, Any]:
        prompt = self.build_prompt(base_prompt)
        
        # Construct the content payload with the direct YouTube URL
        # This relies on the model's ability to process the URL or a specific API feature
        contents = [
            {
                "parts": [
                    {
                        "file_data": {
                            "mime_type": "video/mp4",
                            "file_uri": video_url
                        }
                    },
                    {
                        "text": prompt
                    }
                ]
            }
        ]

        last_error = None

        for model_name in self.FALLBACK_MODELS:
            logger.info(f"Attempting with model: {model_name}")
            try:
                model = genai.GenerativeModel(model_name)
                # Increase token limit for long video context if needed
                response = model.generate_content(contents, generation_config={"response_mime_type": "application/json"})
                
                text = response.text.strip()
                if text.startswith("```json"):
                    text = text[7:]
                if text.endswith("```"):
                    text = text[:-3]
                text = text.strip()

                data = json.loads(text)
                
                if "video_url" not in data:
                    data["video_url"] = video_url
                
                logger.info(f"Success with {model_name}")
                return data

            except Exception as e:
                logger.warning(f"Failed with {model_name}: {e}")
                last_error = e
                continue

        raise RuntimeError(f"All Gemini models failed. Last error: {last_error}")