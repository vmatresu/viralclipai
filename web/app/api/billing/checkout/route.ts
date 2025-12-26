import { NextResponse } from "next/server";
import { requireAuth } from "@/lib/server/firebase-admin";
import { createCheckoutSession } from "@/lib/server/billing";

interface CheckoutRequest {
  price_id: string;
  success_url: string;
  cancel_url: string;
}

/**
 * POST /api/billing/checkout
 * Create a Stripe Checkout session for new subscriptions
 */
export async function POST(request: Request) {
  try {
    const user = await requireAuth(request);
    const body: CheckoutRequest = await request.json();

    // Validate required fields
    if (!body.price_id || !body.success_url || !body.cancel_url) {
      return NextResponse.json(
        { error: "Missing required fields: price_id, success_url, cancel_url" },
        { status: 400 }
      );
    }

    // Validate URLs
    try {
      new URL(body.success_url);
      new URL(body.cancel_url);
    } catch {
      return NextResponse.json(
        { error: "Invalid success_url or cancel_url" },
        { status: 400 }
      );
    }

    // User email is required for checkout
    if (!user.email) {
      return NextResponse.json(
        { error: "Email is required for billing" },
        { status: 400 }
      );
    }

    const result = await createCheckoutSession(
      user.uid,
      user.email,
      body.price_id,
      body.success_url,
      body.cancel_url
    );

    return NextResponse.json(result);
  } catch (error) {
    if (error instanceof Error && error.message === "Unauthorized") {
      return NextResponse.json({ error: "Unauthorized" }, { status: 401 });
    }

    const message = error instanceof Error ? error.message : "Unknown error";
    console.error("Checkout failed:", error);

    return NextResponse.json(
      { error: message },
      { status: 400 }
    );
  }
}
