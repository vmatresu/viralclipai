import { NextResponse } from "next/server";
import { requireAuth } from "@/lib/server/firebase-admin";
import { getInvoices } from "@/lib/server/billing";

/**
 * GET /api/billing/invoices
 * Get invoice history for the authenticated user
 */
export async function GET(request: Request) {
  try {
    const user = await requireAuth(request);

    // Get limit from query params
    const url = new URL(request.url);
    const limitParam = url.searchParams.get("limit");
    const limit = limitParam ? Math.min(parseInt(limitParam, 10), 100) : 10;

    const invoices = await getInvoices(user.uid, limit);

    return NextResponse.json({ invoices });
  } catch (error) {
    if (error instanceof Error && error.message === "Unauthorized") {
      return NextResponse.json({ error: "Unauthorized" }, { status: 401 });
    }

    console.error("Failed to get invoices:", error);
    return NextResponse.json(
      { error: "Failed to get invoices" },
      { status: 500 }
    );
  }
}
