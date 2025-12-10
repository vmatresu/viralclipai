"use client";

import { type FormEvent, useEffect, useState } from "react";

import { UserManagement } from "@/components/admin/UserManagement";
import { apiFetch } from "@/lib/apiClient";
import { useAuth } from "@/lib/auth";
import { type StorageInfo } from "@/types/storage";

interface SettingsResponse {
  plan: string;
  max_clips_per_month: number;
  clips_used_this_month: number;
  role?: string;
  storage: StorageInfo;
  settings: {
    tiktok_access_token?: string;
    tiktok_account_id?: string;
    [key: string]: unknown;
  };
}

export default function SettingsPage() {
  const { getIdToken, user, loading: authLoading } = useAuth();
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

    if (!authLoading) {
      async function load() {
        try {
          if (!user) {
            setStatus("Please sign in to view your settings.");
            setLoading(false);
            return;
          }

          const token = await getIdToken();
          if (!token) {
            setStatus("Unable to retrieve authentication token.");
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
    }

    return () => {
      cancelled = true;
    };
  }, [getIdToken, user, authLoading]);

  // Load admin prompt if user is superadmin
  useEffect(() => {
    let cancelled = false;

    if (data?.role === "superadmin") {
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
    }

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
        <h2 className="text-xl font-semibold text-foreground">Plan &amp; Usage</h2>
        {(() => {
          if (loading && !data) {
            return (
              <div className="text-sm text-muted-foreground">
                Loading plan information...
              </div>
            );
          }
          if (data) {
            const storagePercentage = data.storage?.percentage ?? 0;
            const isHighStorage = storagePercentage >= 80;
            const isNearLimit = storagePercentage >= 90;

            return (
              <div className="space-y-4">
                <div className="space-y-1 text-sm text-muted-foreground">
                  <div>
                    <span className="font-semibold text-foreground">Plan:</span>{" "}
                    <span className="uppercase text-brand-600 text-xs">
                      {data.plan}
                    </span>
                  </div>
                  <div>
                    <span className="font-semibold text-foreground">
                      Monthly Clips:
                    </span>{" "}
                    {data.clips_used_this_month} / {data.max_clips_per_month}
                  </div>
                </div>

                {/* Storage Usage Section */}
                {data.storage && (
                  <div className="space-y-2 pt-2 border-t border-brand-100 dark:border-white/10">
                    <div className="flex justify-between text-sm">
                      <span className="font-semibold text-foreground">Storage</span>
                      <span
                        className={
                          isHighStorage
                            ? "text-red-500 font-semibold"
                            : "text-muted-foreground"
                        }
                      >
                        {data.storage.used_formatted} / {data.storage.limit_formatted}
                      </span>
                    </div>
                    <div className="relative h-3 w-full overflow-hidden rounded-full bg-muted">
                      <div
                        className={`h-full transition-all duration-500 ${
                          isNearLimit
                            ? "bg-red-500"
                            : isHighStorage
                              ? "bg-orange-500"
                              : "bg-brand-500"
                        }`}
                        style={{ width: `${Math.min(storagePercentage, 100)}%` }}
                      />
                    </div>
                    <div className="flex justify-between text-xs text-muted-foreground">
                      <span>{data.storage.total_clips} clips</span>
                      <span>{data.storage.remaining_formatted} remaining</span>
                    </div>
                    {isHighStorage && (
                      <div
                        className={`text-xs ${isNearLimit ? "text-red-500" : "text-orange-500"}`}
                      >
                        {isNearLimit
                          ? "⚠️ Storage almost full! Consider upgrading your plan or deleting old clips."
                          : "⚠️ Storage usage is high. Consider upgrading your plan."}
                      </div>
                    )}
                  </div>
                )}
              </div>
            );
          }
          return (
            <div className="text-sm text-muted-foreground">
              {status ?? "Unable to load plan information."}
            </div>
          );
        })()}
      </section>

      <section className="glass rounded-2xl p-6 space-y-4">
        <h2 className="text-xl font-semibold text-foreground">TikTok Integration</h2>
        <p className="text-sm text-muted-foreground">
          Connect your TikTok account by providing the access token and account ID from
          your TikTok developer application.
        </p>
        <form onSubmit={onSubmit} className="space-y-4">
          <div>
            <label
              htmlFor="tiktok-access-token"
              className="block text-xs font-semibold text-muted-foreground uppercase mb-1"
            >
              TikTok Access Token
            </label>
            <input
              id="tiktok-access-token"
              type="password"
              value={accessToken}
              onChange={(e) => setAccessToken(e.target.value)}
              className="w-full h-11 rounded-lg border border-brand-100 bg-white px-3 text-sm text-foreground shadow-sm placeholder:text-muted-foreground/70 focus:border-brand-300 focus:ring-2 focus:ring-brand-500/20 outline-none transition-colors dark:border-white/10 dark:bg-slate-900 dark:text-white dark:placeholder:text-white/60"
              placeholder="Paste your TikTok access token"
            />
          </div>
          <div>
            <label
              htmlFor="tiktok-account-id"
              className="block text-xs font-semibold text-muted-foreground uppercase mb-1"
            >
              TikTok Account ID
            </label>
            <input
              id="tiktok-account-id"
              type="text"
              value={accountId}
              onChange={(e) => setAccountId(e.target.value)}
              className="w-full h-11 rounded-lg border border-brand-100 bg-white px-3 text-sm text-foreground shadow-sm placeholder:text-muted-foreground/70 focus:border-brand-300 focus:ring-2 focus:ring-brand-500/20 outline-none transition-colors dark:border-white/10 dark:bg-slate-900 dark:text-white dark:placeholder:text-white/60"
              placeholder="Your TikTok account ID or advertiser ID"
            />
          </div>
          <div className="flex items-center justify-between mt-4 text-sm">
            <div className="text-muted-foreground">{status}</div>
            <button
              type="submit"
              disabled={saving}
              className="px-4 h-11 bg-brand-500 hover:bg-brand-600 disabled:opacity-50 disabled:cursor-not-allowed text-white rounded-lg text-sm font-semibold transition-colors shadow-sm shadow-brand-500/20"
            >
              {saving ? "Saving..." : "Save Settings"}
            </button>
          </div>
        </form>
      </section>

      {data?.role === "superadmin" && (
        <section className="glass rounded-2xl p-6 space-y-4">
          <h2 className="text-xl font-semibold text-foreground">
            Admin: User Management
          </h2>
          <p className="text-sm text-muted-foreground">
            View and manage user accounts and their subscription plans.
          </p>
          <UserManagement getIdToken={getIdToken} />
        </section>
      )}

      {data?.role === "superadmin" && (
        <section className="glass rounded-2xl p-6 space-y-4">
          <h2 className="text-xl font-semibold text-foreground">
            Admin: Global Prompt
          </h2>
          <p className="text-sm text-muted-foreground">
            Manage the global base prompt used for video analysis. This prompt is used
            when users do not provide a custom prompt. The prompt is stored in Firestore
            and is the source of truth for the system.
          </p>
          {promptLoading ? (
            <div className="text-sm text-muted-foreground">Loading prompt...</div>
          ) : (
            <form onSubmit={onPromptSubmit} className="space-y-4">
              <div>
                <label
                  htmlFor="admin-prompt"
                  className="block text-xs font-semibold text-muted-foreground uppercase mb-1"
                >
                  Global Base Prompt
                </label>
                <textarea
                  id="admin-prompt"
                  value={prompt}
                  onChange={(e) => setPrompt(e.target.value)}
                  rows={15}
                  className="w-full rounded-lg border border-brand-100 bg-white px-3 py-2 text-sm text-foreground shadow-sm focus:border-brand-300 focus:ring-2 focus:ring-brand-500/20 outline-none transition-colors font-mono dark:border-white/10 dark:bg-slate-900 dark:text-white"
                  placeholder="Enter the global prompt for video analysis..."
                />
                <div className="text-xs text-muted-foreground mt-1">
                  {prompt.length} characters
                </div>
              </div>
              <div className="flex items-center justify-between mt-4 text-sm">
                <div className="text-muted-foreground">{promptStatus}</div>
                <button
                  type="submit"
                  disabled={promptSaving}
                  className="px-4 h-11 bg-brand-500 hover:bg-brand-600 disabled:opacity-50 disabled:cursor-not-allowed text-white rounded-lg text-sm font-semibold transition-colors shadow-sm shadow-brand-500/20"
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
