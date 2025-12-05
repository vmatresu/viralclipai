# R2 Configuration Guide

## Quick Setup

Add these to your `.env` file:

```bash
# R2 Jurisdiction-Specific Endpoint (REQUIRED)
R2_ENDPOINT_URL=https://865ceca77988bf70b8e74a8df02132a6.r2.cloudflarestorage.com

# R2 Credentials
R2_ACCOUNT_ID=your-account-id
R2_ACCESS_KEY_ID=your-access-key
R2_SECRET_ACCESS_KEY=your-secret-key
R2_BUCKET_NAME=your-bucket-name
R2_REGION=auto
```

## Where to Find These Values

### 1. R2_ENDPOINT_URL
- **Location**: Cloudflare Dashboard → R2 → Your Bucket → Settings
- **Section**: "Jurisdiction-specific endpoints"
- **Format**: `https://{account-id}.r2.cloudflarestorage.com`
- **Example**: `https://865ceca77988bf70b8e74a8df02132a6.r2.cloudflarestorage.com`

### 2. R2_ACCESS_KEY_ID & R2_SECRET_ACCESS_KEY
- **Location**: Cloudflare Dashboard → R2 → Manage R2 API Tokens
- **Action**: Create API Token
- **Permissions**: 
  - Object Read & Write
  - Bucket Read
- **Scope**: Apply to specific bucket or all buckets

### 3. R2_BUCKET_NAME
- The name you gave your bucket when creating it
- Example: `viralclipai-videos`

### 4. R2_ACCOUNT_ID
- **Location**: Cloudflare Dashboard URL
- **Format**: The subdomain in your Cloudflare URL
- **Example**: If your URL is `dash.cloudflare.com/abc123/r2/buckets`
  - Your account ID is `abc123`

## Why Each Variable is Needed

| Variable | Purpose |
|----------|---------|
| `R2_ENDPOINT_URL` | Tells the S3 SDK where to send requests (Cloudflare's R2 infrastructure) |
| `R2_ACCESS_KEY_ID` | Authentication - Your access key |
| `R2_SECRET_ACCESS_KEY` | Authentication - Your secret key |
| `R2_BUCKET_NAME` | Which bucket to store/retrieve files from |
| `R2_ACCOUNT_ID` | Your Cloudflare account identifier |
| `R2_REGION` | Usually "auto" for R2 (not like AWS regions) |

## Testing Connection

Once configured, the backend will automatically validate the connection on startup.

Check logs for:
```
✓ R2 storage client initialized successfully
✓ R2 connectivity check passed
```

## Public URL Setup (Optional but Recommended)

For serving videos publicly, set up a custom domain:

1. **Add Custom Domain in R2**:
   - Go to your bucket → Settings → Custom Domains
   - Add: `cdn.yourdomain.com` or `clips.yourdomain.com`
   - Choose TLS 1.2 or 1.3

2. **Update Backend Config**:
   ```bash
   # Add to .env
   R2_PUBLIC_URL=https://cdn.yourdomain.com
   ```

3. **Usage**:
   - Upload: Uses R2_ENDPOINT_URL with credentials
   - Public access: Uses R2_PUBLIC_URL without credentials
   - Example public URL: `https://cdn.yourdomain.com/clips/video123.mp4`

## Security Best Practices

✅ **DO**:
- Use API tokens with minimal required permissions
- Set token expiration dates
- Use different tokens for dev/staging/production
- Keep secrets in `.env` (already gitignored)

❌ **DON'T**:
- Commit `.env` to git
- Share API tokens
- Use production tokens in development
- Give tokens excessive permissions

