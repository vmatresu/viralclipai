#!/usr/bin/env node
/**
 * Migration script: Reset users to credits-based quota
 *
 * This script:
 * 1. Resets credits_used_this_month to 0 for all users
 * 2. Sets usage_reset_month to current month
 * 3. Deletes legacy clips_used_this_month field
 *
 * Usage:
 *   GOOGLE_APPLICATION_CREDENTIALS=/path/to/key.json node migrate-users-to-credits.js
 *
 * cd backend/scripts
 * npm install --no-save firebase-admin
 * GOOGLE_APPLICATION_CREDENTIALS=/Users/valentin/work/viralclipai/firebase-credentials.json \
 * GOOGLE_CLOUD_PROJECT=viralclipai-prod \
 * node migrate-users-to-credits.js
 * Or with emulator:
 *   FIRESTORE_EMULATOR_HOST=localhost:8080 node migrate-users-to-credits.js
 */

const admin = require("firebase-admin");

// Initialize Firebase Admin
if (!admin.apps.length) {
  admin.initializeApp({
    projectId: process.env.GOOGLE_CLOUD_PROJECT || "viralclipai",
  });
}

const db = admin.firestore();

async function migrateUsers() {
  console.log("=== User Credits Migration ===\n");

  const currentMonth = new Date().toISOString().slice(0, 7); // YYYY-MM
  console.log(`Current month: ${currentMonth}\n`);

  try {
    // Get all users
    const usersSnapshot = await db.collection("users").get();

    if (usersSnapshot.empty) {
      console.log("No users found in Firestore.");
      return;
    }

    console.log(`Found ${usersSnapshot.size} users to migrate.\n`);

    const batch = db.batch();
    let count = 0;

    for (const doc of usersSnapshot.docs) {
      const userData = doc.data();
      const uid = doc.id;
      const plan = userData.plan || "free";

      console.log(`Processing user ${uid} (plan: ${plan})`);
      console.log(
        `  Current credits: ${userData.credits_used_this_month || 0}`
      );
      console.log(
        `  Legacy clips: ${userData.clips_used_this_month || "not set"}`
      );

      // Update user document
      batch.update(doc.ref, {
        credits_used_this_month: 0, // Reset to full quota
        usage_reset_month: currentMonth,
        // Delete legacy fields
        clips_used_this_month: admin.firestore.FieldValue.delete(),
        max_clips_per_month: admin.firestore.FieldValue.delete(),
      });

      count++;
      console.log(`  Queued for update (reset to 0 credits used)\n`);
    }

    // Commit all updates
    console.log(`Committing ${count} updates...`);
    await batch.commit();

    console.log("\n=== Migration Complete ===");
    console.log(`Successfully migrated ${count} users to credits-based quota.`);
  } catch (error) {
    console.error("Migration failed:", error);
    process.exit(1);
  }
}

// Run migration
migrateUsers()
  .then(() => process.exit(0))
  .catch((err) => {
    console.error(err);
    process.exit(1);
  });
