"use client";

import { getApps, initializeApp } from "firebase/app";
import {
  GoogleAuthProvider,
  signOut as firebaseSignOut,
  getAuth,
  isSignInWithEmailLink,
  onAuthStateChanged,
  sendSignInLinkToEmail,
  signInWithEmailLink,
  signInWithPopup,
  type User,
} from "firebase/auth";
import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useState,
  type ReactNode,
} from "react";

import { analyticsEvents, initAnalytics } from "@/lib/analytics";
import { clearAllClipsCache } from "@/lib/cache";
import { frontendLogger } from "@/lib/logger";
import { isValidFirebaseConfig } from "@/lib/security/validation";

const firebaseConfig = {
  apiKey: process.env.NEXT_PUBLIC_FIREBASE_API_KEY,
  authDomain: process.env.NEXT_PUBLIC_FIREBASE_AUTH_DOMAIN,
  projectId: process.env.NEXT_PUBLIC_FIREBASE_PROJECT_ID,
  storageBucket: process.env.NEXT_PUBLIC_FIREBASE_STORAGE_BUCKET,
  messagingSenderId: process.env.NEXT_PUBLIC_FIREBASE_MESSAGING_SENDER_ID,
  appId: process.env.NEXT_PUBLIC_FIREBASE_APP_ID,
  measurementId: process.env.NEXT_PUBLIC_FIREBASE_MEASUREMENT_ID,
};

// Validate Firebase configuration at module load time
if (
  typeof window !== "undefined" &&
  !isValidFirebaseConfig({
    apiKey: firebaseConfig.apiKey,
    authDomain: firebaseConfig.authDomain,
    projectId: firebaseConfig.projectId,
  })
) {
  frontendLogger.warn(
    "Firebase configuration is incomplete or invalid. Auth features may not work correctly."
  );
}

let authInstance: ReturnType<typeof getAuth> | null = null;

function getAuthInstance() {
  if (!authInstance) {
    if (!getApps().length) {
      if (!firebaseConfig.apiKey) {
        // Incomplete config; app will run but auth will be disabled.
        frontendLogger.warn(
          "Firebase config is missing. Auth will not work correctly."
        );
      } else {
        initializeApp(firebaseConfig);
      }
    }
    authInstance = getAuth();
  }
  return authInstance;
}

type AuthContextValue = {
  user: User | null;
  loading: boolean;
  signInWithGoogle: () => Promise<void>;
  signOut: () => Promise<void>;
  getIdToken: () => Promise<string | null>;
  sendEmailLink: (email: string) => Promise<void>;
  finishEmailSignIn: (email: string, link: string) => Promise<void>;
  isEmailLink: (link: string) => boolean;
};

const AuthContext = createContext<AuthContextValue | undefined>(undefined);

export function AuthProvider({ children }: { children: ReactNode }) {
  const [user, setUser] = useState<User | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    // Initialize Firebase app first (via getAuthInstance), then analytics
    const auth = getAuthInstance();

    // Initialize analytics after Firebase app is ready
    void initAnalytics();

    const unsub = onAuthStateChanged(auth, (u) => {
      setUser(u);
      setLoading(false);
      // Track sign in event with user ID
      if (u) {
        analyticsEvents.userSignedIn(u.uid);
      }
    });
    return () => unsub();
  }, []);

  const signInWithGoogle = useCallback(async () => {
    void analyticsEvents.signInAttempted();
    const auth = getAuthInstance();
    const provider = new GoogleAuthProvider();
    try {
      frontendLogger.info("Attempting Google sign-in...");
      await signInWithPopup(auth, provider);
      frontendLogger.info("Google sign-in successful");
    } catch (error: unknown) {
      frontendLogger.error("Google sign-in failed", error);

      const message = error instanceof Error ? error.message : "Unknown error";
      void analyticsEvents.signInFailed(message);
      throw error;
    }
  }, []);

  const sendEmailLink = useCallback(async (email: string) => {
    const auth = getAuthInstance();
    const actionCodeSettings = {
      // URL you want to redirect back to. The domain (www.example.com) for this
      // URL must be in the authorized domains list in the Firebase Console.
      url: `${window.location.origin}/auth/finish`,
      handleCodeInApp: true,
    };

    await sendSignInLinkToEmail(auth, email, actionCodeSettings);
    window.localStorage.setItem("emailForSignIn", email);
    void analyticsEvents.signInAttempted(); // Consider adding a specific event for email link
  }, []);

  const finishEmailSignIn = useCallback(async (email: string, link: string) => {
    const auth = getAuthInstance();
    try {
      const result = await signInWithEmailLink(auth, email, link);
      window.localStorage.removeItem("emailForSignIn");
      if (result.user) {
        analyticsEvents.userSignedIn(result.user.uid);
      }
    } catch (error) {
      const message = error instanceof Error ? error.message : "Unknown error";
      void analyticsEvents.signInFailed(message);
      throw error;
    }
  }, []);

  const isEmailLink = useCallback((link: string) => {
    const auth = getAuthInstance();
    return isSignInWithEmailLink(auth, link);
  }, []);

  const signOut = useCallback(async () => {
    const auth = getAuthInstance();
    await firebaseSignOut(auth);
    // SECURITY: Clear all cached clips data when user signs out
    // This prevents signed-out users from accessing cached video data
    await clearAllClipsCache();
    void analyticsEvents.userSignedOut();
  }, []);

  const getIdToken = useCallback((): Promise<string | null> => {
    const auth = getAuthInstance();
    const current = auth.currentUser;
    if (!current) return Promise.resolve(null);
    return current.getIdToken(true);
  }, []);

  const value: AuthContextValue = {
    user,
    loading,
    signInWithGoogle,
    signOut,
    getIdToken,
    sendEmailLink,
    finishEmailSignIn,
    isEmailLink,
  };

  return <AuthContext.Provider value={value}>{children}</AuthContext.Provider>;
}

export function useAuth(): AuthContextValue {
  const ctx = useContext(AuthContext);
  if (!ctx) {
    throw new Error("useAuth must be used within an AuthProvider");
  }
  return ctx;
}
