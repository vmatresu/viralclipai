import logging
from datetime import datetime
from pathlib import Path
from typing import List
from datetime import datetime

from fastapi import FastAPI, WebSocket, WebSocketDisconnect, Request
from fastapi.responses import HTMLResponse, FileResponse, RedirectResponse
from fastapi.staticfiles import StaticFiles

import uvicorn

from gemini_client import GeminiClient
import clipper  # Import the modified clipper module directly

# --- Logging Configuration ---
logging.basicConfig(
    level=logging.DEBUG,
    format="%(asctime)s [%(levelname)s] %(message)s",
    handlers=[
        logging.FileHandler("debug.log"),
        logging.StreamHandler()
    ]
)

# Separate error logger
error_logger = logging.getLogger("error_logger")
error_logger.setLevel(logging.ERROR)
error_handler = logging.FileHandler("error.log")
error_handler.setFormatter(logging.Formatter("%(asctime)s [%(levelname)s] %(message)s"))
error_logger.addHandler(error_handler)

# Main logger
logger = logging.getLogger("app_logger")

BASE_DIR = Path(__file__).parent.resolve()
VIDEOS_DIR = BASE_DIR / "videos"
PROMPT_PATH = BASE_DIR / "prompt.txt"

def extract_youtube_id(url: str) -> str:
    import urllib.parse as up
    parsed = up.urlparse(url)
    if parsed.netloc in {"youtu.be"}:
        return parsed.path.lstrip("/")
    if "youtube.com" in parsed.netloc:
        qs = up.parse_qs(parsed.query)
        if "v" in qs:
            return qs["v"][0]
        parts = [p for p in parsed.path.split("/") if p]
        if parts and parts[0] in {"shorts", "embed"} and len(parts) > 1:
            return parts[1]
    return "video_" + str(abs(hash(url)))

def create_video_workdir(youtube_id: str) -> Path:
    workdir = VIDEOS_DIR / youtube_id
    workdir.mkdir(parents=True, exist_ok=True)
    return workdir

async def process_video_workflow(websocket: WebSocket, url: str, style: str):
    """
    Orchestrates the full workflow, yielding status updates to the websocket.
    """
    try:
        if not PROMPT_PATH.exists():
            raise RuntimeError(f"prompt.txt not found at {PROMPT_PATH}")

        base_prompt = PROMPT_PATH.read_text(encoding="utf-8")
        youtube_id = extract_youtube_id(url)
        workdir = create_video_workdir(youtube_id)
        
        await websocket.send_json({"type": "log", "message": f"🚀 Starting job for Video ID: {youtube_id}"})
        await websocket.send_json({"type": "progress", "value": 10})

        # 1. Gemini Analysis
        await websocket.send_json({"type": "log", "message": "🤖 Asking Gemini to analyze video (this takes a moment)..."})
        
        # Run synchronous Gemini call in thread
        client = GeminiClient()
        data = await asyncio.to_thread(client.get_highlights, base_prompt, url)
        
        await websocket.send_json({"type": "log", "message": "✅ Gemini analysis complete."})
        await websocket.send_json({"type": "progress", "value": 30})

        # Ensure highlights have IDs
        data["video_url"] = url
        highlights = data.get("highlights", [])
        for idx, h in enumerate(highlights, start=1):
            h.setdefault("id", idx)
            h.setdefault("priority", idx)
            h.setdefault("title", f"Clip {idx}")
        
        # Write highlights.json
        highlights_path = workdir / "highlights.json"
        with open(highlights_path, "w", encoding="utf-8") as f:
            json.dump(data, f, indent=2, ensure_ascii=False)

        # 2. Download Video
        video_file = workdir / "source.mp4"
        clips_dir = clipper.ensure_dirs(workdir)
        
        await websocket.send_json({"type": "log", "message": "📥 Downloading video with yt-dlp..."})
        
        # Run download in thread
        await asyncio.to_thread(clipper.download_video, url, video_file)
        
        await websocket.send_json({"type": "log", "message": "✅ Download complete."})
        await websocket.send_json({"type": "progress", "value": 50})

        # 3. Clipping
        styles_to_process = clipper.AVAILABLE_STYLES if style == "all" else [style]
        total_clips = len(highlights) * len(styles_to_process)
        completed_clips = 0

        for h in highlights:
            clip_id = h.get("id")
            title = h.get("title", f"clip_{clip_id}")
            start = h["start"]
            end = h["end"]
            prio = h.get("priority", 99)
            safe_title = clipper.sanitize_filename(title)

            for s in styles_to_process:
                filename = f"clip_{prio:02d}_{clip_id:02d}_{safe_title}_{s}.mp4"
                out_path = clips_dir / filename
                
                await websocket.send_json({"type": "log", "message": f"✂️ Rendering clip: {title} ({s})"})
                
                # Run ffmpeg in thread
                await asyncio.to_thread(clipper.run_ffmpeg_clip, start, end, out_path, s, video_file)
                
                completed_clips += 1
                # Progress mapping from 50% to 90%
                progress = 50 + int((completed_clips / total_clips) * 40)
                await websocket.send_json({"type": "progress", "value": progress})

        # 4. Cleanup
        if video_file.exists():
            video_file.unlink()
            await websocket.send_json({"type": "log", "message": "🧹 Cleaned up source video file."})

        await websocket.send_json({"type": "progress", "value": 100})
        await websocket.send_json({"type": "log", "message": "✨ All done!"})
        await websocket.send_json({"type": "done", "videoId": youtube_id})

    except Exception as e:
        import traceback
        trace = traceback.format_exc()
        print(f"Error processing video: {e}")
        print(trace)
        await websocket.send_json({"type": "error", "message": str(e), "details": trace})


