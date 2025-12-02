/**
 * Error Display Component
 * 
 * Displays error messages and details.
 */

interface ErrorDisplayProps {
  error: string;
  errorDetails: string | null;
}

export function ErrorDisplay({ error, errorDetails }: ErrorDisplayProps) {
  return (
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
  );
}

