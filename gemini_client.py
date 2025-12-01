import os
import textwrap
import json
from typing import Dict, Any

import google.generativeai as genai


class GeminiClient:
    """
    Wrapper around the Gemini API with fallback support for multiple models.
    """

    # Prioritized list of models to try
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

    def build_prompt(self, base_prompt: str, video_url: str) -> str:
        """
        Combine your role prompt with the video URL into a single instruction.
        """
        return textwrap.dedent(
            f"""
            {base_prompt}

            Additional instructions:
            - The video to analyze is: {video_url}
            - Return ONLY a single JSON object and nothing else.
            - Ensure all timestamps are in "HH:MM:SS.s" format.
            - Include title, summary, reason and description for each highlight.
            """
        ).strip()

    def get_highlights(self, base_prompt: str, video_url: str) -> Dict[str, Any]:
        """
        Call Gemini and parse the JSON response into a Python dict.
        Iterates through FALLBACK_MODELS until successful.
        """
        prompt = self.build_prompt(base_prompt, video_url)
        
        last_error = None

        for model_name in self.FALLBACK_MODELS:
            print(f"[Gemini] Attempting with model: {model_name}")
            try:
                model = genai.GenerativeModel(model_name)
                response = model.generate_content(prompt)
                
                # Parse response
                text = response.text.strip()
                
                # Clean up markdown code blocks if present
                if text.startswith("```json"):
                    text = text[7:]
                if text.endswith("```"):
                    text = text[:-3]
                text = text.strip()

                data = json.loads(text)
                
                if "video_url" not in data:
                    data["video_url"] = video_url
                
                print(f"[Gemini] Success with {model_name}")
                return data

            except Exception as e:
                print(f"[Gemini] Failed with {model_name}: {e}")
                last_error = e
                continue

        raise RuntimeError(f"All Gemini models failed. Last error: {last_error}")