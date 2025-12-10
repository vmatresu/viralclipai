/**
 * CORS utilities for clip delivery.
 */

interface Env {
  ALLOWED_ORIGINS: string;
}

/**
 * Get CORS headers for a request.
 */
export function corsHeaders(
  request: Request,
  env: Env
): Record<string, string> {
  const origin = request.headers.get("Origin") || "";
  const allowedOrigins = env.ALLOWED_ORIGINS.split(",").map((o) => o.trim());

  // Check if origin is allowed
  const isAllowed =
    allowedOrigins.includes(origin) || allowedOrigins.includes("*");

  if (!isAllowed) {
    return {};
  }

  return {
    "Access-Control-Allow-Origin": origin,
    "Access-Control-Allow-Methods": "GET, HEAD, OPTIONS",
    "Access-Control-Allow-Headers": "Range, Origin, Accept, Content-Type",
    "Access-Control-Expose-Headers":
      "Content-Length, Content-Type, Accept-Ranges, Content-Range",
    "Access-Control-Max-Age": "86400",
  };
}

/**
 * Handle CORS preflight OPTIONS request.
 */
export function handleOptions(request: Request, env: Env): Response {
  const headers = corsHeaders(request, env);

  if (Object.keys(headers).length === 0) {
    return new Response("Forbidden", { status: 403 });
  }

  return new Response(null, {
    status: 204,
    headers,
  });
}
