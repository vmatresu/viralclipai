"use client";

import { apiFetch } from "@/lib/apiClient";
import { useAuth } from "@/lib/auth";
import { frontendLogger } from "@/lib/logger";
import { useSearchParams } from "next/navigation";
import { FormEvent, useCallback, useEffect, useMemo, useState } from "react";
import { Clip, ClipGrid } from "./ClipGrid";

const STYLES = [
  { value: "split", label: "Split View", subtitle: "Top/Bottom" },
  { value: "left_focus", label: "Left Focus", subtitle: "Full Height" },
  { value: "right_focus", label: "Right Focus", subtitle: "Full Height" },
  { value: "all", label: "All Styles", subtitle: "Generate All" },
];

export function ProcessingClient() {
  const searchParams = useSearchParams();
  const { getIdToken } = useAuth();

  const [url, setUrl] = useState("");
  const [style, setStyle] = useState("split");
  const [logs, setLogs] = useState<string[]>([]);
  const [progress, setProgress] = useState(0);
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [errorDetails, setErrorDetails] = useState<string | null>(null);
  const [videoId, setVideoId] = useState<string | null>(null);
  const [clips, setClips] = useState<Clip[]>([]);
  const [customPrompt, setCustomPrompt] = useState("");
  const [customPromptUsed, setCustomPromptUsed] = useState<string | null>(null);

  const hasResults = useMemo(() => clips.length > 0, [clips]);

  const log = useCallback(
    (msg: string, type: "info" | "error" | "success" = "info") => {
      setLogs((prev) => [
        ...prev,
        `${type === "error" ? "[ERROR]" : type === "success" ? "[OK]" : ">"} ${msg}`,
      ]);
    },
    []
  );

  const loadResults = useCallback(
    async (id: string) => {
      try {
        setSubmitting(false);
        const token = await getIdToken();
        if (!token) {
          throw new Error("You must be signed in to view your clips.");
        }
        const data = await apiFetch<{ clips: Clip[]; custom_prompt?: string }>(
          `/api/videos/${id}`,
          {
            token,
          }
        );
        setClips(data.clips || []);
        setCustomPromptUsed(data.custom_prompt ?? null);
      } catch (err: any) {
        setError(err.message || "Error loading results");
      }
    },
    [getIdToken]
  );

  useEffect(() => {
    const existingId = searchParams.get("id");
    if (existingId) {
      setVideoId(existingId);
      loadResults(existingId);
    }
  }, [searchParams, loadResults]);

  async function onSubmit(e: FormEvent) {
    e.preventDefault();
    setSubmitting(true);
    setError(null);
    setErrorDetails(null);
    setLogs([]);
    setProgress(0);
    setClips([]);
    setVideoId(null);
    setCustomPromptUsed(null);

    try {
      const token = await getIdToken();
      if (!token) {
        log("You must be signed in to process videos.", "error");
        alert("Please sign in with your Google account to use this app.");
        setSubmitting(false);
        return;
      }

      const apiBase = process.env.NEXT_PUBLIC_API_BASE_URL || window.location.origin;
      const baseUrl = new URL(apiBase);
      const wsProtocol = baseUrl.protocol === "https:" ? "wss:" : "ws:";
      const wsUrl = `${wsProtocol}//${baseUrl.host}/ws/process`;
      const ws = new WebSocket(wsUrl);

      ws.onopen = () => {
        log("Connected to server...", "success");
        ws.send(
          JSON.stringify({
            url,
            style,
            token,
            prompt: customPrompt.trim() || undefined,
          })
        );
      };

      ws.onmessage = (event) => {
        const data = JSON.parse(event.data);
        if (data.type === "log") {
          log(data.message);
        } else if (data.type === "progress") {
          setProgress(data.value ?? 0);
        } else if (data.type === "error") {
          ws.close();
          setError(data.message || "An unexpected error occurred.");
          setErrorDetails(data.details || null);
          setSubmitting(false);
        } else if (data.type === "done") {
          ws.close();
          const id = data.videoId as string;
          setVideoId(id);
          const newUrl = new URL(window.location.href);
          newUrl.searchParams.set("id", id);
          window.history.pushState({}, "", newUrl.toString());
          loadResults(id);
        }
      };

      ws.onerror = (ev) => {
        frontendLogger.error("WebSocket error occurred", ev);
        log("WebSocket error occurred.", "error");
      };

      ws.onclose = () => {
        if (!hasResults && !error) {
          setSubmitting(false);
        }
      };
    } catch (err: any) {
      frontendLogger.error("Failed to start processing", err);
      setError(err.message || "Failed to start processing");
      setSubmitting(false);
    }
  }

  return (
    <div className="space-y-8">
      {/* Input Section */}
      {!hasResults && (
        <section className="glass rounded-2xl p-8 shadow-2xl">
          <form onSubmit={onSubmit} className="space-y-6">
            <div className="space-y-2">
              <label className="text-sm font-medium text-gray-400 uppercase tracking-wider">
                YouTube Source URL
              </label>
              <div className="relative">
                <input
                  type="text"
                  value={url}
                  onChange={(e) => setUrl(e.target.value)}
                  placeholder="https://www.youtube.com/watch?v=..."
                  className="w-full bg-gray-800 border border-gray-700 rounded-xl px-5 py-4 text-lg focus:ring-2 focus:ring-blue-500 focus:border-transparent outline-none transition-all placeholder-gray-600"
                  required
                />
              </div>
            </div>

            <div className="space-y-2">
              <label className="text-sm font-medium text-gray-400 uppercase tracking-wider">
                Custom prompt (optional)
              </label>
              <textarea
                value={customPrompt}
                onChange={(e) => setCustomPrompt(e.target.value)}
                rows={3}
                className="w-full bg-gray-800 border border-gray-700 rounded-xl px-4 py-3 text-sm focus:ring-2 focus:ring-blue-500 focus:border-transparent outline-none placeholder-gray-500"
                placeholder="e.g. Find the most emotional moments where the speaker shares a personal story."
              />
              <div className="flex flex-wrap gap-2 text-xs">
                <button
                  type="button"
                  onClick={() =>
                    setCustomPrompt(
                      "Find the most emotional and vulnerable moments in this video that would resonate strongly on TikTok."
                    )
                  }
                  className="px-3 py-1 rounded-full bg-gray-800 hover:bg-gray-700 text-gray-200 border border-gray-700"
                >
                  Emotional moments
                </button>
                <button
                  type="button"
                  onClick={() =>
                    setCustomPrompt(
                      "Find the best high-retention viral clip candidates for short-form social media (TikTok, Shorts, Reels)."
                    )
                  }
                  className="px-3 py-1 rounded-full bg-gray-800 hover:bg-gray-700 text-gray-200 border border-gray-700"
                >
                  Best viral clips
                </button>
                <button
                  type="button"
                  onClick={() =>
                    setCustomPrompt(
                      "Find segments with intense discussion about the main subject, where there is strong opinion or debate."
                    )
                  }
                  className="px-3 py-1 rounded-full bg-gray-800 hover:bg-gray-700 text-gray-200 border border-gray-700"
                >
                  Subject discussion
                </button>
                <button
                  type="button"
                  onClick={() =>
                    setCustomPrompt(
                      "Find moments with interesting sounds or reactions that would work well in sound-on social media clips."
                    )
                  }
                  className="px-3 py-1 rounded-full bg-gray-800 hover:bg-gray-700 text-gray-200 border border-gray-700"
                >
                  Sound-focused clips
                </button>
              </div>
            </div>

            <div className="space-y-2">
              <label className="text-sm font-medium text-gray-400 uppercase tracking-wider">
                Output Style
              </label>
              <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
                {STYLES.map((s) => (
                  <label key={s.value} className="cursor-pointer">
                    <input
                      type="radio"
                      name="style"
                      value={s.value}
                      checked={style === s.value}
                      onChange={() => setStyle(s.value)}
                      className="peer sr-only"
                    />
                    <div className="p-4 rounded-xl bg-gray-800 border border-gray-700 peer-checked:border-blue-500 peer-checked:bg-blue-900/20 transition-all text-center hover:bg-gray-750">
                      <span className="font-medium">{s.label}</span>
                      <span className="block text-xs text-gray-500 mt-1">
                        {s.subtitle}
                      </span>
                    </div>
                  </label>
                ))}
              </div>
            </div>

            <button
              type="submit"
              disabled={submitting}
              className="w-full py-4 bg-gradient-to-r from-blue-600 to-purple-600 hover:from-blue-500 hover:to-purple-500 rounded-xl font-bold text-lg shadow-lg transform transition-all active:scale-[0.98] flex justify-center items-center gap-2 disabled:opacity-50 disabled:cursor-not-allowed"
            >
              <span>🚀 Launch Processor</span>
            </button>
          </form>
        </section>
      )}

      {/* Status Section */}
      {submitting && (
        <section className="space-y-6">
          <div className="glass rounded-2xl p-6 border-l-4 border-blue-500">
            <h3 className="text-xl font-bold mb-4 flex items-center gap-2">
              <span className="animate-spin">⚙️</span> Processing Video...
            </h3>

            <div className="w-full bg-gray-700 rounded-full h-4 mb-6 overflow-hidden">
              <div
                className="bg-gradient-to-r from-blue-500 to-purple-500 h-4 rounded-full transition-all duration-500 ease-out"
                style={{ width: `${progress}%` }}
              ></div>
            </div>

            <div className="bg-black/50 rounded-xl p-4 font-mono text-sm text-green-400 h-64 overflow-y-auto border border-gray-800 space-y-1">
              {logs.length === 0 ? (
                <div className="text-gray-500 italic">Waiting for task...</div>
              ) : (
                logs.map((l, idx) => <div key={idx}>{l}</div>)
              )}
            </div>
          </div>
        </section>
      )}

      {/* Error Section */}
      {error && (
        <section>
          <div className="glass rounded-2xl p-6 border-l-4 border-red-500 bg-red-900/10">
            <h3 className="text-xl font-bold text-red-400 mb-2">
              ❌ Processing Failed
            </h3>
            <p className="text-gray-300 mb-4">{error}</p>
            {errorDetails && (
              <pre className="bg-black/50 p-4 rounded-lg text-xs text-red-300 overflow-x-auto whitespace-pre-wrap">
                {errorDetails}
              </pre>
            )}
          </div>
        </section>
      )}

      {/* Results Section */}
      {videoId && !error && (
        <section className="space-y-6">
          <div className="flex items-center justify-between">
            <h2 className="text-2xl font-bold text-white">🎉 Results</h2>
            <button
              onClick={() => {
                setVideoId(null);
                setClips([]);
                const newUrl = new URL(window.location.href);
                newUrl.searchParams.delete("id");
                window.history.pushState({}, "", newUrl.toString());
              }}
              className="text-sm text-blue-400 hover:text-blue-300 hover:underline"
            >
              Process Another Video
            </button>
          </div>
          {customPromptUsed && (
            <div className="glass rounded-xl p-4 border border-gray-700">
              <h3 className="text-sm font-semibold text-gray-200 mb-1">
                Custom prompt used
              </h3>
              <p className="text-xs text-gray-300 whitespace-pre-wrap">
                {customPromptUsed}
              </p>
            </div>
          )}
          <ClipGrid videoId={videoId} clips={clips} log={log} />
        </section>
      )}
    </div>
  );
}
