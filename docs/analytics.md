# Analytics Implementation

This document describes the Firebase Analytics implementation for the Viral Clip AI application.

## Overview

The analytics system is built with production-ready best practices including:
- **Type Safety**: Strong TypeScript types for all events and parameters
- **Privacy Compliance**: Respects Do Not Track and user consent
- **Performance**: Lazy initialization, event queuing, and efficient tracking
- **Error Resilience**: Graceful degradation when analytics is unavailable
- **Data Validation**: Automatic sanitization to meet Firebase limits
- **Modular Architecture**: Clean separation of concerns

## Architecture

### Core Module: `web/lib/analytics.ts`

The main analytics module provides:

1. **Initialization**: `initAnalytics()` - Lazy initialization with proper checks
2. **Event Tracking**: `trackEvent()` - Generic event tracking with validation
3. **User Management**: `setAnalyticsUserId()`, `setAnalyticsUserProperties()`
4. **Privacy Controls**: `setAnalyticsEnabled()` - Enable/disable analytics
5. **Predefined Events**: Type-safe helper functions for common events

### Page View Tracking: `web/lib/usePageView.ts`

React hook for automatic page view tracking:

```typescript
import { usePageView } from "@/lib/usePageView";

export default function MyPage() {
  usePageView("page_name");
  // ...
}
```

## Event Types

### Authentication Events
- `user_signed_in` - User successfully signed in
- `user_signed_out` - User signed out
- `sign_in_attempted` - User attempted to sign in
- `sign_in_failed` - Sign in failed with error details

### Video Processing Events
- `video_processing_started` - Processing initiated
- `video_processing_completed` - Processing finished successfully
- `video_processing_failed` - Processing failed with error details
- `video_processing_cancelled` - Processing was cancelled

### Clip Events
- `clip_downloaded` - User downloaded a clip
- `clip_copied_link` - User copied clip link
- `clip_published_tiktok` - Clip published to TikTok (success/failure)
- `clip_published_failed` - TikTok publish failed

### Navigation Events
- `page_view` - Page viewed
- `navigation_clicked` - Navigation link clicked

### Engagement Events
- `cta_clicked` - Call-to-action button clicked
- `feature_viewed` - Feature section viewed
- `error_encountered` - Error occurred

## Usage Examples

### Basic Event Tracking

```typescript
import { trackEvent } from "@/lib/analytics";

// Generic event
await trackEvent("custom_event", {
  custom_param: "value",
});
```

### Using Predefined Events

```typescript
import { analyticsEvents } from "@/lib/analytics";

// Video processing
analyticsEvents.videoProcessingStarted({
  style: "split",
  hasCustomPrompt: true,
  videoUrl: "https://youtube.com/...",
});

analyticsEvents.videoProcessingCompleted({
  videoId: "video-123",
  style: "split",
  clipsGenerated: 5,
  durationMs: 120000,
  hasCustomPrompt: true,
});

// Clip interactions
analyticsEvents.clipDownloaded({
  clipId: "clip-123",
  clipName: "clip_01_01_title_split.mp4",
  style: "split",
});

// CTA clicks
analyticsEvents.ctaClicked({
  ctaName: "try_it_now",
  location: "home",
});
```

### User Management

```typescript
import { setAnalyticsUserId, setAnalyticsUserProperties } from "@/lib/analytics";

// Set user ID (automatically called on sign in)
await setAnalyticsUserId("user-123");

// Set user properties
await setAnalyticsUserProperties({
  subscription_tier: "premium",
  account_type: "creator",
});
```

## Privacy & Compliance

### Do Not Track Support

The implementation automatically respects the `DNT` (Do Not Track) header:

```typescript
// Automatically checked in shouldEnableAnalytics()
if (navigator.doNotTrack === "1") {
  // Analytics disabled
}
```

### User Consent

Analytics can be disabled via localStorage:

```typescript
import { setAnalyticsEnabled } from "@/lib/analytics";

// Disable analytics
await setAnalyticsEnabled(false);

// Enable analytics
await setAnalyticsEnabled(true);
```

The consent preference is stored in `localStorage` as `analytics_consent`.

## Data Validation & Sanitization

All events are automatically sanitized to meet Firebase Analytics requirements:

- **Event Names**: Max 40 characters, alphanumeric + underscores only
- **Parameter Names**: Max 40 characters, alphanumeric + underscores only
- **Parameter Values**: Max 100 characters for strings
- **Parameters per Event**: Max 25 parameters

Invalid characters are replaced with underscores, and values are truncated if necessary.

## Error Handling

The analytics system is designed to fail gracefully:

1. **Initialization Failures**: Logged but don't break the app
2. **Event Tracking Failures**: Logged but don't interrupt user flow
3. **Missing Configuration**: Analytics silently disabled
4. **Unsupported Environments**: Automatically detected and disabled

## Performance Considerations

1. **Lazy Initialization**: Analytics only initializes when needed
2. **Event Queuing**: Events are queued before initialization and processed after
3. **Non-Blocking**: All tracking is asynchronous and non-blocking
4. **Debug Mode**: Detailed logging only in development

## Configuration

### Environment Variables

```bash
# Required for analytics to work
NEXT_PUBLIC_FIREBASE_MEASUREMENT_ID=G-XXXXXXXXXX
```

### Configuration Options

The analytics system can be configured via the `DEFAULT_CONFIG` object:

```typescript
const DEFAULT_CONFIG: AnalyticsConfig = {
  enabled: true,
  debug: process.env.NODE_ENV === "development",
  respectDoNotTrack: true,
  maxEventQueueSize: 50,
  batchIntervalMs: 1000,
};
```

## Integration Points

### Authentication (`web/lib/auth.tsx`)
- Tracks sign in/out events
- Sets user ID on authentication

### Video Processing (`web/components/ProcessingClient.tsx`)
- Tracks processing start/completion/failure
- Measures processing duration
- Tracks custom prompt usage

### Clip Management (`web/components/ClipGrid.tsx`)
- Tracks downloads, link copies, TikTok publishing

### Pages
- Automatic page view tracking via `usePageView` hook
- CTA click tracking on home page

## Best Practices

1. **Use Predefined Events**: Prefer `analyticsEvents` helpers over raw `trackEvent`
2. **Include Context**: Always include relevant context (page name, user state, etc.)
3. **Don't Track PII**: Never track personally identifiable information
4. **Consistent Naming**: Use snake_case for event and parameter names
5. **Test in Development**: Check analytics events in Firebase console during development

## Testing

In development mode, analytics events are logged to the console:

```
Analytics event: video_processing_started { style: 'split', has_custom_prompt: true }
```

Use Firebase Analytics DebugView to see events in real-time during development.

## Monitoring

Monitor analytics health via:

1. **Firebase Console**: Check event counts and parameters
2. **Error Logs**: Monitor frontend logs for analytics errors
3. **User Feedback**: Track if analytics affects user experience

## Future Enhancements

Potential improvements:

1. **Event Batching**: Batch multiple events for better performance
2. **Custom Dimensions**: Add custom dimensions for better segmentation
3. **Conversion Tracking**: Track conversion funnels
4. **A/B Testing**: Integrate with Firebase A/B Testing
5. **Audience Building**: Create audiences based on user behavior

