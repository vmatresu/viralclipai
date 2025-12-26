"use client";

import { AlertTriangle, Calendar, CreditCard } from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import {
  PLANS,
  formatDate,
  type SubscriptionInfo,
} from "@/types/billing";

interface CurrentPlanCardProps {
  subscription: SubscriptionInfo;
  onCancelClick: () => void;
  onReactivateClick: () => void;
  isLoading?: boolean;
}

export function CurrentPlanCard({
  subscription,
  onCancelClick,
  onReactivateClick,
  isLoading,
}: CurrentPlanCardProps) {
  const planInfo = PLANS.find((p) => p.tier === subscription.plan);
  const isActive = subscription.status === "active";
  const isPastDue = subscription.status === "past_due";
  const isCanceled = subscription.cancel_at_period_end;

  return (
    <Card>
      <CardHeader>
        <div className="flex items-center justify-between">
          <CardTitle>Current Plan</CardTitle>
          <Badge
            variant={
              isPastDue ? "destructive" : isCanceled ? "secondary" : "default"
            }
          >
            {isPastDue
              ? "Past Due"
              : isCanceled
                ? "Canceling"
                : isActive
                  ? "Active"
                  : subscription.status}
          </Badge>
        </div>
        <CardDescription>
          Your current subscription and billing details
        </CardDescription>
      </CardHeader>

      <CardContent className="space-y-4">
        {/* Plan info */}
        <div className="flex items-center justify-between rounded-lg bg-muted p-4">
          <div>
            <p className="text-lg font-semibold">{planInfo?.name ?? "Unknown"} Plan</p>
            <p className="text-sm text-muted-foreground">
              {planInfo?.credits ?? 0} credits/month
            </p>
          </div>
          {subscription.plan !== "free" && subscription.interval && (
            <div className="text-right">
              <p className="text-lg font-semibold">
                $
                {subscription.interval === "monthly"
                  ? planInfo?.monthlyPrice
                  : planInfo?.annualPrice}
              </p>
              <p className="text-sm text-muted-foreground">
                per {subscription.interval === "monthly" ? "month" : "year"}
              </p>
            </div>
          )}
        </div>

        {/* Billing cycle */}
        {subscription.current_period_end && subscription.plan !== "free" && (
          <div className="flex items-center gap-2 text-sm">
            <Calendar className="h-4 w-4 text-muted-foreground" />
            <span>
              {isCanceled ? "Access until: " : "Next billing: "}
              <span className="font-medium">
                {formatDate(subscription.current_period_end)}
              </span>
            </span>
          </div>
        )}

        {/* Past due warning */}
        {isPastDue && (
          <div className="flex items-center gap-2 rounded-lg bg-destructive/10 p-3 text-sm text-destructive">
            <AlertTriangle className="h-4 w-4" />
            <span>
              Payment failed. Please update your payment method to continue
              service.
            </span>
          </div>
        )}

        {/* Cancellation notice */}
        {isCanceled && !isPastDue && (
          <div className="flex items-center gap-2 rounded-lg bg-yellow-500/10 p-3 text-sm text-yellow-600 dark:text-yellow-400">
            <AlertTriangle className="h-4 w-4" />
            <span>
              Your subscription will end on{" "}
              {subscription.current_period_end
                ? formatDate(subscription.current_period_end)
                : "the end of the billing period"}
              . You can reactivate anytime before then.
            </span>
          </div>
        )}

        {/* Actions */}
        {subscription.plan !== "free" && (
          <div className="flex gap-2 pt-2">
            {isCanceled ? (
              <Button
                variant="default"
                onClick={onReactivateClick}
                disabled={isLoading}
              >
                <CreditCard className="mr-2 h-4 w-4" />
                {isLoading ? "Processing..." : "Reactivate Subscription"}
              </Button>
            ) : (
              <Button
                variant="outline"
                onClick={onCancelClick}
                disabled={isLoading}
              >
                {isLoading ? "Processing..." : "Cancel Subscription"}
              </Button>
            )}
          </div>
        )}
      </CardContent>
    </Card>
  );
}
