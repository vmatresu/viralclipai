/**
 * Results Display Component
 *
 * Displays processing results and clips.
 */

import { ClipGrid, type Clip } from "../ClipGrid";

interface ResultsProps {
  videoId: string;
  clips: Clip[];
  customPromptUsed: string | null;
  log: (msg: string, type?: "info" | "error" | "success") => void;
  onReset: () => void;
}

export function Results({
  videoId,
  clips,
  customPromptUsed,
  log,
  onReset,
}: ResultsProps) {
  return (
    <section className="space-y-6">
      <div className="flex items-center justify-between">
        <h2 className="text-2xl font-bold text-white">🎉 Results</h2>
        <button
          onClick={onReset}
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
  );
}