# ---------------- Web UI ---------------- #

app = FastAPI(title="YT Gemini Clipper")

@app.get("/", response_class=HTMLResponse)
async def index(request: Request):
    html = """
    <!DOCTYPE html>
    <html lang="en" class="dark">
    <head>
        <meta charset="UTF-8">
        <meta name="viewport" content="width=device-width, initial-scale=1.0">
        <title>YT Gemini Clipper</title>
        <script src="https://cdn.tailwindcss.com"></script>
        <script>
            tailwind.config = {
                darkMode: 'class',
                theme: {
                    extend: {
                        colors: {
                            gray: {
                                900: '#121212',
                                800: '#1e1e1e',
                                700: '#2d2d2d',
                            }
                        }
                    }
                }
            }
        </script>
        <style>
            /* Custom Scrollbar */
            ::-webkit-scrollbar { width: 8px; }
            ::-webkit-scrollbar-track { background: #1e1e1e; }
            ::-webkit-scrollbar-thumb { background: #4a4a4a; border-radius: 4px; }
            ::-webkit-scrollbar-thumb:hover { background: #555; }
            
            .glass {
                background: rgba(30, 30, 30, 0.7);
                backdrop-filter: blur(10px);
                border: 1px solid rgba(255, 255, 255, 0.1);
            }
        </style>
    </head>
    <body class="bg-gray-900 text-gray-100 min-h-screen font-sans antialiased selection:bg-blue-500 selection:text-white">

        <!-- Navbar -->
        <nav class="glass fixed top-0 w-full z-50 border-b border-gray-700">
            <div class="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8">
                <div class="flex items-center justify-between h-16">
                    <div class="flex items-center gap-3">
                        <div class="w-8 h-8 bg-gradient-to-br from-blue-500 to-purple-600 rounded-lg flex items-center justify-center text-white font-bold text-xl">✂️</div>
                        <a href="/" class="text-xl font-bold bg-clip-text text-transparent bg-gradient-to-r from-blue-400 to-purple-500 hover:opacity-80 transition-opacity">
                            YT Gemini Clipper
                        </a>
                    </div>
                    <div>
                        <a href="/history" class="text-gray-300 hover:text-white transition-colors flex items-center gap-2 px-3 py-2 rounded-md hover:bg-gray-800">
                            <span>📜</span>
                            <span class="hidden sm:inline">History</span>
                        </a>
                    </div>
                </div>
            </div>
        </nav>

        <!-- Main Content -->
        <main class="max-w-5xl mx-auto px-4 pt-24 pb-12 space-y-8">
            
            <!-- Hero / Input Section -->
            <section id="inputSection" class="glass rounded-2xl p-8 shadow-2xl animate-fade-in-up">
                <form id="processForm" class="space-y-6">
                    <div class="space-y-2">
                        <label class="text-sm font-medium text-gray-400 uppercase tracking-wider">YouTube Source URL</label>
                        <div class="relative">
                            <input type="text" id="urlInput" placeholder="https://www.youtube.com/watch?v=..." 
                                class="w-full bg-gray-800 border border-gray-700 rounded-xl px-5 py-4 text-lg focus:ring-2 focus:ring-blue-500 focus:border-transparent outline-none transition-all placeholder-gray-600" required>
                            <div class="absolute right-4 top-1/2 transform -translate-y-1/2 text-gray-500">
                                <svg class="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 10l4.553-2.276A1 1 0 0121 8.618v6.764a1 1 0 01-1.447.894L15 14M5 18h8a2 2 0 002-2V8a2 2 0 00-2-2H5a2 2 0 00-2 2v8a2 2 0 002 2z"></path></svg>
                            </div>
                        </div>
                    </div>

                    <div class="space-y-2">
                        <label class="text-sm font-medium text-gray-400 uppercase tracking-wider">Output Style</label>
                        <div class="grid grid-cols-2 md:grid-cols-4 gap-4">
                            <label class="cursor-pointer">
                                <input type="radio" name="style" value="split" class="peer sr-only" checked>
                                <div class="p-4 rounded-xl bg-gray-800 border border-gray-700 peer-checked:border-blue-500 peer-checked:bg-blue-900/20 transition-all text-center hover:bg-gray-750">
                                    <span class="font-medium">Split View</span>
                                    <span class="block text-xs text-gray-500 mt-1">Top/Bottom</span>
                                </div>
                            </label>
                            <label class="cursor-pointer">
                                <input type="radio" name="style" value="left_focus" class="peer sr-only">
                                <div class="p-4 rounded-xl bg-gray-800 border border-gray-700 peer-checked:border-blue-500 peer-checked:bg-blue-900/20 transition-all text-center hover:bg-gray-750">
                                    <span class="font-medium">Left Focus</span>
                                    <span class="block text-xs text-gray-500 mt-1">Full Height</span>
                                </div>
                            </label>
                            <label class="cursor-pointer">
                                <input type="radio" name="style" value="right_focus" class="peer sr-only">
                                <div class="p-4 rounded-xl bg-gray-800 border border-gray-700 peer-checked:border-blue-500 peer-checked:bg-blue-900/20 transition-all text-center hover:bg-gray-750">
                                    <span class="font-medium">Right Focus</span>
                                    <span class="block text-xs text-gray-500 mt-1">Full Height</span>
                                </div>
                            </label>
                            <label class="cursor-pointer">
                                <input type="radio" name="style" value="all" class="peer sr-only">
                                <div class="p-4 rounded-xl bg-gray-800 border border-gray-700 peer-checked:border-blue-500 peer-checked:bg-blue-900/20 transition-all text-center hover:bg-gray-750">
                                    <span class="font-medium">All Styles</span>
                                    <span class="block text-xs text-gray-500 mt-1">Generate All</span>
                                </div>
                            </label>
                        </div>
                    </div>

                    <button type="submit" id="submitBtn" class="w-full py-4 bg-gradient-to-r from-blue-600 to-purple-600 hover:from-blue-500 hover:to-purple-500 rounded-xl font-bold text-lg shadow-lg transform transition-all active:scale-[0.98] flex justify-center items-center gap-2">
                        <span>🚀 Launch Processor</span>
                    </button>
                </form>
            </section>

            <!-- Processing Status (Hidden by default) -->
            <section id="statusSection" class="hidden space-y-6">
                <div class="glass rounded-2xl p-6 border-l-4 border-blue-500">
                    <h3 class="text-xl font-bold mb-4 flex items-center gap-2">
                        <span class="animate-spin">⚙️</span> Processing Video...
                    </h3>
                    
                    <!-- Progress Bar -->
                    <div class="w-full bg-gray-700 rounded-full h-4 mb-6 overflow-hidden">
                        <div id="progressBar" class="bg-gradient-to-r from-blue-500 to-purple-500 h-4 rounded-full transition-all duration-500 ease-out" style="width: 0%"></div>
                    </div>

                    <!-- Console Log -->
                    <div id="consoleLog" class="bg-black/50 rounded-xl p-4 font-mono text-sm text-green-400 h-64 overflow-y-auto border border-gray-800 space-y-1">
                        <div class="text-gray-500 italic">Waiting for task...</div>
                    </div>
                </div>
            </section>

            <!-- Error Display (Hidden by default) -->
            <section id="errorSection" class="hidden">
                <div class="glass rounded-2xl p-6 border-l-4 border-red-500 bg-red-900/10">
                    <h3 class="text-xl font-bold text-red-400 mb-2">❌ Processing Failed</h3>
                    <p id="errorMessage" class="text-gray-300 mb-4">An unexpected error occurred.</p>
                    <pre id="errorDetails" class="bg-black/50 p-4 rounded-lg text-xs text-red-300 overflow-x-auto whitespace-pre-wrap"></pre>
                </div>
            </section>

            <!-- Results Section (Hidden by default) -->
            <section id="resultsSection" class="hidden space-y-6">
                <div class="flex items-center justify-between">
                    <h2 class="text-2xl font-bold text-white">🎉 Results</h2>
                    <button onclick="window.location.href='/'" class="text-sm text-blue-400 hover:text-blue-300 hover:underline">Process Another Video</button>
                </div>
                
                <div id="clipsGrid" class="grid grid-cols-1 md:grid-cols-2 gap-6">
                    <!-- Clip cards will be injected here -->
                </div>
            </section>

        </main>

        <script>
            const form = document.getElementById('processForm');
            const statusSection = document.getElementById('statusSection');
            const errorSection = document.getElementById('errorSection');
            const resultsSection = document.getElementById('resultsSection');
            const inputSection = document.getElementById('inputSection');
            const consoleLog = document.getElementById('consoleLog');
            const progressBar = document.getElementById('progressBar');
            const submitBtn = document.getElementById('submitBtn');

            function log(msg, type = 'info') {
                const div = document.createElement('div');
                div.textContent = `> ${msg}`;
                if (type === 'error') div.className = 'text-red-400';
                if (type === 'success') div.className = 'text-blue-400';
                consoleLog.appendChild(div);
                consoleLog.scrollTop = consoleLog.scrollHeight;
            }

            form.addEventListener('submit', async (e) => {
                e.preventDefault();
                
                // Reset UI
                statusSection.classList.remove('hidden');
                errorSection.classList.add('hidden');
                resultsSection.classList.add('hidden');
                submitBtn.disabled = true;
                submitBtn.classList.add('opacity-50', 'cursor-not-allowed');
                consoleLog.innerHTML = '';
                progressBar.style.width = '0%';

                const url = document.getElementById('urlInput').value;
                const style = document.querySelector('input[name="style"]:checked').value;

                // Connect WebSocket
                const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
                const wsUrl = `${protocol}//${window.location.host}/ws/process`;
                const ws = new WebSocket(wsUrl);

                ws.onopen = () => {
                    log('Connected to server...', 'success');
                    ws.send(JSON.stringify({ url, style }));
                };

                ws.onmessage = (event) => {
                    const data = JSON.parse(event.data);

                    if (data.type === 'log') {
                        log(data.message);
                    } else if (data.type === 'progress') {
                        progressBar.style.width = `${data.value}%`;
                    } else if (data.type === 'error') {
                        ws.close();
                        errorSection.classList.remove('hidden');
                        statusSection.classList.add('hidden');
                        document.getElementById('errorMessage').textContent = data.message;
                        document.getElementById('errorDetails').textContent = data.details || '';
                        submitBtn.disabled = false;
                        submitBtn.classList.remove('opacity-50', 'cursor-not-allowed');
                    } else if (data.type === 'done') {
                        ws.close();
                        // Update URL without reloading to show state
                        const newUrl = new URL(window.location);
                        newUrl.searchParams.set('id', data.videoId);
                        window.history.pushState({}, '', newUrl);
                        
                        loadResults(data.videoId);
                    }
                };

                ws.onclose = () => {
                    if (!errorSection.classList.contains('hidden') || !resultsSection.classList.contains('hidden')) return;
                };
                
                ws.onerror = (err) => {
                    log('WebSocket error occurred.', 'error');
                    console.error(err);
                };
            });

            async function loadResults(videoId) {
                statusSection.classList.add('hidden');
                inputSection.classList.remove('hidden'); // Keep input visible
                resultsSection.classList.remove('hidden');
                submitBtn.disabled = false;
                submitBtn.classList.remove('opacity-50', 'cursor-not-allowed');

                const grid = document.getElementById('clipsGrid');
                grid.innerHTML = '<div class="col-span-full text-center py-8"><div class="animate-spin text-4xl mb-2">⏳</div>Loading clips...</div>';

                try {
                    const res = await fetch(`/api/videos/${videoId}`);
                    if (!res.ok) throw new Error('Failed to fetch video info');
                    const data = await res.json();

                    grid.innerHTML = '';
                    
                    if (data.clips.length === 0) {
                        grid.innerHTML = '<div class="col-span-full text-center text-gray-500">No clips found. Check logs for errors.</div>';
                        return;
                    }

                    data.clips.forEach(clip => {
                        const card = document.createElement('div');
                        card.className = 'glass rounded-xl overflow-hidden hover:bg-gray-800 transition-colors group';
                        card.innerHTML = `
                            <div class="p-5">
                                <div class="flex items-start justify-between mb-2">
                                    <h4 class="font-bold text-lg leading-tight text-white group-hover:text-blue-400 transition-colors pr-4 break-words">${clip.name}</h4>
                                    <span class="text-xs font-mono bg-gray-700 text-gray-300 px-2 py-1 rounded uppercase">${clip.size}</span>
                                </div>
                                <div class="flex gap-3 mt-4">
                                    <a href="${clip.url}" download class="flex-1 bg-blue-600 hover:bg-blue-500 text-white text-center py-2 rounded-lg text-sm font-semibold transition-colors flex items-center justify-center gap-2">
                                        <span>⬇️ Download</span>
                                    </a>
                                    <button onclick="navigator.clipboard.writeText('${window.location.origin}${clip.url}')" class="px-3 bg-gray-700 hover:bg-gray-600 text-gray-300 rounded-lg transition-colors" title="Copy Link">
                                        🔗
                                    </button>
                                </div>
                            </div>
                        `;
                        grid.appendChild(card);
                    });
                    
                    // Scroll to results
                    resultsSection.scrollIntoView({ behavior: 'smooth' });

                } catch (e) {
                    grid.innerHTML = `<div class="col-span-full text-red-400">Error loading results: ${e.message}</div>`;
                }
            }

            // Check URL for ID on load
            window.addEventListener('DOMContentLoaded', () => {
                const urlParams = new URLSearchParams(window.location.search);
                const videoId = urlParams.get('id');
                if (videoId) {
                    loadResults(videoId);
                }
            });
        </script>
    </body>
    </html>
    """
    return HTMLResponse(html)

