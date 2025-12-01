import os
import textwrap
import json
import logging
import subprocess
import re
import tempfile
from pathlib import Path
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

    def _get_transcript(self, video_url: str) -> str:
        """
        Fetches video transcript using yt-dlp (vtt format) and parses it.
        """
        logger.info(f"Fetching transcript for {video_url} using yt-dlp...")
        
        with tempfile.TemporaryDirectory() as tmpdirname:
            tmp_path = Path(tmpdirname)
            output_template = str(tmp_path / "% (id)s")
            
            # Command to download subtitles only
            cmd = [
                "yt-dlp",
                "--write-auto-sub",
                "--write-sub",
                "--sub-lang", "en,en-US,en-GB",
                "--skip-download",
                "--sub-format", "vtt",
                "--output", output_template,
                video_url
            ]
            
            try:
                # Capture output to avoid polluting logs unless error
                subprocess.run(cmd, check=True, capture_output=True, text=True)
            except subprocess.CalledProcessError as e:
                logger.error(f"yt-dlp subtitle download failed: {e.stderr}")
                raise RuntimeError(f"Failed to download transcript. ensure video has captions. Details: {e.stderr}") from e
            
            # Find the downloaded .vtt file
            vtt_files = list(tmp_path.glob("*.vtt"))
            if not vtt_files:
                # Try finding any sub file if specific language failed or name differs
                vtt_files = list(tmp_path.glob("*" ))
                logger.warning(f"No .vtt found. Files in temp: {[f.name for f in vtt_files]}")
                raise RuntimeError("No transcript file downloaded by yt-dlp.")
            
            vtt_file = vtt_files[0]
            content = vtt_file.read_text(encoding='utf-8', errors='ignore')
            
            return self._parse_vtt(content)

    def _parse_vtt(self, content: str) -> str:
        """
        Parses VTT content into a simple string with timestamps.
        """
        lines = content.splitlines()
        transcript_text = ""
        seen_lines = set()
        
        # Regex for VTT timestamp: 00:00:00.000 --> 00:00:05.000
        # Matches start timestamp group 1
        ts_pattern = re.compile(r"( \d{2}:\d{2}:\d{2})\.\d{3} -->.*")
        
        current_ts = "00:00:00"
        
        for line in lines:
            line = line.strip()
            # clean up tags like <c.colorCCCCCC> or <00:00:00.200>
            line = re.sub(r"<[^>]+>", "", line)
            
            if not line:
                continue
            if line == "WEBVTT":
                continue
            
            m = ts_pattern.match(line)
            if m:
                current_ts = m.group(1) # HH:MM:SS
                continue
            
            # Skip simple numbers (cue indices) if they appear alone
            if line.isdigit():
                continue
                
            # Skip metadata headers (contains :)
            if "-->" not in line and ":" in line and not re.match(r"^\s*\[.*?\]", line):
                 # Heuristic to skip header lines like "Language: en" if they slipped through
                 # But be careful not to skip dialogue. 
                 # Better heuristic: VTT headers usually usually at top. 
                 # We'll assume lines after WEBVTT and before first TS are header? 
                 # For simplicity, let's just accept text.
                 pass

            # Deduplicate adjacent lines (common in auto-caps rolling window)
            if line and line not in seen_lines:
                # Reset seen_lines occasionally to allow repetition of phrases later in video?
                # For summary, global unique set is risky if "Yes" is said twice.
                # Let's just dedupe immediate repetition or use a sliding window.
                # Simple approach: just add it.
                # Actually, auto-subs often repeat the same line with slight additions.
                # A simple dedupe logic:
                if len(transcript_text) > 0 and line in transcript_text[-200:]:
                     continue

                transcript_text += f"[{current_ts}] {line}\n"
                # seen_lines.add(line) # Don't use global seen, standard conversation has repeats.
        
        return transcript_text

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
            """
        ).strip()

    def get_highlights(self, base_prompt: str, video_url: str) -> Dict[str, Any]:
        # 1. Fetch Transcript
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
