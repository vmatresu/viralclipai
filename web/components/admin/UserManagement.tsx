"use client";

import { useCallback, useEffect, useState } from "react";
import { toast } from "sonner";

import { Button } from "@/components/ui/button";
import {
    Select,
    SelectContent,
    SelectItem,
    SelectTrigger,
    SelectValue,
} from "@/components/ui/select";
import { apiFetch } from "@/lib/apiClient";

interface AdminUser {
  uid: string;
  email: string | null;
  plan: string;
  role: string | null;
  credits_used_this_month: number;
  monthly_credits_limit: number;
  created_at: string;
  updated_at: string;
}

interface ListUsersResponse {
  users: AdminUser[];
  next_page_token: string | null;
}

interface UserManagementProps {
  getIdToken: () => Promise<string | null>;
}

const VALID_PLANS = ["free", "pro", "studio"] as const;

export function UserManagement({ getIdToken }: UserManagementProps) {
  const [users, setUsers] = useState<AdminUser[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [updating, setUpdating] = useState<string | null>(null);
  const [searchUid, setSearchUid] = useState("");
  const [searchResult, setSearchResult] = useState<AdminUser | null>(null);
  const [searching, setSearching] = useState(false);
  const [editingUsage, setEditingUsage] = useState<string | null>(null);
  const [usageValue, setUsageValue] = useState("");

  const loadUsers = useCallback(async () => {
    try {
      setLoading(true);
      const token = await getIdToken();
      if (!token) return;

      const res = await apiFetch<ListUsersResponse>("/api/admin/users?limit=50", {
        token,
      });
      setUsers(res.users);
      setError(null);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to load users");
    } finally {
      setLoading(false);
    }
  }, [getIdToken]);

  useEffect(() => {
    void loadUsers();
  }, [loadUsers]);

  const handlePlanChange = async (uid: string, newPlan: string) => {
    try {
      setUpdating(uid);
      const token = await getIdToken();
      if (!token) return;

      await apiFetch(`/api/admin/users/${encodeURIComponent(uid)}/plan`, {
        method: "PATCH",
        token,
        body: { plan: newPlan },
      });

      // Update local state
      setUsers((prev) =>
        prev.map((u) => (u.uid === uid ? { ...u, plan: newPlan } : u))
      );
      if (searchResult?.uid === uid) {
        setSearchResult({ ...searchResult, plan: newPlan });
      }

      toast.success(`Plan updated to ${newPlan}`);
    } catch (err) {
      toast.error(err instanceof Error ? err.message : "Failed to update plan");
    } finally {
      setUpdating(null);
    }
  };

  const handleSearch = async () => {
    if (!searchUid.trim()) return;

    try {
      setSearching(true);
      const token = await getIdToken();
      if (!token) return;

      const user = await apiFetch<AdminUser>(
        `/api/admin/users/${encodeURIComponent(searchUid.trim())}`,
        { token }
      );
      setSearchResult(user);
    } catch (err) {
      toast.error(err instanceof Error ? err.message : "User not found");
      setSearchResult(null);
    } finally {
      setSearching(false);
    }
  };

  const handleUsageChange = async (uid: string, newUsage: number) => {
    if (isNaN(newUsage) || newUsage < 0) {
      toast.error("Invalid usage value");
      return;
    }

    try {
      setUpdating(uid);
      const token = await getIdToken();
      if (!token) return;

      await apiFetch(`/api/admin/users/${encodeURIComponent(uid)}/usage`, {
        method: "PATCH",
        token,
        body: { credits_used: newUsage },
      });

      // Update local state
      setUsers((prev) =>
        prev.map((u) => (u.uid === uid ? { ...u, credits_used_this_month: newUsage } : u))
      );
      if (searchResult?.uid === uid) {
        setSearchResult({ ...searchResult, credits_used_this_month: newUsage });
      }

      setEditingUsage(null);
      toast.success(`Usage updated to ${newUsage}`);
    } catch (err) {
      toast.error(err instanceof Error ? err.message : "Failed to update usage");
    } finally {
      setUpdating(null);
    }
  };

  const startEditingUsage = (uid: string, currentUsage: number) => {
    setEditingUsage(uid);
    setUsageValue(String(currentUsage));
  };

  const cancelEditingUsage = () => {
    setEditingUsage(null);
    setUsageValue("");
  };

  const formatDate = (dateStr: string) => {
    return new Date(dateStr).toLocaleDateString("en-US", {
      month: "short",
      day: "numeric",
      year: "numeric",
    });
  };

  const UserRow = ({ user }: { user: AdminUser }) => {
    const isEditing = editingUsage === user.uid;

    return (
      <tr className="border-b border-white/5 hover:bg-white/5">
        <td className="py-3 px-4">
          <div className="font-mono text-xs text-muted-foreground truncate max-w-[120px]">
            {user.uid}
          </div>
          {user.email && <div className="text-sm text-foreground">{user.email}</div>}
        </td>
        <td className="py-3 px-4">
          <Select
            value={user.plan}
            onValueChange={(value) => handlePlanChange(user.uid, value)}
            disabled={updating === user.uid}
          >
            <SelectTrigger className="w-28 h-8">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {VALID_PLANS.map((plan) => (
                <SelectItem key={plan} value={plan}>
                  {plan.charAt(0).toUpperCase() + plan.slice(1)}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </td>
        <td className="py-3 px-4">
          <div className="flex items-center gap-1">
            {isEditing ? (
              <>
                <input
                  type="number"
                  min="0"
                  value={usageValue}
                  onChange={(e) => setUsageValue(e.target.value)}
                  onKeyDown={(e) => {
                    if (e.key === "Enter") {
                      void handleUsageChange(user.uid, parseInt(usageValue, 10));
                    } else if (e.key === "Escape") {
                      cancelEditingUsage();
                    }
                  }}
                  className="w-16 h-7 px-2 text-sm rounded border border-brand-200 dark:border-white/20 bg-white dark:bg-slate-800 text-foreground"
                  // eslint-disable-next-line jsx-a11y/no-autofocus
                  autoFocus
                  disabled={updating === user.uid}
                />
                <span className="text-sm text-muted-foreground">
                  /{user.monthly_credits_limit}
                </span>
                <Button
                  size="sm"
                  variant="ghost"
                  className="h-7 px-2"
                  onClick={() => handleUsageChange(user.uid, parseInt(usageValue, 10))}
                  disabled={updating === user.uid}
                >
                  ✓
                </Button>
                <Button
                  size="sm"
                  variant="ghost"
                  className="h-7 px-2"
                  onClick={cancelEditingUsage}
                  disabled={updating === user.uid}
                >
                  ✕
                </Button>
              </>
            ) : (
              <button
                type="button"
                onClick={() => startEditingUsage(user.uid, user.credits_used_this_month)}
                className="text-sm text-muted-foreground hover:text-foreground transition-colors cursor-pointer"
                title="Click to edit"
              >
                <span className="font-medium text-foreground">
                  {user.credits_used_this_month}
                </span>
                <span className="text-muted-foreground">
                  /{user.monthly_credits_limit}
                </span>
              </button>
            )}
          </div>
        </td>
        <td className="py-3 px-4 text-sm text-muted-foreground">{user.role ?? "-"}</td>
        <td className="py-3 px-4 text-xs text-muted-foreground">
          {formatDate(user.created_at)}
        </td>
      </tr>
    );
  };

  if (loading) {
    return <div className="text-sm text-muted-foreground">Loading users...</div>;
  }

  if (error) {
    return (
      <div className="text-sm text-destructive">
        {error}
        <Button variant="link" onClick={loadUsers} className="ml-2">
          Retry
        </Button>
      </div>
    );
  }

  return (
    <div className="space-y-6">
      {/* Search by UID */}
      <div className="flex gap-2">
        <input
          type="text"
          value={searchUid}
          onChange={(e) => setSearchUid(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && handleSearch()}
          placeholder="Search by User ID..."
          className="flex-1 h-10 rounded-lg border border-brand-100 bg-white px-3 text-sm text-foreground shadow-sm placeholder:text-muted-foreground/70 focus:border-brand-300 focus:ring-2 focus:ring-brand-500/20 outline-none transition-colors dark:border-white/10 dark:bg-slate-900 dark:text-white"
        />
        <Button onClick={handleSearch} disabled={searching || !searchUid.trim()}>
          {searching ? "Searching..." : "Search"}
        </Button>
      </div>

      {/* Search Result */}
      {searchResult && (
        <div className="p-4 rounded-lg border border-brand-200 dark:border-white/10 bg-brand-50 dark:bg-slate-800">
          <h4 className="text-sm font-semibold mb-2">Search Result</h4>
          <table className="w-full text-left">
            <tbody>
              <UserRow user={searchResult} />
            </tbody>
          </table>
        </div>
      )}

      {/* Users Table */}
      <div className="overflow-x-auto">
        <table className="w-full text-left">
          <thead>
            <tr className="border-b border-white/10 text-xs uppercase text-muted-foreground">
              <th className="py-2 px-4 font-medium">User</th>
              <th className="py-2 px-4 font-medium">Plan</th>
              <th className="py-2 px-4 font-medium">Credits</th>
              <th className="py-2 px-4 font-medium">Role</th>
              <th className="py-2 px-4 font-medium">Created</th>
            </tr>
          </thead>
          <tbody>
            {users.map((user) => (
              <UserRow key={user.uid} user={user} />
            ))}
          </tbody>
        </table>
      </div>

      {users.length === 0 && (
        <div className="text-center text-sm text-muted-foreground py-8">
          No users found
        </div>
      )}
    </div>
  );
}
