import { NextResponse } from "next/server";
import { requireAuth } from "@/lib/server/firebase-admin";
import {
  getSubscriptionInfo,
  changePlan,
  cancelSubscription,
  reactivateSubscription,
  previewPlanChange,
} from "@/lib/server/billing";

/**
 * GET /api/billing/subscription
 * Get current subscription info
 */
export async function GET(request: Request) {
  try {
    const user = await requireAuth(request);
    const subscription = await getSubscriptionInfo(user.uid);
    return NextResponse.json(subscription);
  } catch (error) {
    if (error instanceof Error && error.message === "Unauthorized") {
      return NextResponse.json({ error: "Unauthorized" }, { status: 401 });
    }
    console.error("Failed to get subscription:", error);
    return NextResponse.json(
      { error: "Failed to get subscription" },
      { status: 500 }
    );
  }
}

/**
 * POST /api/billing/subscription
 * Handle subscription actions: preview, change, cancel, reactivate
 */
export async function POST(request: Request) {
  try {
    const user = await requireAuth(request);
    const body = await request.json();
    const { action, ...params } = body;

    switch (action) {
      case "preview": {
        const preview = await previewPlanChange(user.uid, params.price_id);
        return NextResponse.json(preview);
      }

      case "change": {
        const result = await changePlan(
          user.uid,
          params.price_id,
          params.prorate ?? true
        );
        return NextResponse.json(result);
      }

      case "cancel": {
        const result = await cancelSubscription(
          user.uid,
          params.immediately ?? false
        );
        return NextResponse.json(result);
      }

      case "reactivate": {
        const result = await reactivateSubscription(user.uid);
        return NextResponse.json(result);
      }

      default:
        return NextResponse.json(
          { error: "Invalid action" },
          { status: 400 }
        );
    }
  } catch (error) {
    if (error instanceof Error && error.message === "Unauthorized") {
      return NextResponse.json({ error: "Unauthorized" }, { status: 401 });
    }

    const message = error instanceof Error ? error.message : "Unknown error";
    console.error("Subscription action failed:", error);

    return NextResponse.json(
      { error: message },
      { status: 400 }
    );
  }
}
