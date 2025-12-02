import { NextResponse } from "next/server";

/**
 * Health check endpoint for Docker/Kubernetes orchestration
 * Returns 200 OK if the service is healthy
 */
export function GET() {
  return NextResponse.json(
    {
      status: "healthy",
      timestamp: new Date().toISOString(),
      service: "viralclipai-web",
    },
    { status: 200 }
  );
}
