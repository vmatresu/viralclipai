/**
 * Video Processing Form Component
 * 
 * Form for submitting video processing requests.
 */

import { FormEvent } from "react";

const STYLES = [
  { value: "split", label: "Split View", subtitle: "Top/Bottom" },
  { value: "left_focus", label: "Left Focus", subtitle: "Full Height" },
  { value: "right_focus", label: "Right Focus", subtitle: "Full Height" },
  { value: "all", label: "All Styles", subtitle: "Generate All" },
];

interface VideoFormProps {
  url: string;
  setUrl: (url: string) => void;
  style: string;
  setStyle: (style: string) => void;
  customPrompt: string;
  setCustomPrompt: (prompt: string) => void;
  onSubmit: (e: FormEvent) => void;
  submitting: boolean;
}

export function VideoForm({
  url,
  setUrl,
  style,
  setStyle,
  customPrompt,
  setCustomPrompt,
  onSubmit,
  submitting,
}: VideoFormProps) {
  return (
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
  );
}

