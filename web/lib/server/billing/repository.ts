/**
 * Billing data repository.
 *
 * Firestore data access layer for billing operations.
 * This module handles all database operations for user billing data.
 */

import { getAdminFirestore } from "../firebase-admin";
import type { UserBillingData } from "./types";

/**
 * Get user billing data from Firestore.
 */
export async function getUserBillingData(uid: string): Promise<UserBillingData> {
  const db = getAdminFirestore();
  const doc = await db.collection("users").doc(uid).get();

  if (!doc.exists) {
    return {};
  }

  const data = doc.data();
  return {
    stripe_customer_id: data?.stripe_customer_id,
    stripe_subscription_id: data?.stripe_subscription_id,
    subscription_status: data?.subscription_status,
    current_period_end: data?.current_period_end,
    cancel_at_period_end: data?.cancel_at_period_end,
    plan: data?.plan,
  };
}

/**
 * Update user billing data in Firestore.
 * Uses merge to only update specified fields.
 */
export async function updateUserBillingData(
  uid: string,
  data: Partial<UserBillingData>
): Promise<void> {
  const db = getAdminFirestore();
  const { FieldValue } = await import("firebase-admin/firestore");

  await db
    .collection("users")
    .doc(uid)
    .set(
      {
        ...data,
        updated_at: FieldValue.serverTimestamp(),
      },
      { merge: true }
    );
}

/**
 * Find user ID by Stripe customer ID.
 */
export async function findUserByCustomerId(
  customerId: string
): Promise<string | null> {
  const db = getAdminFirestore();
  const snapshot = await db
    .collection("users")
    .where("stripe_customer_id", "==", customerId)
    .limit(1)
    .get();

  if (snapshot.empty || snapshot.docs.length === 0) {
    return null;
  }

  const doc = snapshot.docs[0];
  return doc ? doc.id : null;
}

/**
 * Reset monthly credits for a user (called on subscription renewal).
 */
export async function resetMonthlyCredits(uid: string): Promise<void> {
  const db = getAdminFirestore();
  const { FieldValue } = await import("firebase-admin/firestore");

  const currentMonth = new Date().toISOString().slice(0, 7); // YYYY-MM

  await db.collection("users").doc(uid).update({
    credits_used_this_month: 0,
    usage_reset_month: currentMonth,
    updated_at: FieldValue.serverTimestamp(),
  });
}

/**
 * Downgrade user to free plan.
 */
export async function downgradeToFree(uid: string): Promise<void> {
  await updateUserBillingData(uid, {
    plan: "free",
    stripe_subscription_id: undefined,
    subscription_status: "none",
    current_period_end: undefined,
    cancel_at_period_end: false,
  });
}
