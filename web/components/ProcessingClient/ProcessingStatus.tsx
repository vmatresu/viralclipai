/**
 * Processing Status Component
 *
 * Displays processing progress and logs.
 */

interface ProcessingStatusProps {
  progress: number;
  logs: string[];
}

export function ProcessingStatus({ progress, logs }: ProcessingStatusProps) {
  return (
    <section className="space-y-6">
      <div className="glass rounded-2xl p-6 border-l-4 border-blue-500">
        <h3 className="text-xl font-bold mb-4 flex items-center gap-2">
          <span className="animate-spin">⚙️</span> Processing Video...
        </h3>

        <div className="w-full bg-gray-700 rounded-full h-4 mb-6 overflow-hidden">
          <div
            className="bg-gradient-to-r from-blue-500 to-purple-500 h-4 rounded-full transition-all duration-500 ease-out"
            style={{ width: `${progress}%` }}
          />
        </div>

        <div className="bg-black/50 rounded-xl p-4 font-mono text-sm text-green-400 h-64 overflow-y-auto border border-gray-800 space-y-1">
          {logs.length === 0 ? (
            <div className="text-gray-500 italic">Waiting for task...</div>
          ) : (
            logs.map((l) => <div key={l}>{l}</div>)
          )}
        </div>
      </div>
    </section>
  );
}
