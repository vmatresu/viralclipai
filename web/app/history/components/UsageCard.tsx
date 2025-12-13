"use client";

import { TrendingUp, Zap } from "lucide-react";

import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";

import { type PlanUsage } from "../types";

interface UsageCardProps {
  planUsage: PlanUsage | null;
  loadingUsage: boolean;
}

export function UsageCard({ planUsage, loadingUsage }: UsageCardProps) {
  if (!planUsage) {
    if (loadingUsage) {
      return (
        <Card className="glass">
          <CardHeader className="pb-2">
            <CardTitle className="text-lg flex items-center gap-2">
              <Zap className="h-5 w-5 text-primary" />
              Plan Usage
            </CardTitle>
          </CardHeader>
          <CardContent>
            <div className="text-sm text-muted-foreground">
              Loading usage information...
            </div>
          </CardContent>
        </Card>
      );
    }
    return (
      <Card className="glass">
        <CardHeader className="pb-2">
          <CardTitle className="text-lg flex items-center gap-2">
            <Zap className="h-5 w-5 text-primary" />
            Plan Usage
          </CardTitle>
        </CardHeader>
        <CardContent>
          <div className="text-sm text-muted-foreground">
            Unable to load usage information.
          </div>
        </CardContent>
      </Card>
    );
  }

  const usagePercentage = Math.min(
    (planUsage.clips_used_this_month / planUsage.max_clips_per_month) * 100,
    100
  );
  const isHighUsage = usagePercentage >= 80;
  const isNearLimit = usagePercentage >= 90;
  const remainingClips = Math.max(
    0,
    planUsage.max_clips_per_month - planUsage.clips_used_this_month
  );

  const storagePercentage = planUsage.storage?.percentage ?? 0;
  const isHighStorage = storagePercentage >= 80;
  const isNearStorageLimit = storagePercentage >= 90;

  const getProgressBarColor = () => {
    if (isNearLimit) return "bg-destructive";
    if (isHighUsage) return "bg-destructive/80";
    return "bg-primary";
  };

  const getStorageBarColor = () => {
    if (isNearStorageLimit) return "bg-destructive";
    if (isHighStorage) return "bg-destructive/80";
    return "bg-primary";
  };

  return (
    <Card className="glass">
      <CardHeader className="pb-2">
        <CardTitle className="text-lg flex items-center gap-2">
          <Zap className="h-5 w-5 text-primary" />
          Plan Usage
        </CardTitle>
        <CardDescription className="flex items-center gap-2">
          <span className="capitalize font-medium text-foreground">
            {planUsage.plan}
          </span>
          <span>Plan</span>
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-4">
        {/* Monthly Clips Usage */}
        <div className="space-y-2">
          <div className="flex justify-between text-sm">
            <span className="text-muted-foreground">Monthly Clips</span>
            <span
              className={
                isHighUsage ? "text-destructive font-semibold" : "text-muted-foreground"
              }
            >
              {planUsage.clips_used_this_month} / {planUsage.max_clips_per_month}
            </span>
          </div>
          <div className="relative h-3 w-full overflow-hidden rounded-full bg-muted">
            <div
              className={`h-full transition-all duration-500 ${getProgressBarColor()}`}
              style={{ width: `${usagePercentage}%` }}
            />
          </div>
          <div className="flex items-center justify-between text-xs text-muted-foreground">
            <span className="flex items-center gap-1">
              <TrendingUp className="h-3 w-3" />
              {remainingClips} remaining
            </span>
            {isHighUsage && (
              <span className="text-destructive">
                {isNearLimit ? "Almost at limit!" : "High usage"}
              </span>
            )}
          </div>
        </div>

        {/* Storage Usage */}
        {planUsage.storage && (
          <div className="space-y-2 pt-2 border-t border-muted">
            <div className="flex justify-between text-sm">
              <span className="text-muted-foreground">Storage</span>
              <span
                className={
                  isHighStorage
                    ? "text-destructive font-semibold"
                    : "text-muted-foreground"
                }
              >
                {planUsage.storage.used_formatted} / {planUsage.storage.limit_formatted}
              </span>
            </div>
            <div className="relative h-3 w-full overflow-hidden rounded-full bg-muted">
              <div
                className={`h-full transition-all duration-500 ${getStorageBarColor()}`}
                style={{ width: `${Math.min(storagePercentage, 100)}%` }}
              />
            </div>
            <div className="flex items-center justify-between text-xs text-muted-foreground">
              <span>{planUsage.storage.total_clips} clips stored</span>
              {isHighStorage && (
                <span className="text-destructive">
                  {isNearStorageLimit ? "Almost full!" : "High usage"}
                </span>
              )}
            </div>
          </div>
        )}
      </CardContent>
    </Card>
  );
}
