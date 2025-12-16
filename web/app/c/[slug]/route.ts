import { NextResponse } from "next/server";

import type { NextRequest } from "next/server";

/**
 * Public share link resolution route.
 *
 * GET /c/{share_slug}
 *
 * This route proxies to the backend API which resolves the share slug
 * and returns a 302 redirect to a short-lived presigned URL for playback.
 */
export async function GET(
  request: NextRequest,
  { params }: { params: { slug: string } }
): Promise<NextResponse> {
  const { slug } = params;

  // Validate slug format (alphanumeric, 8-16 chars)
  if (!slug || !/^[a-zA-Z0-9]{8,16}$/.test(slug)) {
    return NextResponse.json({ error: "Invalid share link" }, { status: 400 });
  }

  // Get backend API URL from environment
  // Fallback to production API domain to avoid 503 when env vars are missing
  const apiUrl =
    process.env.NEXT_PUBLIC_API_URL ??
    process.env.API_URL ??
    "https://api.viralclipai.io";
  if (!apiUrl) {
    console.error("[share-route] API_URL not configured");
    return NextResponse.json({ error: "Service unavailable" }, { status: 503 });
  }

  try {
    // Proxy to backend /c/{share_slug} endpoint
    const backendUrl = `${apiUrl}/c/${encodeURIComponent(slug)}`;

    const response = await fetch(backendUrl, {
      method: "GET",
      redirect: "manual", // Don't follow redirects, we want to return them
      headers: {
        Accept: "video/mp4,*/*",
        "User-Agent": request.headers.get("user-agent") ?? "ViralClipAI-Web",
      },
    });

    // If backend returns a redirect (302), pass it through
    if (response.status === 302 || response.status === 301) {
      const location = response.headers.get("location");
      if (location) {
        return NextResponse.redirect(location, response.status === 301 ? 301 : 302);
      }
    }

    // If backend returns an error, pass it through
    if (!response.ok) {
      const contentType = response.headers.get("content-type") ?? "";

      if (contentType.includes("application/json")) {
        const errorData = await response.json();
        return NextResponse.json(errorData, { status: response.status });
      }

      // Map common error codes to user-friendly messages
      const errorMessages: Record<number, string> = {
        400: "Invalid share link",
        404: "Share link not found",
        410: "Share link has expired or been revoked",
        429: "Too many requests. Please try again later.",
        500: "Server error. Please try again later.",
      };

      return NextResponse.json(
        { error: errorMessages[response.status] ?? "Failed to resolve share link" },
        { status: response.status }
      );
    }

    // Unexpected success response (should be a redirect)
    console.warn("[share-route] Unexpected success response from backend");
    return NextResponse.json({ error: "Unexpected response" }, { status: 500 });
  } catch (error) {
    console.error("[share-route] Failed to resolve share:", error);
    return NextResponse.json(
      { error: "Failed to resolve share link" },
      { status: 502 }
    );
  }
}
