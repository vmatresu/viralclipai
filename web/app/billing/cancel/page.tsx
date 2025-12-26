"use client";

import Link from "next/link";
import { XCircle } from "lucide-react";
import { PageWrapper } from "@/components/landing";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";

export default function BillingCancelPage() {
  return (
    <PageWrapper>
      <div className="flex min-h-[50vh] items-center justify-center">
        <Card className="max-w-md text-center">
          <CardHeader className="space-y-4">
            <div className="mx-auto flex h-16 w-16 items-center justify-center rounded-full bg-yellow-100 dark:bg-yellow-900">
              <XCircle className="h-10 w-10 text-yellow-600 dark:text-yellow-400" />
            </div>
            <CardTitle className="text-2xl">Payment Cancelled</CardTitle>
            <CardDescription>
              Your payment was cancelled and no charges were made.
            </CardDescription>
          </CardHeader>
          <CardContent className="space-y-4">
            <p className="text-sm text-muted-foreground">
              No worries! You can continue using your current plan, or try again
              whenever you&apos;re ready. If you have any questions about our
              plans, feel free to check our pricing page.
            </p>
            <div className="flex flex-col gap-2 pt-4 sm:flex-row sm:justify-center">
              <Button asChild>
                <Link href="/billing">Back to Billing</Link>
              </Button>
              <Button variant="outline" asChild>
                <Link href="/pricing">View Pricing</Link>
              </Button>
            </div>
          </CardContent>
        </Card>
      </div>
    </PageWrapper>
  );
}
