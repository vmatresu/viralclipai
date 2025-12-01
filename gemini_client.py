import os
import textwrap
from typing import Dict, Any

import google.generativeai as genai


class GeminiClient:
    """
    Thin wrapper around the Gemini API.
    Expects the model to return a single JSON object following your schema.
    """

    def __init__(self, api_key: str | None = None, model_name: str = "gemini-1.5-pro"):
        api_key = api_key or os.getenv("GEMINI_API_KEY")
        if not api_key:
            raise RuntimeError("GEMINI_API_KEY is not set in environment.")
        genai.configure(api_key=api_key)
        self.model = genai.GenerativeModel(model_name)

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
            """).strip()

    def get_highlights(self, base_prompt: str, video_url: str) -> Dict[str, Any]:
        """
        Call Gemini and parse the JSON response into a Python dict.
        """
        prompt = self.build_prompt(base_prompt, video_url)
        response = self.model.generate_content(prompt)
        text = response.text.strip()

        # Model should return JSON only. Parse safely.
        import json

        try:
            # Clean up markdown code blocks if present
            if text.startswith("```json"):
                text = text[7:]
            if text.endswith("```"):
                text = text[:-3]
            text = text.strip()
            
            data = json.loads(text)
        except json.JSONDecodeError as e:
            raise RuntimeError(f"Gemini did not return valid JSON: {e}\nRaw text:\n{text}") from e

        if "video_url" not in data:
            data["video_url"] = video_url

        return data
