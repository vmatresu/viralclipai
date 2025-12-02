"use client";

import {
  ReactNode,
  createContext,
  useCallback,
  useContext,
  useEffect,
  useState,
} from "react";
import { initializeApp, getApps } from "firebase/app";
import {
  getAuth,
  onAuthStateChanged,
  signInWithPopup,
  GoogleAuthProvider,
  signOut as firebaseSignOut,
  User,
} from "firebase/auth";
import { frontendLogger } from "@/lib/logger";
import { initAnalytics, analyticsEvents } from "@/lib/analytics";

const firebaseConfig = {
  apiKey: process.env.NEXT_PUBLIC_FIREBASE_API_KEY,
  authDomain: process.env.NEXT_PUBLIC_FIREBASE_AUTH_DOMAIN,
  projectId: process.env.NEXT_PUBLIC_FIREBASE_PROJECT_ID,
  storageBucket: process.env.NEXT_PUBLIC_FIREBASE_STORAGE_BUCKET,
  messagingSenderId: process.env.NEXT_PUBLIC_FIREBASE_MESSAGING_SENDER_ID,
  appId: process.env.NEXT_PUBLIC_FIREBASE_APP_ID,
  measurementId: process.env.NEXT_PUBLIC_FIREBASE_MEASUREMENT_ID,
};

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
};

const AuthContext = createContext<AuthContextValue | undefined>(undefined);

export function AuthProvider({ children }: { children: ReactNode }) {
  const [user, setUser] = useState<User | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    // Initialize Firebase app first (via getAuthInstance), then analytics
    const auth = getAuthInstance();
    
    // Initialize analytics after Firebase app is ready
    initAnalytics();

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
    analyticsEvents.signInAttempted();
    const auth = getAuthInstance();
    const provider = new GoogleAuthProvider();
    try {
      await signInWithPopup(auth, provider);
    } catch (error: any) {
      analyticsEvents.signInFailed(error?.message || "Unknown error");
      throw error;
    }
  }, []);

  const signOut = useCallback(async () => {
    const auth = getAuthInstance();
    await firebaseSignOut(auth);
    analyticsEvents.userSignedOut();
  }, []);

  const getIdToken = useCallback(async (): Promise<string | null> => {
    const auth = getAuthInstance();
    const current = auth.currentUser;
    if (!current) return null;
    return await current.getIdToken(true);
  }, []);

  const value: AuthContextValue = {
    user,
    loading,
    signInWithGoogle,
    signOut,
    getIdToken,
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
