import os
import textwrap
import json
import logging
from typing import Dict, Any

import google.generativeai as genai
from youtube_transcript_api import YouTubeTranscriptApi

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

    # NEW HELPER METHOD
    def _get_transcript(self, video_url: str) -> str:
        try:
            # Extract video ID
            if "v=" in video_url:
                video_id = video_url.split("v=")[-1].split("&")[0]
            elif "youtu.be" in video_url:
                video_id = video_url.split("/")[-1]
            else:
                video_id = video_url.split("/")[-1]

            # Try list_transcripts first to find English or auto-generated English
            try:
                transcript_list = YouTubeTranscriptApi.list_transcripts(video_id)
                
                # Priority: Manually created English -> Generated English -> Any English -> First available
                try:
                    transcript = transcript_list.find_transcript(['en'])
                except:
                    try:
                        transcript = transcript_list.find_generated_transcript(['en'])
                    except:
                        # Fallback: iterate and find any english code 'en-*'
                        transcript = None
                        for t in transcript_list:
                            if t.language_code.startswith('en'):
                                transcript = t
                                break
                        if not transcript:
                            # Last resort: take the first available one
                            transcript = next(iter(transcript_list))

                transcript_data = transcript.fetch()
                
            except Exception as list_error:
                logger.warning(f"list_transcripts failed ({list_error}), trying direct get_transcript fallback...")
                # Fallback to the static method (which defaults to 'en')
                transcript_data = YouTubeTranscriptApi.get_transcript(video_id)

            # Format as a string with timestamps for the model
            formatted_transcript = ""
            for entry in transcript_data:
                start = entry['start']
                text = entry['text']
                # Format timestamp as HH:MM:SS
                m, s = divmod(start, 60)
                h, m = divmod(m, 60)
                time_str = "{:02d}:{:02d}:{:02d}".format(int(h), int(m), int(s))
                formatted_transcript += f"[{time_str}] {text}\n"
            
            return formatted_transcript
        except Exception as e:
            logger.error(f"Error fetching transcript: {e}")
            raise RuntimeError(f"Could not fetch transcript for {video_url}. Is it available/captioned? Details: {e}")

    def build_prompt(self, base_prompt: str, transcript_text: str) -> str:
        return textwrap.dedent(
            f"""
            {base_prompt}

            Here is the TRANSCRIPT of the video with timestamps. 
            Use these exact timestamps for the 'start' and 'end' fields.
            
            TRANSCRIPT:
            {transcript_text}

            Additional instructions:
            - Return ONLY a single JSON object and nothing else.
            - Ensure all timestamps are in "HH:MM:SS" format.
            - You MUST verify the quotes exist in the transcript provided above.
            """).strip()

    def get_highlights(self, base_prompt: str, video_url: str) -> Dict[str, Any]:
        # 1. Fetch Transcript first
        logger.info(f"Fetching transcript for {video_url}")
        transcript_text = self._get_transcript(video_url)

        # 2. Build prompt with transcript
        prompt = self.build_prompt(base_prompt, transcript_text)
        last_error = None

        for model_name in self.FALLBACK_MODELS:
            logger.info(f"Attempting with model: {model_name}")
            try:
                model = genai.GenerativeModel(model_name)
                # Increase token limit for long transcripts
                response = model.generate_content(prompt, generation_config={"response_mime_type": "application/json"})
                
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
