# Firestore Deployment Instructions

## ✅ Configuration Complete

The following files have been created/verified:
- `firebase.json` - Firebase project configuration
- `.firebaserc` - Project ID configuration (viralclipai-prod)
- `firestore.indexes.json` - Index definitions
- `firestore.rules` - Security rules
- `scripts/deploy-firestore.sh` - Deployment script

## 🚀 Deployment Steps

### Option 1: Using the Deployment Script (Recommended)

1. **Authenticate with Firebase** (one-time setup):
   ```bash
   firebase login
   ```

2. **Deploy indexes and rules**:
   ```bash
   ./scripts/deploy-firestore.sh
   ```

### Option 2: Using Makefile

1. **Authenticate with Firebase** (one-time setup):
   ```bash
   firebase login
   ```

2. **Deploy indexes**:
   ```bash
   make firebase-deploy-indexes
   ```

3. **Deploy rules**:
   ```bash
   make firebase-deploy-rules
   ```

4. **Deploy both**:
   ```bash
   make firebase-deploy
   ```

### Option 3: Direct Firebase CLI

1. **Authenticate**:
   ```bash
   firebase login
   ```

2. **Set project**:
   ```bash
   firebase use viralclipai-prod
   ```

3. **Deploy indexes**:
   ```bash
   firebase deploy --only firestore:indexes
   ```

4. **Deploy rules**:
   ```bash
   firebase deploy --only firestore:rules
   ```

## 📊 Indexes Being Deployed

1. **Videos Collection**:
   - `created_at` (DESCENDING) - For listing user videos
   - `status` + `created_at` (ASCENDING + DESCENDING) - For filtering by status

2. **Clips Collection**:
   - `status` + `priority` + `created_at` (all ASCENDING) - For listing clips
   - `style` + `created_at` (ASCENDING) - For filtering by style
   - `scene_id` + `priority` (ASCENDING) - For filtering by scene

## ⏱️ Index Build Time

After deployment, indexes will be in "Building" status. This typically takes:
- **Small collections** (< 10K documents): 1-5 minutes
- **Medium collections** (10K-100K documents): 5-15 minutes
- **Large collections** (> 100K documents): 15-60 minutes

Monitor progress at: https://console.firebase.google.com/project/viralclipai-prod/firestore/indexes

## 🔍 Verify Deployment

After deployment, verify indexes are building:
```bash
firebase firestore:indexes
```

Or check in Firebase Console:
https://console.firebase.google.com/project/viralclipai-prod/firestore/indexes

## 🐛 Troubleshooting

### Error: "Failed to authenticate"
Run `firebase login` first.

### Error: "Project not found"
Verify project ID in `.firebaserc` matches your Firebase project.

### Error: "Index already exists"
This is normal - Firebase will update existing indexes.

### Slow Index Building
Large collections take time. Wait for indexes to finish building before using queries that require them.
