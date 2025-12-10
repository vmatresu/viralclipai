/**
 * Token verification for clip delivery.
 */

/**
 * Delivery token payload (matches Rust backend).
 */
export interface DeliveryToken {
  /** Clip ID */
  cid: string;
  /** User ID */
  uid: string;
  /** Scope: play | dl | thumb */
  scope: string;
  /** Expiry timestamp (Unix seconds) */
  exp: number;
  /** R2 object key - enables stateless Worker delivery */
  r2_key?: string;
  /** Is this a public share access */
  share?: boolean;
  /** Watermark flag */
  wm?: boolean;
}

// NOTE: Synchronous token verification is not supported in Cloudflare Workers.
// The Web Crypto API is async-only. Use verifyTokenAsync() instead.
// The sync verifyToken() function has been removed to prevent accidental usage.

/**
 * Sign a payload with HMAC-SHA256 (async version for Workers).
 */
export async function signPayloadAsync(
  payload: string,
  secret: string
): Promise<string> {
  const encoder = new TextEncoder();
  const keyData = encoder.encode(secret);
  const payloadData = encoder.encode(payload);

  const key = await crypto.subtle.importKey(
    "raw",
    keyData,
    { name: "HMAC", hash: "SHA-256" },
    false,
    ["sign"]
  );

  const signature = await crypto.subtle.sign("HMAC", key, payloadData);
  return base64UrlEncode(new Uint8Array(signature));
}

/**
 * Verify signature using constant-time comparison (async version for Workers).
 *
 * Uses crypto.subtle.verify() which performs constant-time comparison internally,
 * preventing timing attacks that could leak signature information.
 */
export async function verifyTokenAsync(
  signed: string,
  secret: string
): Promise<DeliveryToken | null> {
  const parts = signed.split(".");
  if (parts.length !== 2) {
    return null;
  }

  const [payloadB64, sigB64] = parts;

  try {
    const encoder = new TextEncoder();

    // Import the secret key for verification
    const key = await crypto.subtle.importKey(
      "raw",
      encoder.encode(secret),
      { name: "HMAC", hash: "SHA-256" },
      false,
      ["verify"]
    );

    // Decode the signature from base64url
    const sigBytes = base64UrlDecodeToBytes(sigB64);
    const payloadBytes = encoder.encode(payloadB64);

    // Constant-time signature verification via WebCrypto
    const isValid = await crypto.subtle.verify(
      "HMAC",
      key,
      sigBytes,
      payloadBytes
    );
    if (!isValid) {
      return null;
    }

    // Decode payload
    const json = base64UrlDecode(payloadB64);
    const token: DeliveryToken = JSON.parse(json);

    // Check expiry
    const now = Math.floor(Date.now() / 1000);
    if (token.exp <= now) {
      return null;
    }

    return token;
  } catch {
    return null;
  }
}

/**
 * Base64 URL-safe decode to string.
 */
function base64UrlDecode(str: string): string {
  // Replace URL-safe chars with standard base64
  const base64 = str.replace(/-/g, "+").replace(/_/g, "/");
  // Add padding if needed
  const padded = base64 + "=".repeat((4 - (base64.length % 4)) % 4);
  return atob(padded);
}

/**
 * Base64 URL-safe decode to Uint8Array (for signature verification).
 */
function base64UrlDecodeToBytes(str: string): Uint8Array {
  const decoded = base64UrlDecode(str);
  const bytes = new Uint8Array(decoded.length);
  for (let i = 0; i < decoded.length; i++) {
    bytes[i] = decoded.charCodeAt(i);
  }
  return bytes;
}

/**
 * Base64 URL-safe encode.
 */
function base64UrlEncode(data: Uint8Array): string {
  const base64 = btoa(String.fromCharCode(...data));
  // Convert to URL-safe base64
  return base64.replace(/\+/g, "-").replace(/\//g, "_").replace(/=/g, "");
}
