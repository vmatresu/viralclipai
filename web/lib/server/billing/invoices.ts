/**
 * Invoice operations.
 *
 * Handles listing invoice history.
 */

import { stripe } from "../stripe";
import { getUserBillingData } from "./repository";
import type { Invoice } from "./types";

/** Maximum number of invoices to return */
const MAX_INVOICES = 100;

/**
 * Get invoice history for a user.
 *
 * @param limit - Maximum number of invoices to return (default: 10, max: 100)
 */
export async function getInvoices(
  uid: string,
  limit: number = 10
): Promise<Invoice[]> {
  const billingData = await getUserBillingData(uid);

  if (!billingData.stripe_customer_id) {
    return [];
  }

  const invoices = await stripe.invoices.list({
    customer: billingData.stripe_customer_id,
    limit: Math.min(limit, MAX_INVOICES),
  });

  return invoices.data.map((inv) => ({
    id: inv.id,
    number: inv.number ?? undefined,
    amount_cents: inv.amount_due,
    currency: inv.currency,
    status: inv.status || "unknown",
    created_at: new Date((inv.created ?? 0) * 1000).toISOString(),
    hosted_invoice_url: inv.hosted_invoice_url ?? undefined,
    invoice_pdf_url: inv.invoice_pdf ?? undefined,
  }));
}
