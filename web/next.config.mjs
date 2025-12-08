/** @type {import('next').NextConfig} */
const nextConfig = {
  reactStrictMode: true,
  // Use Turbopack (default in Next.js 16)
  turbopack: {},
  // Enable standalone output for Docker production builds
  // This creates a minimal production image with only necessary files
  output: "standalone",
  // Optimize images
  images: {
    formats: ["image/avif", "image/webp"],
    // Security: restrict image domains
    remotePatterns: [
      {
        protocol: "https",
        hostname: "**",
      },
    ],
    // Disable dangerous image optimization features
    dangerouslyAllowSVG: false,
    contentDispositionType: "attachment",
    contentSecurityPolicy: "default-src 'self'; script-src 'none'; sandbox;",
  },
  // Security headers
  async headers() {
    const isDev = process.env.NODE_ENV === "development";

    // Content Security Policy
    // Note: 'unsafe-inline' and 'unsafe-eval' are required for Next.js
    // In production, consider using nonces or hashes for stricter CSP
    const cspDirectives = [
      "default-src 'self'",
      `script-src 'self' 'unsafe-inline' ${isDev ? "'unsafe-eval'" : ""} https://www.googletagmanager.com https://www.google-analytics.com https://apis.google.com https://accounts.google.com`,
      "style-src 'self' 'unsafe-inline' https://accounts.google.com https://fonts.googleapis.com",
      "style-src-elem 'self' 'unsafe-inline' https://accounts.google.com https://fonts.googleapis.com",
      "img-src 'self' data: https: blob:",
      "font-src 'self' data: https://fonts.gstatic.com https://fonts.googleapis.com",
      "connect-src 'self' https://*.googleapis.com https://identitytoolkit.googleapis.com https://securetoken.googleapis.com https://apis.google.com https://accounts.google.com https://*.firebaseapp.com https://*.firebaseio.com wss://* ws://* http://localhost:8000 ws://localhost:8000 http://localhost:8001 ws://localhost:8001",
      "frame-src 'self' https://accounts.google.com https://*.googleapis.com https://*.firebaseapp.com https://www.youtube.com https://youtube.com https://*.youtube.com",
      "media-src 'self' blob: https:",
      "object-src 'none'",
      "base-uri 'self'",
      "form-action 'self'",
      "frame-ancestors 'none'",
      "upgrade-insecure-requests",
    ].join("; ");

    // Permissions Policy (formerly Feature Policy)
    const permissionsPolicy = [
      "accelerometer=()",
      "ambient-light-sensor=()",
      "autoplay=()",
      "battery=()",
      "camera=()",
      "cross-origin-isolated=()",
      "display-capture=()",
      "document-domain=()",
      "encrypted-media=()",
      "execution-while-not-rendered=()",
      "execution-while-out-of-viewport=()",
      "fullscreen=(self)",
      "geolocation=()",
      "gyroscope=()",
      "keyboard-map=()",
      "magnetometer=()",
      "microphone=()",
      "midi=()",
      "navigation-override=()",
      "payment=()",
      "picture-in-picture=()",
      "publickey-credentials-get=()",
      "screen-wake-lock=()",
      "sync-xhr=()",
      "usb=()",
      "web-share=()",
      "xr-spatial-tracking=()",
    ].join(", ");

    return [
      {
        source: "/:path*",
        headers: [
          {
            key: "X-DNS-Prefetch-Control",
            value: "on",
          },
          {
            key: "Strict-Transport-Security",
            value: "max-age=63072000; includeSubDomains; preload",
          },
          {
            key: "X-Frame-Options",
            value: "SAMEORIGIN",
          },
          {
            key: "X-Content-Type-Options",
            value: "nosniff",
          },
          {
            key: "X-XSS-Protection",
            value: "1; mode=block",
          },
          {
            key: "Referrer-Policy",
            value: "strict-origin-when-cross-origin",
          },
          {
            key: "Content-Security-Policy",
            value: cspDirectives,
          },
          {
            key: "Permissions-Policy",
            value: permissionsPolicy,
          },
          {
            key: "X-Permitted-Cross-Domain-Policies",
            value: "none",
          },
          // Note: COEP and COOP are relaxed to allow Google Sign-In popups/iframes
          // Google Sign-In requires cross-origin popups, so we need unsafe-none for COOP
          // Consider using signInWithRedirect instead of signInWithPopup for stricter security
          {
            key: "Cross-Origin-Embedder-Policy",
            value: "unsafe-none",
          },
          {
            key: "Cross-Origin-Opener-Policy",
            value: "unsafe-none",
          },
          {
            key: "Cross-Origin-Resource-Policy",
            value: "same-origin",
          },
        ],
      },
    ];
  },
  // Compress responses
  compress: true,
  // Power optimization
  poweredByHeader: false,
  // Security: disable x-powered-by header
  generateEtags: true,
};

export default nextConfig;
