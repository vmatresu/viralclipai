import json
import subprocess
import argparse
from pathlib import Path
from datetime import datetime, timedelta

INPUT_JSON = "highlights.json"
OUTPUT_DIR = "clips"
VIDEO_FILE = "source.mp4"

# --- Configuration ---
# Available styles:
# 1. "split": Left half top, Right half bottom (Portrait 1080x1920).
# 2. "left_focus": Zoom/crop to the left half only, full-screen 9:16.
# 3. "right_focus": Zoom/crop to the right half only, full-screen 9:16.
AVAILABLE_STYLES = ["split", "left_focus", "right_focus"]

def ensure_dirs():
    Path(OUTPUT_DIR).mkdir(exist_ok=True)

def load_highlights(json_path):
    with open(json_path, "r") as f:
        data = json.load(f)
    url = data.get("video_url", "https://www.youtube.com/watch?v=pNrLFiJiIHs")
    highlights = data.get("highlights", [])
    highlights = sorted(
        highlights,
        key=lambda h: (h.get("priority", 9999), h.get("id", 9999))
    )
    return url, highlights

def download_video(url: str):
    # Check if file exists and is large enough (heuristic for video content)
    if Path(VIDEO_FILE).exists():
        if Path(VIDEO_FILE).stat().st_size > 50 * 1024 * 1024:  # > 50MB
            print(f"[info] Using existing {VIDEO_FILE}")
            return
        print(f"[info] Existing file {VIDEO_FILE} seems too small (audio only?), re-downloading.")
        Path(VIDEO_FILE).unlink(missing_ok=True)

    print(f"[info] Downloading video from {url}")
    subprocess.run(
        [
            "yt-dlp",
            "--remote-components", "ejs:github",
            "-f", "bestvideo[ext=mp4]+bestaudio[ext=m4a]/best[ext=mp4]/best",
            "-o", VIDEO_FILE,
            url,
        ],
        check=True
    )

def parse_time(t_str):
    # Handle formats like "00:19:24.0" or "00:19:24"
    fmt = "%H:%M:%S.%f" if "." in t_str else "%H:%M:%S"
    return datetime.strptime(t_str, fmt)

def format_time(dt):
    # Return string in HH:MM:SS.mmm format
    return dt.strftime("%H:%M:%S.%f")[:-3]

def build_vf_filter(style: str) -> str:
    if style == "split":
        # Portrait 1080x1920: top = left half, bottom = right half
        # Adjusted left crop to 910 width to avoid seeing the right-side person
        # Using scale=1080:-2,crop=1080:960 to preserve aspect ratio (center crop vertically)
        return (
            "scale=1920:-2,split=2[full][full2];"
            "[full]crop=910:1080:0:0[left];"       
            "[full2]crop=960:1080:960:0[right];"   
            "[left]scale=1080:-2,crop=1080:960[left_scaled];"
            "[right]scale=1080:-2,crop=1080:960[right_scaled];"
            "[left_scaled][right_scaled]vstack=inputs=2"
        )

    if style == "left_focus":
        # Focus only on left half -> crop, then scale to 9:16
        return (
            "scale=1920:-2,"
            "crop=910:1080:0:0,"         # adjusted left half width
            "scale=1080:1920:force_original_aspect_ratio=decrease,"
            "pad=1080:1920:(ow-iw)/2:(oh-ih)/2"
        )

    if style == "right_focus":
        # Focus only on right half -> crop, then scale to 9:16
        return (
            "scale=1920:-2,"
            "crop=960:1080:960:0,"       # right half
            "scale=1080:1920:force_original_aspect_ratio=decrease,"
            "pad=1080:1920:(ow-iw)/2:(oh-ih)/2"
        )

    # fallback: simple center crop
    return "scale=-2:1920,crop=1080:1920"

def run_ffmpeg_clip(start_str: str, end_str: str, out_path: Path, style: str):
    
    # Padding logic
    try:
        t_start = parse_time(start_str)
        t_end = parse_time(end_str)
        
        # Pad -1s / +2s
        t_start = max(t_start - timedelta(seconds=1), datetime(1900, 1, 1)) # Clamp to 0
        t_end = t_end + timedelta(seconds=2)
        
        duration = (t_end - t_start).total_seconds()
        start_seconds = (t_start - datetime(1900, 1, 1)).total_seconds()
        
    except ValueError as e:
        print(f"[error] Time parsing failed: {e}")
        return

    vf_filter = build_vf_filter(style)

    cmd = [
        "ffmpeg",
        "-y",
        "-ss", f"{start_seconds:.3f}",
        "-t", f"{duration:.3f}",
        "-i", VIDEO_FILE,
        "-vf", vf_filter,
        "-c:v", "libx264",
        "-preset", "fast",
        "-crf", "18",
        "-c:a", "aac",
        "-b:a", "128k",
        str(out_path)
    ]
    # print(f"[ffmpeg] Clipping ({style}): {start_seconds}s + {duration}s") 
    subprocess.run(cmd, check=True)

def sanitize_filename(text: str, max_len: int = 60) -> str:
    safe = "".join(c if c.isalnum() or c in "-_" else "_" for c in text)
    return safe[:max_len] if safe else "clip"

def main():
    parser = argparse.ArgumentParser(description="Clip video from YouTube based on highlights.")
    parser.add_argument("--url", help="YouTube Video URL to override highlights.json", default=None)
    parser.add_argument(
        "--style", 
        help="Output style. Options: 'split', 'left_focus', 'right_focus', or 'all'. Default is 'split'.", 
        default="split",
        choices=AVAILABLE_STYLES + ["all"]
    )
    args = parser.parse_args()

    ensure_dirs()
    json_url, highlights = load_highlights(INPUT_JSON)
    
    # Prioritize CLI URL argument
    video_url = args.url if args.url else json_url

    if not highlights:
        print("[error] No highlights found in JSON.")
        return

    download_video(video_url)

    # Determine which styles to process
    if args.style == "all":
        styles_to_process = AVAILABLE_STYLES
    else:
        styles_to_process = [args.style]

    for h in highlights:
        clip_id = h.get("id")
        title = h.get("title", f"clip_{clip_id}")
        start = h["start"]
        end = h["end"]
        prio = h.get("priority", 9999)
        safe_title = sanitize_filename(title)

        print(f"\n[clip] P{prio} ID{clip_id} | {title} | {start} -> {end} (Padded +/- 1s)")

        for style in styles_to_process:
            # Append style to filename to distinguish them
            filename = f"P{prio:02d}_ID{clip_id:02d}_{safe_title}_{style}.mp4"
            out_path = Path(OUTPUT_DIR) / filename
            
            print(f"  > Rendering style: {style}")
            run_ffmpeg_clip(start, end, out_path, style=style)

    print(f"\n[done] All clips rendered into {OUTPUT_DIR}")

if __name__ == "__main__":
    main()
