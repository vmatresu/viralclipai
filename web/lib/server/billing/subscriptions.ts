/**
 * Subscription management.
 *
 * Handles subscription retrieval, checkout, plan changes, and cancellation.
 */

import type Stripe from "stripe";
import { stripe, getPlanTierFromPriceId, isValidPriceId } from "../stripe";
import { getUserBillingData, updateUserBillingData } from "./repository";
import { getOrCreateCustomer } from "./customers";
import type {
  SubscriptionInfo,
  SubscriptionStatus,
  InvoicePreview,
  CheckoutResult,
} from "./types";

// =============================================================================
// Subscription Info
// =============================================================================

/**
 * Get subscription information for a user.
 */
export async function getSubscriptionInfo(uid: string): Promise<SubscriptionInfo> {
  const billingData = await getUserBillingData(uid);

  // No subscription - free tier
  if (!billingData.stripe_subscription_id) {
    return {
      plan: (billingData.plan as "free" | "pro" | "studio") || "free",
      status: "none",
      cancel_at_period_end: false,
      stripe_customer_id: billingData.stripe_customer_id,
    };
  }

  // Fetch subscription from Stripe
  const subscription = (await stripe.subscriptions.retrieve(
    billingData.stripe_subscription_id
  )) as Stripe.Subscription;

  // Get price ID to determine plan
  const priceId = subscription.items.data[0]?.price?.id;
  const planTier = priceId ? getPlanTierFromPriceId(priceId) : null;

  // Determine billing interval
  const interval = subscription.items.data[0]?.price?.recurring?.interval;
  const billingInterval = interval === "year" ? "annual" : "monthly";

  // Access period times safely (they're timestamps)
  const periodStart = (subscription as { current_period_start?: number })
    .current_period_start;
  const periodEnd = (subscription as { current_period_end?: number })
    .current_period_end;

  return {
    plan: planTier || "free",
    status: mapStripeStatus(subscription.status),
    interval: billingInterval,
    current_period_start: periodStart
      ? new Date(periodStart * 1000).toISOString()
      : undefined,
    current_period_end: periodEnd
      ? new Date(periodEnd * 1000).toISOString()
      : undefined,
    cancel_at_period_end: subscription.cancel_at_period_end,
    stripe_subscription_id: subscription.id,
    stripe_customer_id: billingData.stripe_customer_id,
  };
}

/**
 * Map Stripe subscription status to our status type.
 */
function mapStripeStatus(status: Stripe.Subscription.Status): SubscriptionStatus {
  switch (status) {
    case "active":
      return "active";
    case "past_due":
      return "past_due";
    case "canceled":
      return "canceled";
    case "trialing":
      return "trialing";
    case "incomplete":
    case "incomplete_expired":
    case "unpaid":
      return "past_due";
    default:
      return "none";
  }
}

// =============================================================================
// Checkout
// =============================================================================

/**
 * Create a Stripe Checkout session for new subscriptions.
 */
export async function createCheckoutSession(
  uid: string,
  email: string,
  priceId: string,
  successUrl: string,
  cancelUrl: string
): Promise<CheckoutResult> {
  if (!isValidPriceId(priceId)) {
    throw new Error("Invalid price ID");
  }

  const customerId = await getOrCreateCustomer(uid, email);

  const session = await stripe.checkout.sessions.create({
    customer: customerId,
    mode: "subscription",
    payment_method_types: ["card"],
    line_items: [
      {
        price: priceId,
        quantity: 1,
      },
    ],
    success_url: successUrl,
    cancel_url: cancelUrl,
    automatic_tax: { enabled: true },
    allow_promotion_codes: true,
    billing_address_collection: "auto",
    metadata: {
      firebase_uid: uid,
    },
  });

  if (!session.url) {
    throw new Error("Failed to create checkout session");
  }

  return {
    checkout_url: session.url,
    session_id: session.id,
  };
}

// =============================================================================
// Plan Changes
// =============================================================================

/**
 * Preview invoice for a plan change.
 */