@app.get("/history", response_class=HTMLResponse)
async def history(request: Request):
    # Scan videos directory
    videos = []
    if VIDEOS_DIR.exists():
        # Sort by modified time, newest first
        paths = sorted(VIDEOS_DIR.iterdir(), key=os.path.getmtime, reverse=True)
        for p in paths:
            if p.is_dir():
                json_path = p / "highlights.json"
                title = p.name
                url = ""
                timestamp = datetime.fromtimestamp(p.stat().st_mtime).strftime("%Y-%m-%d %H:%M")
                
                if json_path.exists():
                    try:
                        data = json.loads(json_path.read_text(encoding="utf-8"))
                        # Use the first clip title as a proxy for the video title or fallback
                        highlights = data.get("highlights", [])
                        if highlights:
                            # Maybe aggregate titles or just show the first one
                            title = f"{len(highlights)} Clips Generated"
                        url = data.get("video_url", "")
                    except:
                        pass
                
                videos.append({
                    "id": p.name,
                    "title": title,
                    "url": url,
                    "date": timestamp
                })

    html = """
    <!DOCTYPE html>
    <html lang="en" class="dark">
    <head>
        <meta charset="UTF-8">
        <meta name="viewport" content="width=device-width, initial-scale=1.0">
        <title>History - YT Gemini Clipper</title>
        <script src="https://cdn.tailwindcss.com"></script>
        <script>
            tailwind.config = {
                darkMode: 'class',
                theme: {
                    extend: {
                        colors: {
                            gray: {
                                900: '#121212',
                                800: '#1e1e1e',
                                700: '#2d2d2d',
                            }
                        }
                    }
                }
            }
        </script>
        <style>
            .glass {
                background: rgba(30, 30, 30, 0.7);
                backdrop-filter: blur(10px);
                border: 1px solid rgba(255, 255, 255, 0.1);
            }
        </style>
    </head>
    <body class="bg-gray-900 text-gray-100 min-h-screen font-sans antialiased">

        <!-- Navbar -->
        <nav class="glass fixed top-0 w-full z-50 border-b border-gray-700">
            <div class="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8">
                <div class="flex items-center justify-between h-16">
                    <div class="flex items-center gap-3">
                        <div class="w-8 h-8 bg-gradient-to-br from-blue-500 to-purple-600 rounded-lg flex items-center justify-center text-white font-bold text-xl">✂️</div>
                        <a href="/" class="text-xl font-bold bg-clip-text text-transparent bg-gradient-to-r from-blue-400 to-purple-500 hover:opacity-80 transition-opacity">
                            YT Gemini Clipper
                        </a>
                    </div>
                    <div>
                        <a href="/" class="text-gray-300 hover:text-white transition-colors flex items-center gap-2 px-3 py-2 rounded-md hover:bg-gray-800">
                            <span>🏠</span>
                            <span class="hidden sm:inline">Home</span>
                        </a>
                    </div>
                </div>
            </div>
        </nav>

        <!-- Main Content -->
        <main class="max-w-5xl mx-auto px-4 pt-24 pb-12 space-y-8">
            <div class="flex items-center justify-between">
                <h1 class="text-3xl font-bold text-white">Processing History</h1>
            </div>

            <div class="grid gap-4">
    """
    
    if not videos:
        html += '<div class="text-center py-12 text-gray-500 text-lg">No history found.</div>'
    else:
        for v in videos:
            html += f"""
            <a href="/?id={v['id']}" class="glass p-6 rounded-xl hover:bg-gray-800 transition-all group block border-l-4 border-transparent hover:border-blue-500">
                <div class="flex items-start justify-between">
                    <div>
                        <h3 class="text-lg font-bold text-white group-hover:text-blue-400 transition-colors mb-1">{v['title']}</h3>
                        <div class="text-sm text-gray-400 font-mono mb-2">{v['id']}</div>
                        <div class="text-sm text-gray-500 truncate max-w-md">{v['url']}</div>
                    </div>
                    <div class="text-xs text-gray-500 font-mono bg-gray-800 px-2 py-1 rounded border border-gray-700">
                        {v['date']}
                    </div>
                </div>
            </a>
            """

    html += """
            </div>
        </main>
    </body>
    </html>
    """
    return HTMLResponse(html)

