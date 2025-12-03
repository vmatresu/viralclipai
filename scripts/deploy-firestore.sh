#!/bin/bash
# Deploy Firestore indexes and rules to Firebase
# 
# Prerequisites:
# 1. Firebase CLI installed: npm install -g firebase-tools
# 2. Authenticated: firebase login
# 3. Project configured: firebase use viralclipai-prod

set -e

echo "🚀 Deploying Firestore configuration..."
echo ""

# Check if Firebase CLI is installed
if ! command -v firebase &> /dev/null; then
    echo "❌ Error: Firebase CLI is not installed"
    echo "Install it with: npm install -g firebase-tools"
    exit 1
fi

# Check if authenticated
if ! firebase projects:list &> /dev/null; then
    echo "❌ Error: Not authenticated with Firebase"
    echo "Run: firebase login"
    exit 1
fi

# Verify project is set (compatible with both BSD and GNU grep)
CURRENT_PROJECT=$(firebase use 2>&1 | grep "Using" | sed -E 's/.*Using ([^ ]+).*/\1/' || echo "")
if [ "$CURRENT_PROJECT" != "viralclipai-prod" ]; then
    echo "⚠️  Setting Firebase project to viralclipai-prod..."
    firebase use viralclipai-prod
fi

echo "📋 Deploying Firestore security rules..."
firebase deploy --only firestore:rules

echo ""
echo "📊 Deploying Firestore indexes..."
firebase deploy --only firestore:indexes

echo ""
echo "✅ Deployment complete!"
echo ""
echo "📝 Note: Indexes may take a few minutes to build. Check status at:"
echo "   https://console.firebase.google.com/project/viralclipai-prod/firestore/indexes"