export async function previewPlanChange(
  uid: string,
  newPriceId: string
): Promise<InvoicePreview> {
  if (!isValidPriceId(newPriceId)) {
    throw new Error("Invalid price ID");
  }

  const billingData = await getUserBillingData(uid);

  if (!billingData.stripe_subscription_id) {
    throw new Error("No active subscription");
  }

  // Get current subscription
  const subscription = await stripe.subscriptions.retrieve(
    billingData.stripe_subscription_id
  );

  const subscriptionItemId = subscription.items.data[0]?.id;
  if (!subscriptionItemId) {
    throw new Error("Subscription has no items");
  }

  // Preview the upcoming invoice with the new price
  const invoice = await stripe.invoices.createPreview({
    customer: billingData.stripe_customer_id!,
    subscription: billingData.stripe_subscription_id,
    subscription_details: {
      items: [
        {
          id: subscriptionItemId,
          price: newPriceId,
        },
      ],
      proration_behavior: "create_prorations",
    },
  });

  // Build line items - detect proration from description or amount sign
  const lineItems = (invoice.lines?.data || []).map((line) => {
    const description = line.description || "Subscription";
    const isProration =
      description.includes("Unused") ||
      description.includes("Remaining") ||
      line.amount < 0;
    return {
      description,
      amount_cents: line.amount,
      currency: line.currency,
      proration: isProration,
    };
  });

  const totalCents = invoice.total;

  // Generate human-readable summary
  let summary: string;
  if (totalCents > 0) {
    summary = `You will be charged $${(totalCents / 100).toFixed(2)} now for the upgrade.`;
  } else if (totalCents < 0) {
    summary = `You will receive $${(Math.abs(totalCents) / 100).toFixed(2)} credit on your next invoice.`;
  } else {
    summary = "No immediate charge - your next billing will reflect the new plan.";
  }

  return {
    line_items: lineItems,
    subtotal_cents: invoice.subtotal,
    total_cents: totalCents,
    currency: invoice.currency,
    summary,
  };
}

/**
 * Change subscription plan.
 */
export async function changePlan(
  uid: string,
  newPriceId: string,
  prorate: boolean = true
): Promise<SubscriptionInfo> {
  if (!isValidPriceId(newPriceId)) {
    throw new Error("Invalid price ID");
  }

  const billingData = await getUserBillingData(uid);

  if (!billingData.stripe_subscription_id) {
    throw new Error("No active subscription");
  }

  // Get current subscription
  const subscription = await stripe.subscriptions.retrieve(
    billingData.stripe_subscription_id
  );

  const subscriptionItemId = subscription.items.data[0]?.id;
  if (!subscriptionItemId) {
    throw new Error("Subscription has no items");
  }

  // Update subscription
  const updatedSubscription = await stripe.subscriptions.update(
    billingData.stripe_subscription_id,
    {
      items: [
        {
          id: subscriptionItemId,
          price: newPriceId,
        },
      ],
      proration_behavior: prorate ? "create_prorations" : "none",
    }
  );

  // Sync to Firestore
  await syncSubscriptionToFirestore(uid, updatedSubscription);

  return getSubscriptionInfo(uid);
}

// =============================================================================
// Cancellation
// =============================================================================

/**
 * Cancel subscription.
 *
 * @param immediately - If true, cancel now. If false, cancel at period end.
 */
export async function cancelSubscription(
  uid: string,
  immediately: boolean = false
): Promise<SubscriptionInfo> {
  const billingData = await getUserBillingData(uid);

  if (!billingData.stripe_subscription_id) {
    throw new Error("No active subscription");
  }

  if (immediately) {
    // Cancel immediately
    await stripe.subscriptions.cancel(billingData.stripe_subscription_id);

    // Downgrade to free in Firestore
    await updateUserBillingData(uid, {
      plan: "free",
      stripe_subscription_id: undefined,
      subscription_status: "none",
      current_period_end: undefined,
      cancel_at_period_end: false,
    });
  } else {
    // Cancel at period end
    const subscription = await stripe.subscriptions.update(
      billingData.stripe_subscription_id,
      {
        cancel_at_period_end: true,
      }
    );

    await syncSubscriptionToFirestore(uid, subscription);
  }

  return getSubscriptionInfo(uid);
}

/**
 * Reactivate a subscription that was scheduled to cancel.
 */
export async function reactivateSubscription(
  uid: string
): Promise<SubscriptionInfo> {
  const billingData = await getUserBillingData(uid);

  if (!billingData.stripe_subscription_id) {
    throw new Error("No active subscription");
  }

  const subscription = await stripe.subscriptions.update(
    billingData.stripe_subscription_id,
    {
      cancel_at_period_end: false,
    }
  );

  await syncSubscriptionToFirestore(uid, subscription);

  return getSubscriptionInfo(uid);
}

// =============================================================================
// Sync Helper
// =============================================================================

/**
 * Sync subscription data to Firestore.
 * Called after subscription changes and from webhooks.
 */
export async function syncSubscriptionToFirestore(
  uid: string,
  subscription: Stripe.Subscription
): Promise<void> {
  const priceId = subscription.items.data[0]?.price?.id;
  const planTier = priceId ? getPlanTierFromPriceId(priceId) : "free";

  // Access current_period_end safely
  const periodEnd = (subscription as unknown as { current_period_end?: number })
    .current_period_end;

  await updateUserBillingData(uid, {
    plan: planTier || "free",
    stripe_subscription_id: subscription.id,
    subscription_status: subscription.status,
    current_period_end: periodEnd,
    cancel_at_period_end: subscription.cancel_at_period_end,
  });
}
