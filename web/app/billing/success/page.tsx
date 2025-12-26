"use client";

import { useEffect, useState } from "react";
import Link from "next/link";
import { CheckCircle } from "lucide-react";
import { PageWrapper } from "@/components/landing";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { useAuth } from "@/lib/auth";
import { getSubscription } from "@/lib/apiClient";
import { PLANS } from "@/types/billing";

export default function BillingSuccessPage() {
  const { getIdToken, user, loading: authLoading } = useAuth();
  const [planName, setPlanName] = useState<string>("your new plan");
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    async function fetchSubscription() {
      if (authLoading || !user) return;

      try {
        const token = await getIdToken();
        if (!token) return;

        const subscription = await getSubscription(token);
        const plan = PLANS.find((p) => p.tier === subscription.plan);
        if (plan) {
          setPlanName(plan.name);
        }
      } catch {
        // Ignore errors - we'll show a generic message
      } finally {
        setLoading(false);
      }
    }

    void fetchSubscription();
  }, [authLoading, user, getIdToken]);

  return (
    <PageWrapper>
      <div className="flex min-h-[50vh] items-center justify-center">
        <Card className="max-w-md text-center">
          <CardHeader className="space-y-4">
            <div className="mx-auto flex h-16 w-16 items-center justify-center rounded-full bg-green-100 dark:bg-green-900">
              <CheckCircle className="h-10 w-10 text-green-600 dark:text-green-400" />
            </div>
            <CardTitle className="text-2xl">Payment Successful!</CardTitle>
            <CardDescription>
              {loading
                ? "Loading your subscription details..."
                : `Welcome to the ${planName} plan! Your subscription is now active.`}
            </CardDescription>
          </CardHeader>
          <CardContent className="space-y-4">
            <p className="text-sm text-muted-foreground">
              Thank you for upgrading. Your new credits and features are now
              available. You can manage your subscription at any time from the
              billing page.
            </p>
            <div className="flex flex-col gap-2 pt-4 sm:flex-row sm:justify-center">
              <Button asChild>
                <Link href="/analyze">Start Creating</Link>
              </Button>
              <Button variant="outline" asChild>
                <Link href="/billing">View Billing</Link>
              </Button>
            </div>
          </CardContent>
        </Card>
      </div>
    </PageWrapper>
  );
}
