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
        "gemini-2.5-flash",
        "gemini-2.5-flash-lite",
        "gemini-2.5-pro",
        "gemini-3-pro-preview",
    ]

    def __init__(self, api_key: str | None = None):
        api_key = api_key or os.getenv("GEMINI_API_KEY")
        if not api_key:
            raise RuntimeError("GEMINI_API_KEY is not set in environment.")
        genai.configure(api_key=api_key)

    def _get_transcript(self, video_url: str, output_dir: Path = None) -> str:
        """
        Fetches video transcript using yt-dlp (vtt format) and parses it.
        Optionally saves the raw VTT to output_dir.
        """
        logger.info(f"Fetching transcript for {video_url} using yt-dlp...")
        
        # Use a provided directory or a temporary one
        target_dir = output_dir if output_dir else Path(tempfile.mkdtemp())
        output_template = str(target_dir / "%(id)s")
        
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
        
                vtt_files = list(target_dir.glob("*.vtt"))
        
                if not vtt_files:
        
                    # Try finding any sub file if specific language failed or name differs
        
                    vtt_files = list(target_dir.glob("*"))
        
                    logger.warning(f"No .vtt found. Files in target: {[f.name for f in vtt_files]}")
        
                    raise RuntimeError("No transcript file downloaded by yt-dlp.")
        
        # Prefer English if multiple
        vtt_file = vtt_files[0]
        for f in vtt_files:
            if ".en" in f.name:
                vtt_file = f
                break

        content = vtt_file.read_text(encoding='utf-8', errors='ignore')
        
        # Save parsed transcript for debugging/reference if output_dir is provided
        parsed_transcript = self._parse_vtt(content)
        if output_dir:
            transcript_path = output_dir / "transcript.txt"
            transcript_path.write_text(parsed_transcript, encoding="utf-8")
            logger.info(f"Saved parsed transcript to {transcript_path}")

        # Cleanup raw VTT files
        for f in vtt_files:
            try:
                f.unlink()
                logger.debug(f"Deleted raw VTT file: {f}")
            except Exception as e:
                logger.warning(f"Failed to delete VTT file {f}: {e}")

        # If we created a temporary directory, clean it up
        if not output_dir:
            import shutil
            try:
                shutil.rmtree(target_dir)
            except Exception as e:
                logger.warning(f"Failed to remove temp dir {target_dir}: {e}")

        return parsed_transcript

    def _parse_vtt(self, content: str) -> str:
        """
        Parses VTT content into a simple string with timestamps.
        """
        lines = content.splitlines()
        transcript_text = ""
        
        # Regex for VTT timestamp: 00:00:00.000 --> 00:00:05.000
        # Matches start timestamp group 1
        # Some VTTs use 00:00.000 (MM:SS.mmm) format too
        ts_pattern = re.compile(r"((?:\d{2}:)?\d{2}:\d{2}\.\d{3}) -->.*")
        
        current_ts = "00:00:00"
        buffer_text = ""
        
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
                ts = m.group(1)
                # Normalize to HH:MM:SS
                if len(ts.split(":")) == 2:
                    ts = "00:" + ts
                current_ts = ts.split(".")[0] # Drop milliseconds for readability in prompt
                continue
            
            # Skip metadata headers (contains :)
            # Improved heuristic: skip lines starting with typical VTT headers or Metadata
            if "-->" not in line and ":" in line:
                # Check for Metadata-like lines "Key: Value"
                if re.match(r"^(Kind|Language|Style|Region):", line, re.IGNORECASE):
                    continue
                # Fallback for other headers if they don't look like dialogue (dialogue usually doesn't start with Key:)
                # But be careful with "Person: Hello"
                if not re.match(r"^[\[.*?\]]", line) and not re.match(r"^<.*?<", line):
                     # If it's not a timestamped line (which we shouldn't be processing here anyway)
                     # and not a tag.
                     pass

            # Skip cues/numbers
            if line.isdigit():
                continue

            # De-duplication for rolling captions
            if line != buffer_text:
                transcript_text += f"[{current_ts}] {line}\n"
                buffer_text = line
        
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

    def get_highlights(self, base_prompt: str, video_url: str, workdir: Path = None) -> Dict[str, Any]:
        # 1. Fetch Transcript (pass workdir to save it)
        transcript_text = self._get_transcript(video_url, output_dir=workdir)
        
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
