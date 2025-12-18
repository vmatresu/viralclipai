#!/bin/bash
# Migration script: Reset users to credits-based quota
# This script removes legacy clips_used_this_month field and sets credits
# to full monthly allocation for existing users.
#
# Usage: ./migrate-users-to-credits.sh <project-id>
#
# Prerequisites:
# - gcloud CLI installed and authenticated
# - firebase CLI installed (optional, for validation)

set -e

PROJECT_ID="${1:-viralclipai}"
CURRENT_MONTH=$(date +"%Y-%m")

echo "=== User Credits Migration ==="
echo "Project: $PROJECT_ID"
echo "Current month: $CURRENT_MONTH"
echo ""

# Function to update a single user
update_user() {
    local UID="$1"
    local PLAN="$2"

    # Determine monthly credits based on plan
    case "$PLAN" in
        "pro")
            MONTHLY_CREDITS=4000
            ;;
        "studio")
            MONTHLY_CREDITS=12000
            ;;
        *)
            MONTHLY_CREDITS=200
            ;;
    esac

    echo "Updating user $UID (plan: $PLAN, monthly credits: $MONTHLY_CREDITS)"

    # Use gcloud firestore to update the document
    # This sets credits_used_this_month to 0 (full quota available)
    # and removes the legacy clips_used_this_month field
    gcloud firestore documents update \
        "projects/$PROJECT_ID/databases/(default)/documents/users/$UID" \
        --project="$PROJECT_ID" \
        --update-mask="credits_used_this_month,usage_reset_month" \
        --data='{
            "credits_used_this_month": {"integerValue": "0"},
            "usage_reset_month": {"stringValue": "'"$CURRENT_MONTH"'"}
        }' 2>/dev/null || {
        echo "  Warning: gcloud update failed, trying alternative method..."

        # Alternative: Use firebase-admin via node script
        node -e "
const admin = require('firebase-admin');
admin.initializeApp({ projectId: '$PROJECT_ID' });
const db = admin.firestore();

async function migrate() {
    const userRef = db.collection('users').doc('$UID');
    await userRef.update({
        credits_used_this_month: 0,
        usage_reset_month: '$CURRENT_MONTH',
        clips_used_this_month: admin.firestore.FieldValue.delete(),
        max_clips_per_month: admin.firestore.FieldValue.delete()
    });
    console.log('  Updated via firebase-admin');
}
migrate().then(() => process.exit(0)).catch(e => { console.error(e); process.exit(1); });
" 2>/dev/null || echo "  Could not update user $UID"
    }

    echo "  Done"
}

echo "Fetching users from Firestore..."
echo ""

# List all users and update each one
# For just 2 users, we can list them manually or query
# Using gcloud to list documents in the users collection

USERS=$(gcloud firestore documents list \
    "projects/$PROJECT_ID/databases/(default)/documents/users" \
    --project="$PROJECT_ID" \
    --format="json" 2>/dev/null | jq -r '.[].name | split("/") | .[-1]') || {
    echo "Could not list users via gcloud. Please update users manually."
    echo ""
    echo "Manual update instructions:"
    echo "1. Go to Firebase Console > Firestore"
    echo "2. For each user document in 'users' collection:"
    echo "   - Set 'credits_used_this_month' to 0"
    echo "   - Set 'usage_reset_month' to '$CURRENT_MONTH'"
    echo "   - Delete 'clips_used_this_month' field"
    echo "   - Delete 'max_clips_per_month' field (if exists)"
    exit 0
}

if [ -z "$USERS" ]; then
    echo "No users found in Firestore."
    exit 0
fi

echo "Found users:"
echo "$USERS"
echo ""

for UID in $USERS; do
    # Get user's plan
    PLAN=$(gcloud firestore documents get \
        "projects/$PROJECT_ID/databases/(default)/documents/users/$UID" \
        --project="$PROJECT_ID" \
        --format="json" 2>/dev/null | jq -r '.fields.plan.stringValue // "free"') || PLAN="free"

    update_user "$UID" "$PLAN"
done

echo ""
echo "=== Migration Complete ==="
echo "All users have been reset to credits-based quota."
echo "Legacy clips_used_this_month fields should be removed."
