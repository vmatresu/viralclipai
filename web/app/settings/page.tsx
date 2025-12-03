"use client";

import { type FormEvent, useEffect, useState } from "react";

import { apiFetch } from "@/lib/apiClient";
import { useAuth } from "@/lib/auth";

interface SettingsResponse {
  plan: string;
  max_clips_per_month: number;
  clips_used_this_month: number;
  role?: string;
  settings: {
    tiktok_access_token?: string;
    tiktok_account_id?: string;
    [key: string]: unknown;
  };
}

export default function SettingsPage() {
  const { getIdToken } = useAuth();
  const [data, setData] = useState<SettingsResponse | null>(null);
  const [accessToken, setAccessToken] = useState("");
  const [accountId, setAccountId] = useState("");
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [status, setStatus] = useState<string | null>(null);

  // Admin prompt state
  const [prompt, setPrompt] = useState("");
  const [promptLoading, setPromptLoading] = useState(false);
  const [promptSaving, setPromptSaving] = useState(false);
  const [promptStatus, setPromptStatus] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    async function load() {
      try {
        const token = await getIdToken();
        if (!token) {
          setStatus("Please sign in to view your settings.");
          setLoading(false);
          return;
        }
        const res = await apiFetch<SettingsResponse>("/api/settings", {
          token,
        });
        if (!cancelled) {
          setData(res);
          setAccessToken(res.settings?.tiktok_access_token ?? "");
          setAccountId(res.settings?.tiktok_account_id ?? "");
        }
      } catch (err: unknown) {
        if (!cancelled) {
          const errorMessage =
            err instanceof Error ? err.message : "Failed to load settings";
          setStatus(errorMessage);
        }
      } finally {
        if (!cancelled) setLoading(false);
      }
    }
    void load();
    return () => {
      cancelled = true;
    };
  }, [getIdToken]);

  // Load admin prompt if user is superadmin
  useEffect(() => {
    if (!data?.role || data.role !== "superadmin") {
      return;
    }

    let cancelled = false;
    async function loadPrompt() {
      try {
        setPromptLoading(true);
        const token = await getIdToken();
        if (!token) return;

        const res = await apiFetch<{ prompt: string }>("/api/admin/prompt", {
          token,
        });
        if (!cancelled) {
          setPrompt(res.prompt);
        }
      } catch (err: unknown) {
        if (!cancelled) {
          const errorMessage =
            err instanceof Error ? err.message : "Failed to load prompt";
          setPromptStatus(errorMessage);
        }
      } finally {
        if (!cancelled) setPromptLoading(false);
      }
    }
    void loadPrompt();
    return () => {
      cancelled = true;
    };
  }, [data?.role, getIdToken]);

  async function onSubmit(e: FormEvent) {
    e.preventDefault();
    setSaving(true);
    setStatus(null);
    try {
      const token = await getIdToken();
      if (!token) {
        setStatus("Please sign in to save settings.");
        return;
      }
      const payload = {
        settings: {
          tiktok_access_token: accessToken,
          tiktok_account_id: accountId,
        },
      };
      await apiFetch("/api/settings", {
        method: "POST",
        token,
        body: payload,
      });
      setStatus("Settings saved.");
    } catch (err: unknown) {
      const errorMessage =
        err instanceof Error ? err.message : "Error saving settings.";
      setStatus(errorMessage);
    } finally {
      setSaving(false);
    }
  }

  async function onPromptSubmit(e: FormEvent) {
    e.preventDefault();
    setPromptSaving(true);
    setPromptStatus(null);
    try {
      const token = await getIdToken();
      if (!token) {
        setPromptStatus("Please sign in to save prompt.");
        return;
      }
      await apiFetch("/api/admin/prompt", {
        method: "POST",
        token,
        body: { prompt },
      });
      setPromptStatus("Prompt saved successfully.");
    } catch (err: unknown) {
      const errorMessage = err instanceof Error ? err.message : "Error saving prompt.";
      setPromptStatus(errorMessage);
    } finally {
      setPromptSaving(false);
    }
  }

  return (
    <div className="space-y-8">
      <section className="glass rounded-2xl p-6 space-y-4">
        <h2 className="text-xl font-semibold text-white">Plan &amp; Usage</h2>
        {(() => {
          if (loading && !data) {
            return (
              <div className="text-sm text-gray-400">Loading plan information...</div>
            );
          }
          if (data) {
            return (
              <div className="space-y-1 text-sm text-gray-300">
                <div>
                  <span className="font-semibold">Plan:</span>{" "}
                  <span className="uppercase text-blue-400 text-xs">{data.plan}</span>
                </div>
                <div>
                  <span className="font-semibold">Monthly Clips:</span>{" "}
                  {data.clips_used_this_month} / {data.max_clips_per_month}
                </div>
              </div>
            );
          }
          return (
            <div className="text-sm text-gray-400">
              {status ?? "Unable to load plan information."}
            </div>
          );
        })()}
      </section>

      <section className="glass rounded-2xl p-6 space-y-4">
        <h2 className="text-xl font-semibold text-white">TikTok Integration</h2>
        <p className="text-sm text-gray-400">
          Connect your TikTok account by providing the access token and account ID from
          your TikTok developer application.
        </p>
        <form onSubmit={onSubmit} className="space-y-4">
          <div>
            <label
              htmlFor="tiktok-access-token"
              className="block text-xs font-semibold text-gray-400 uppercase mb-1"
            >
              TikTok Access Token
            </label>
            <input
              id="tiktok-access-token"
              type="password"
              value={accessToken}
              onChange={(e) => setAccessToken(e.target.value)}
              className="w-full bg-gray-800 border border-gray-700 rounded-lg px-3 py-2 text-sm focus:border-blue-500 outline-none"
              placeholder="Paste your TikTok access token"
            />
          </div>
          <div>
            <label
              htmlFor="tiktok-account-id"
              className="block text-xs font-semibold text-gray-400 uppercase mb-1"
            >
              TikTok Account ID
            </label>
            <input
              id="tiktok-account-id"
              type="text"
              value={accountId}
              onChange={(e) => setAccountId(e.target.value)}
              className="w-full bg-gray-800 border border-gray-700 rounded-lg px-3 py-2 text-sm focus:border-blue-500 outline-none"
              placeholder="Your TikTok account ID or advertiser ID"
            />
          </div>
          <div className="flex items-center justify-between mt-4 text-sm">
            <div className="text-gray-400">{status}</div>
            <button
              type="submit"
              disabled={saving}
              className="px-4 py-2 bg-blue-600 hover:bg-blue-500 disabled:opacity-50 disabled:cursor-not-allowed text-white rounded-lg text-sm font-semibold transition-colors"
            >
              {saving ? "Saving..." : "Save Settings"}
            </button>
          </div>
        </form>
      </section>

      {data?.role === "superadmin" && (
        <section className="glass rounded-2xl p-6 space-y-4">
          <h2 className="text-xl font-semibold text-white">Admin: Global Prompt</h2>
          <p className="text-sm text-gray-400">
            Manage the global base prompt used for video analysis. This prompt is used
            when users don't provide a custom prompt. The prompt is stored in Firestore
            and is the source of truth for the system.
          </p>
          {promptLoading ? (
            <div className="text-sm text-gray-400">Loading prompt...</div>
          ) : (
            <form onSubmit={onPromptSubmit} className="space-y-4">
              <div>
                <label
                  htmlFor="admin-prompt"
                  className="block text-xs font-semibold text-gray-400 uppercase mb-1"
                >
                  Global Base Prompt
                </label>
                <textarea
                  id="admin-prompt"
                  value={prompt}
                  onChange={(e) => setPrompt(e.target.value)}
                  rows={15}
                  className="w-full bg-gray-800 border border-gray-700 rounded-lg px-3 py-2 text-sm focus:border-blue-500 outline-none font-mono"
                  placeholder="Enter the global prompt for video analysis..."
                />
                <div className="text-xs text-gray-500 mt-1">
                  {prompt.length} characters
                </div>
              </div>
              <div className="flex items-center justify-between mt-4 text-sm">
                <div className="text-gray-400">{promptStatus}</div>
                <button
                  type="submit"
                  disabled={promptSaving}
                  className="px-4 py-2 bg-blue-600 hover:bg-blue-500 disabled:opacity-50 disabled:cursor-not-allowed text-white rounded-lg text-sm font-semibold transition-colors"
                >
                  {promptSaving ? "Saving..." : "Save Prompt"}
                </button>
              </div>
            </form>
          )}
        </section>
      )}
    </div>
  );
}
