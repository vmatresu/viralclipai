"use client";

import { apiFetch } from "@/lib/apiClient";
import { useAuth } from "@/lib/auth";
import { FormEvent, useEffect, useState } from "react";

interface SettingsResponse {
  plan: string;
  max_clips_per_month: number;
  clips_used_this_month: number;
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
        const res = await apiFetch("/api/settings", { token });
        if (!cancelled) {
          setData(res as SettingsResponse);
          setAccessToken(res.settings?.tiktok_access_token ?? "");
          setAccountId(res.settings?.tiktok_account_id ?? "");
        }
      } catch (err: any) {
        if (!cancelled) setStatus(err.message || "Failed to load settings");
      } finally {
        if (!cancelled) setLoading(false);
      }
    }
    load();
    return () => {
      cancelled = true;
    };
  }, [getIdToken]);

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
    } catch (err: any) {
      setStatus(err.message || "Error saving settings.");
    } finally {
      setSaving(false);
    }
  }

  return (
    <div className="space-y-8">
      <section className="glass rounded-2xl p-6 space-y-4">
        <h2 className="text-xl font-semibold text-white">Plan &amp; Usage</h2>
        {loading && !data ? (
          <div className="text-sm text-gray-400">Loading plan information...</div>
        ) : data ? (
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
        ) : (
          <div className="text-sm text-gray-400">
            {status || "Unable to load plan information."}
          </div>
        )}
      </section>

      <section className="glass rounded-2xl p-6 space-y-4">
        <h2 className="text-xl font-semibold text-white">TikTok Integration</h2>
        <p className="text-sm text-gray-400">
          Connect your TikTok account by providing the access token and account ID from
          your TikTok developer application.
        </p>
        <form onSubmit={onSubmit} className="space-y-4">
          <div>
            <label className="block text-xs font-semibold text-gray-400 uppercase mb-1">
              TikTok Access Token
            </label>
            <input
              type="password"
              value={accessToken}
              onChange={(e) => setAccessToken(e.target.value)}
              className="w-full bg-gray-800 border border-gray-700 rounded-lg px-3 py-2 text-sm focus:border-blue-500 outline-none"
              placeholder="Paste your TikTok access token"
            />
          </div>
          <div>
            <label className="block text-xs font-semibold text-gray-400 uppercase mb-1">
              TikTok Account ID
            </label>
            <input
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
    </div>
  );
}
