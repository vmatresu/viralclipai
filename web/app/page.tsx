import { ProcessingClient } from "@/components/ProcessingClient";
import { Suspense } from "react";

export default function HomePage() {
  return (
    <div className="space-y-12">
      <section className="space-y-4">
        <h1 className="text-3xl md:text-4xl font-extrabold text-white">
          Turn long-form videos into viral clips in minutes.
        </h1>
        <p className="text-gray-300 max-w-2xl">
          Viral Clip AI analyzes your YouTube commentary videos, finds the most engaging
          moments, and generates social-ready clips optimized for TikTok, Shorts, and
          Reels.
        </p>
        <div className="flex flex-wrap gap-3">
          <a
            href="#app"
            className="px-5 py-3 rounded-xl bg-blue-600 hover:bg-blue-500 text-white font-semibold text-sm md:text-base transition-colors"
          >
            Try it now
          </a>
          <a
            href="/pricing"
            className="px-5 py-3 rounded-xl bg-gray-800 hover:bg-gray-700 text-gray-100 font-semibold text-sm md:text-base border border-gray-700 transition-colors"
          >
            View pricing
          </a>
        </div>
      </section>

      <section className="grid md:grid-cols-3 gap-6">
        <div className="glass rounded-2xl p-5 space-y-2">
          <h3 className="font-semibold text-white">AI highlight detection</h3>
          <p className="text-sm text-gray-300">
            Powered by Gemini to find high-retention segments and automatically propose
            clip boundaries.
          </p>
        </div>
        <div className="glass rounded-2xl p-5 space-y-2">
          <h3 className="font-semibold text-white">Vertical-ready formats</h3>
          <p className="text-sm text-gray-300">
            Split view, left/right focus, or all styles—designed for TikTok, Shorts, and
            Reels.
          </p>
        </div>
        <div className="glass rounded-2xl p-5 space-y-2">
          <h3 className="font-semibold text-white">Per-user history & limits</h3>
          <p className="text-sm text-gray-300">
            Firebase Auth, Firestore, and S3-backed storage so every creator has their
            own secure workspace.
          </p>
        </div>
      </section>

      <section id="app">
        <Suspense fallback={<div className="text-gray-400">Loading...</div>}>
          <ProcessingClient />
        </Suspense>
      </section>
    </div>
  );
}
