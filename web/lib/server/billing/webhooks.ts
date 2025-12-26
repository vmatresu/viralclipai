/**
 * Stripe webhook event handlers.
 *
 * Processes webhook events from Stripe to keep subscription state in sync.
 */

import type Stripe from "stripe";
import { stripe } from "../stripe";
import {
  findUserByCustomerId,
  updateUserBillingData,
  resetMonthlyCredits,
  downgradeToFree,
} from "./repository";
import { syncSubscriptionToFirestore } from "./subscriptions";

/**
 * Handle checkout.session.completed event.
 *
 * Called when a user successfully completes checkout.
 */
export async function handleCheckoutCompleted(
  session: Stripe.Checkout.Session
): Promise<void> {
  const customerId = extractCustomerId(session.customer);
  const subscriptionId = extractSubscriptionId(session.subscription);

  if (!customerId || !subscriptionId) {
    console.warn("Checkout session missing customer or subscription");
    return;
  }

  const uid = await findUserByCustomerId(customerId);
  if (!uid) {
    console.error(`No user found for customer ${customerId}`);
    return;
  }

  const subscription = await stripe.subscriptions.retrieve(subscriptionId);
  await syncSubscriptionToFirestore(uid, subscription);

  console.log(`Checkout completed for user ${uid}`);
}

/**
 * Handle customer.subscription.updated event.
 *
 * Called when a subscription is modified (plan change, status change, etc).
 */
export async function handleSubscriptionUpdated(
  subscription: Stripe.Subscription
): Promise<void> {
  const customerId = extractCustomerId(subscription.customer);
  if (!customerId) {
    console.error("Subscription missing customer ID");
    return;
  }

  const uid = await findUserByCustomerId(customerId);
  if (!uid) {
    console.error(`No user found for customer ${customerId}`);
    return;
  }

  await syncSubscriptionToFirestore(uid, subscription);

  console.log(`Subscription ${subscription.id} updated for user ${uid}`);
}

/**
 * Handle customer.subscription.deleted event.
 *
 * Called when a subscription is canceled/deleted.
 */
export async function handleSubscriptionDeleted(
  subscription: Stripe.Subscription
): Promise<void> {
  const customerId = extractCustomerId(subscription.customer);
  if (!customerId) {
    console.error("Subscription missing customer ID");
    return;
  }

  const uid = await findUserByCustomerId(customerId);
  if (!uid) {
    console.error(`No user found for customer ${customerId}`);
    return;
  }

  // Downgrade to free
  await downgradeToFree(uid);

  console.log(
    `Subscription ${subscription.id} deleted, user ${uid} downgraded to free`
  );
}

/**
 * Handle invoice.paid event.
 *
 * Called when an invoice is successfully paid.
 * Resets monthly credits on subscription renewal.
 */
export async function handleInvoicePaid(invoice: Stripe.Invoice): Promise<void> {
  const customerId = extractCustomerId(invoice.customer);
  if (!customerId) {
    return;
  }

  const uid = await findUserByCustomerId(customerId);
  if (!uid) {
    console.error(`No user found for customer ${customerId}`);
    return;
  }

  // Reset monthly credits on subscription renewal
  if (invoice.billing_reason === "subscription_cycle") {
    await resetMonthlyCredits(uid);
    console.log(`Reset monthly credits for user ${uid}`);
  }
}

/**
 * Handle invoice.payment_failed event.
 *
 * Called when a payment attempt fails.
 */
export async function handleInvoicePaymentFailed(
  invoice: Stripe.Invoice
): Promise<void> {
  const customerId = extractCustomerId(invoice.customer);
  if (!customerId) {
    return;
  }

  const uid = await findUserByCustomerId(customerId);
  if (!uid) {
    console.error(`No user found for customer ${customerId}`);
    return;
  }

  // Mark as past_due
  await updateUserBillingData(uid, {
    subscription_status: "past_due",
  });

  console.warn(`Payment failed for user ${uid}`);
}

// =============================================================================
// Helpers
// =============================================================================

/**
 * Extract customer ID from various Stripe object formats.
 */
function extractCustomerId(
  customer: string | Stripe.Customer | Stripe.DeletedCustomer | null | undefined
): string | null {
  if (!customer) return null;
  if (typeof customer === "string") return customer;
  return customer.id;
}

/**
 * Extract subscription ID from various Stripe object formats.
 */
function extractSubscriptionId(
  subscription: string | Stripe.Subscription | null | undefined
): string | null {
  if (!subscription) return null;
  if (typeof subscription === "string") return subscription;
  return subscription.id;
}
