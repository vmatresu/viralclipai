import { frontendLogger } from "@/lib/logger";

export function getWebSocketUrl(apiBaseUrl?: string): string {
  const apiBase = apiBaseUrl ?? window.location.origin;
  let baseUrl: URL;
  try {
    baseUrl = new URL(apiBase);
    // Ensure only http/https protocols
    if (baseUrl.protocol !== "http:" && baseUrl.protocol !== "https:") {
      throw new Error("Invalid API protocol");
    }
  } catch {
    throw new Error("Invalid API base URL configuration");
  }

  // Build WebSocket URL securely
  const wsProtocol = baseUrl.protocol === "https:" ? "wss:" : "ws:";
  const wsUrl = `${wsProtocol}//${baseUrl.host}/ws/process`;

  // Validate WebSocket URL
  if (
    !wsUrl.startsWith("ws://") &&
    !wsUrl.startsWith("wss://") &&
    !wsUrl.startsWith(`${window.location.protocol === "https:" ? "wss" : "ws"}://`)
  ) {
    throw new Error("Invalid WebSocket URL");
  }

  return wsUrl;
}

export function createWebSocketConnection(
  url: string,
  onOpen: () => void,
  onMessage: (data: any) => void,
  onError: (error: Event) => void,
  onClose: () => void
): WebSocket {
  const ws = new WebSocket(url);

  ws.onopen = onOpen;

  ws.onmessage = (event) => {
    // Security: Limit message size to prevent DoS
    if (event.data.length > 1024 * 1024) {
      // 1MB limit
      frontendLogger.error("WebSocket message too large", {
        size: event.data.length,
      });
      ws.close();
      onError(new Event("Message too large"));
      return;
    }

    let data: unknown;
    try {
      data = JSON.parse(event.data);
    } catch (error) {
      frontendLogger.error("Failed to parse WebSocket message", error);
      ws.close();
      onError(new Event("Invalid JSON"));
      return;
    }

    onMessage(data);
  };

  ws.onerror = onError;
  ws.onclose = onClose;

  return ws;
}
