"use client";

import { apiFetch } from "@/lib/apiClient";
import { useAuth } from "@/lib/auth";
import { useEffect, useState } from "react";

interface UserVideo {
  id?: string;
  video_id?: string;
  video_title?: string;
  video_url?: string;
  created_at?: string;
  custom_prompt?: string;
}

export default function HistoryPage() {
  const { getIdToken } = useAuth();
  const [videos, setVideos] = useState<UserVideo[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    async function load() {
      try {
        const token = await getIdToken();
        if (!token) {
          setError("Please sign in to view your history.");
          setLoading(false);
          return;
        }
        const data = await apiFetch("/api/user/videos", { token });
        if (!cancelled) {
          setVideos(data.videos ?? []);
        }
      } catch (err: any) {
        if (!cancelled) {
          setError(err.message || "Failed to load history");
        }
      } finally {
        if (!cancelled) setLoading(false);
      }
    }
    load();
    return () => {
      cancelled = true;
    };
  }, [getIdToken]);

  if (loading) {
    return (
      <div className="text-center py-12 text-gray-500 text-lg">
        Loading your processing history...
      </div>
    );
  }

  if (error) {
    return <div className="text-center py-12 text-red-400 text-lg">{error}</div>;
  }

  if (videos.length === 0) {
    return (
      <div className="text-center py-12 text-gray-500 text-lg">No history found.</div>
    );
  }

  return (
    <div className="grid gap-4">
      {videos.map((v) => {
        const id = v.video_id || v.id || "";
        return (
          <a
            key={id}
            href={`/?id=${encodeURIComponent(id)}`}
            className="glass p-6 rounded-xl hover:bg-gray-800 transition-all group block border-l-4 border-transparent hover:border-blue-500"
          >
            <div className="flex items-start justify-between">
              <div>
                <h3 className="text-lg font-bold text-white group-hover:text-blue-400 transition-colors mb-1">
                  {v.video_title || "Generated Clips"}
                </h3>
                <div className="text-sm text-gray-400 font-mono mb-2">{id}</div>
                <div className="text-sm text-gray-500 truncate max-w-md">
                  {v.video_url || ""}
                </div>
                {v.custom_prompt && (
                  <div className="mt-1 text-xs text-gray-500 line-clamp-2">
                    <span className="font-semibold text-gray-400 mr-1">Prompt:</span>
                    {v.custom_prompt}
                  </div>
                )}
              </div>
              <div className="text-xs text-gray-500 font-mono bg-gray-800 px-2 py-1 rounded border border-gray-700">
                {v.created_at || ""}
              </div>
            </div>
          </a>
        );
      })}
    </div>
  );
}
