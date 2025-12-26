import { getApps, initializeApp, cert, type App } from "firebase-admin/app";
import { getAuth, type DecodedIdToken } from "firebase-admin/auth";
import { getFirestore, type Firestore } from "firebase-admin/firestore";

let cachedApp: App | undefined = undefined;

/**
 * Initialize Firebase Admin SDK.
 *
 * Uses GOOGLE_APPLICATION_CREDENTIALS environment variable or
 * FIREBASE_SERVICE_ACCOUNT_KEY for authentication.
 */
function getAdminApp(): App {
  if (cachedApp) return cachedApp;

  const apps = getApps();
  if (apps.length > 0) {
    const existingApp = apps[0]!;
    cachedApp = existingApp;
    return existingApp;
  }

  // Try to use service account key from environment
  const serviceAccountKey = process.env.FIREBASE_SERVICE_ACCOUNT_KEY;

  if (serviceAccountKey) {
    try {
      const serviceAccount = JSON.parse(serviceAccountKey);
      cachedApp = initializeApp({
        credential: cert(serviceAccount),
        projectId: process.env.NEXT_PUBLIC_FIREBASE_PROJECT_ID,
      });
    } catch {
      throw new Error("Failed to parse FIREBASE_SERVICE_ACCOUNT_KEY");
    }
  } else if (process.env.GOOGLE_APPLICATION_CREDENTIALS) {
    // Use default credentials from file
    cachedApp = initializeApp({
      projectId: process.env.NEXT_PUBLIC_FIREBASE_PROJECT_ID,
    });
  } else {
    throw new Error(
      "Firebase Admin SDK requires either FIREBASE_SERVICE_ACCOUNT_KEY or GOOGLE_APPLICATION_CREDENTIALS"
    );
  }

  return cachedApp;
}

/**
 * Get Firebase Auth instance.
 */
export function getAdminAuth() {
  return getAuth(getAdminApp());
}

/**
 * Get Firestore instance.
 */
export function getAdminFirestore(): Firestore {
  return getFirestore(getAdminApp());
}

/**
 * Verify a Firebase ID token and return the decoded token.
 */
export async function verifyIdToken(token: string): Promise<DecodedIdToken> {
  const auth = getAdminAuth();
  return auth.verifyIdToken(token);
}

/**
 * Extract and verify the authorization token from a request.
 */
export async function getAuthenticatedUser(
  request: Request
): Promise<DecodedIdToken | null> {
  const authHeader = request.headers.get("authorization");

  if (!authHeader?.startsWith("Bearer ")) {
    return null;
  }

  const token = authHeader.slice(7); // Remove "Bearer " prefix

  try {
    return await verifyIdToken(token);
  } catch {
    return null;
  }
}

/**
 * Require authentication - throws if not authenticated.
 */
export async function requireAuth(request: Request): Promise<DecodedIdToken> {
  const user = await getAuthenticatedUser(request);

  if (!user) {
    throw new Error("Unauthorized");
  }

  return user;
}
