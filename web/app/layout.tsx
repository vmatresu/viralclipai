import { type ReactNode } from "react";

import { Navbar } from "@/components/Navbar";
import { AuthProvider } from "@/lib/auth";
import { ThemeProvider } from "@/lib/theme-provider";

import type { Metadata } from "next";

import "./globals.css";

export const metadata: Metadata = {
  title: "Viral Clip AI",
  description: "AI-powered viral short creation for commentary videos",
  metadataBase: new URL("https://www.viralvideoai.io"),
  alternates: {
    canonical: "/",
  },
};

export default function RootLayout({ children }: { children: ReactNode }) {
  return (
    <html lang="en" suppressHydrationWarning>
      <body className="min-h-screen font-sans antialiased">
        <ThemeProvider
          attribute="class"
          defaultTheme="dark"
          enableSystem
          disableTransitionOnChange={false}
        >
          <AuthProvider>
            <Navbar />
            <main className="max-w-5xl mx-auto px-4 pt-24 pb-12 space-y-8">
              {children}
            </main>
          </AuthProvider>
        </ThemeProvider>
      </body>
    </html>
  );
}
