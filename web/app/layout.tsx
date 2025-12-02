import { type ReactNode } from "react";

import type { Metadata } from "next";

import { Navbar } from "@/components/Navbar";
import { AuthProvider } from "@/lib/auth";

import "./globals.css";

export const metadata: Metadata = {
  title: "Viral Clip AI",
  description: "AI-powered viral short creation for commentary videos",
};

export default function RootLayout({ children }: { children: ReactNode }) {
  return (
    <html lang="en" className="dark">
      <body className="bg-gray-900 text-gray-100 min-h-screen font-sans antialiased">
        <AuthProvider>
          <Navbar />
          <main className="max-w-5xl mx-auto px-4 pt-24 pb-12 space-y-8">
            {children}
          </main>
        </AuthProvider>
      </body>
    </html>
  );
}
