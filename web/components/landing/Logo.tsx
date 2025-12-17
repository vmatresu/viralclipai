"use client";

import Link from "next/link";

interface LogoProps {
  className?: string;
  showText?: boolean;
}

export function Logo({ className = "", showText = true }: LogoProps) {
  return (
    <Link href="/" className={`flex items-center gap-2 ${className}`}>
      <div className="w-8 h-8">
        <svg viewBox="0 0 32 32" fill="none" xmlns="http://www.w3.org/2000/svg">
          <path
            d="M6 16C6 12 10 8 16 8C22 8 26 12 28 16"
            stroke="url(#momentum-gradient)"
            strokeWidth="3"
            strokeLinecap="round"
          />
          <path
            d="M8 20C8 17 11 14 16 14C21 14 24 17 26 20"
            stroke="url(#momentum-gradient)"
            strokeWidth="2.5"
            strokeLinecap="round"
            opacity="0.7"
          />
          <path
            d="M10 24C10 22 12.5 20 16 20C19.5 20 22 22 24 24"
            stroke="url(#momentum-gradient)"
            strokeWidth="2"
            strokeLinecap="round"
            opacity="0.4"
          />
          <defs>
            <linearGradient
              id="momentum-gradient"
              x1="6"
              y1="16"
              x2="28"
              y2="16"
              gradientUnits="userSpaceOnUse"
            >
              <stop stopColor="#A45CFF" />
              <stop offset="1" stopColor="#5CFFF9" />
            </linearGradient>
          </defs>
        </svg>
      </div>
      {showText && (
        <span className="text-lg font-semibold text-foreground">Viral Clip AI</span>
      )}
    </Link>
  );
}
