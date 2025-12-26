import { NextResponse } from "next/server";
import { requireAuth } from "@/lib/server/firebase-admin";
import {
  getPaymentMethods,
  addPaymentMethod,
  deletePaymentMethod,
  setDefaultPaymentMethod,
} from "@/lib/server/billing";

/**
 * GET /api/billing/payment-methods
 * Get payment methods for the authenticated user
 */
export async function GET(request: Request) {
  try {
    const user = await requireAuth(request);
    const paymentMethods = await getPaymentMethods(user.uid);
    return NextResponse.json({ payment_methods: paymentMethods });
  } catch (error) {
    if (error instanceof Error && error.message === "Unauthorized") {
      return NextResponse.json({ error: "Unauthorized" }, { status: 401 });
    }

    console.error("Failed to get payment methods:", error);
    return NextResponse.json(
      { error: "Failed to get payment methods" },
      { status: 500 }
    );
  }
}

/**
 * POST /api/billing/payment-methods
 * Add a new payment method or set default
 */
export async function POST(request: Request) {
  try {
    const user = await requireAuth(request);
    const body = await request.json();

    // Set default action
    if (body.action === "set_default") {
      if (!body.payment_method_id) {
        return NextResponse.json(
          { error: "payment_method_id is required" },
          { status: 400 }
        );
      }

      await setDefaultPaymentMethod(user.uid, body.payment_method_id);
      return NextResponse.json({ success: true });
    }

    // Add new payment method
    if (!body.payment_method_id) {
      return NextResponse.json(
        { error: "payment_method_id is required" },
        { status: 400 }
      );
    }

    if (!user.email) {
      return NextResponse.json(
        { error: "Email is required for billing" },
        { status: 400 }
      );
    }

    const paymentMethod = await addPaymentMethod(
      user.uid,
      user.email,
      body.payment_method_id,
      body.set_as_default ?? false
    );

    return NextResponse.json(paymentMethod);
  } catch (error) {
    if (error instanceof Error && error.message === "Unauthorized") {
      return NextResponse.json({ error: "Unauthorized" }, { status: 401 });
    }

    const message = error instanceof Error ? error.message : "Unknown error";
    console.error("Payment method operation failed:", error);

    return NextResponse.json(
      { error: message },
      { status: 400 }
    );
  }
}

/**
 * DELETE /api/billing/payment-methods
 * Delete a payment method
 */
export async function DELETE(request: Request) {
  try {
    const user = await requireAuth(request);

    const url = new URL(request.url);
    const paymentMethodId = url.searchParams.get("id");

    if (!paymentMethodId) {
      return NextResponse.json(
        { error: "Payment method ID is required" },
        { status: 400 }
      );
    }

    await deletePaymentMethod(user.uid, paymentMethodId);
    return NextResponse.json({ success: true });
  } catch (error) {
    if (error instanceof Error && error.message === "Unauthorized") {
      return NextResponse.json({ error: "Unauthorized" }, { status: 401 });
    }

    const message = error instanceof Error ? error.message : "Unknown error";
    console.error("Failed to delete payment method:", error);

    return NextResponse.json(
      { error: message },
      { status: 400 }
    );
  }
}
