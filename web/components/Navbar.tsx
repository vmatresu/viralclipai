"use client";

import { useAuth } from "@/lib/auth";
import Link from "next/link";

export function Navbar() {
  const { user, loading, signInWithGoogle, signOut } = useAuth();

  return (
    <nav className="glass fixed top-0 w-full z-50 border-b border-gray-700">
      <div className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8">
        <div className="flex items-center justify-between h-16">
          <div className="flex items-center gap-3">
            <div className="w-8 h-8 bg-gradient-to-br from-blue-500 to-purple-600 rounded-lg flex items-center justify-center text-white font-bold text-xl">
              ✂️
            </div>
            <Link
              href="/"
              className="text-xl font-bold bg-clip-text text-transparent bg-gradient-to-r from-blue-400 to-purple-500 hover:opacity-80 transition-opacity"
            >
              Viral Clip AI
            </Link>
          </div>
          <div className="flex items-center gap-4">
            <Link
              href="/"
              className="text-gray-300 hover:text-white transition-colors flex items-center gap-2 px-3 py-2 rounded-md hover:bg-gray-800"
            >
              <span>🏠</span>
              <span className="hidden sm:inline">Home</span>
            </Link>
            <Link
              href="/history"
              className="text-gray-300 hover:text-white transition-colors flex items-center gap-2 px-3 py-2 rounded-md hover:bg-gray-800"
            >
              <span>📜</span>
              <span className="hidden sm:inline">History</span>
            </Link>
            <Link
              href="/settings"
              className="text-gray-300 hover:text-white transition-colors flex items-center gap-2 px-3 py-2 rounded-md hover:bg-gray-800"
            >
              <span>⚙️</span>
              <span className="hidden sm:inline">Settings</span>
            </Link>
            {!loading && !user && (
              <button
                onClick={signInWithGoogle}
                className="text-gray-300 hover:text-white transition-colors flex items-center gap-2 px-3 py-2 rounded-md hover:bg-gray-800"
              >
                <span>🔐</span>
                <span className="hidden sm:inline">Sign in</span>
              </button>
            )}
            {!loading && user && (
              <div className="flex items-center gap-3">
                <span className="hidden sm:inline text-xs text-gray-400">
                  {user.email}
                </span>
                <button
                  onClick={signOut}
                  className="text-gray-300 hover:text-white transition-colors flex items-center gap-2 px-3 py-2 rounded-md hover:bg-gray-800"
                >
                  <span>🚪</span>
                  <span className="hidden sm:inline">Sign out</span>
                </button>
              </div>
            )}
          </div>
        </div>
      </div>
    </nav>
  );
}