# ... rest of the file (websocket endpoint, api endpoints) ...

@app.websocket("/ws/process")
async def websocket_endpoint(websocket: WebSocket):
    await websocket.accept()
    try:
        data = await websocket.receive_json()
        url = data.get("url")
        style = data.get("style", "split")
        if not url:
            await websocket.send_json({"type": "error", "message": "No URL provided"})
            return
        
        await process_video_workflow(websocket, url, style)
    except WebSocketDisconnect:
        print("Client disconnected")
    except Exception as e:
        print(f"WebSocket error: {e}")
        # Try to send error if socket is still open
        try:
            await websocket.send_json({"type": "error", "message": str(e)})
        except:
            pass

@app.get("/api/videos/{video_id}")
async def get_video_info(video_id: str):
    workdir = VIDEOS_DIR / video_id
    if not workdir.exists():
        return {"error": "Video not found"}
    
    clips_dir = workdir / "clips"
    clips = []
    if clips_dir.exists():
        for f in sorted(clips_dir.glob("*.mp4")):
            size_mb = f.stat().st_size / (1024 * 1024)
            clips.append({
                "name": f.name,
                "url": f"/download/{video_id}/{f.name}",
                "size": f"{size_mb:.1f} MB"
            })
    
    return {"id": video_id, "clips": clips}

@app.get("/download/{video_id}/{filename}", response_class=FileResponse)
async def download_clip(video_id: str, filename: str):
    clip_path = VIDEOS_DIR / video_id / "clips" / filename
    if not clip_path.exists():
        return HTMLResponse("File not found", status_code=404)
    return FileResponse(clip_path, filename=filename, media_type="video/mp4")


if __name__ == "__main__":
    cli_main()
