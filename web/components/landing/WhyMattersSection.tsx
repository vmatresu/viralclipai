"use client";

import { GlassCard } from "./GlassCard";
import { LandingSection, SectionHeader } from "./LandingSection";

const PROBLEMS = [
  {
    icon: (
      <svg viewBox="0 0 48 48" fill="none" className="w-16 h-16">
        <circle cx="24" cy="24" r="20" stroke="url(#icon-grad-1)" strokeWidth="2" />
        <path
          d="M24 14V24L30 28"
          stroke="url(#icon-grad-1)"
          strokeWidth="2"
          strokeLinecap="round"
        />
        <defs>
          <linearGradient
            id="icon-grad-1"
            x1="4"
            y1="24"
            x2="44"
            y2="24"
            gradientUnits="userSpaceOnUse"
          >
            <stop stopColor="#A45CFF" />
            <stop offset="1" stopColor="#5CFFF9" />
          </linearGradient>
        </defs>
      </svg>
    ),
    title: "Editing kills momentum",
    text: "Hours lost in timelines means less time creating.",
  },
  {
    icon: (
      <svg viewBox="0 0 48 48" fill="none" className="w-16 h-16">
        <path
          d="M8 36L16 24L24 30L32 18L40 24"
          stroke="url(#icon-grad-2)"
          strokeWidth="2"
          strokeLinecap="round"
          strokeLinejoin="round"
        />
        <path
          d="M8 12V36H40"
          stroke="#A7B0C4"
          strokeWidth="2"
          strokeLinecap="round"
          opacity="0.3"
        />
        <defs>
          <linearGradient
            id="icon-grad-2"
            x1="8"
            y1="24"
            x2="40"
            y2="24"
            gradientUnits="userSpaceOnUse"
          >
            <stop stopColor="#A45CFF" />
            <stop offset="1" stopColor="#5CFFF9" />
          </linearGradient>
        </defs>
      </svg>
    ),
    title: "Inconsistency kills reach",
    text: "If you stop posting, the algorithm forgets you.",
  },
  {
    icon: (
      <svg viewBox="0 0 48 48" fill="none" className="w-16 h-16">
        <circle cx="24" cy="20" r="8" stroke="url(#icon-grad-3)" strokeWidth="2" />
        <path
          d="M12 38C12 32.477 17.373 28 24 28C30.627 28 36 32.477 36 38"
          stroke="url(#icon-grad-3)"
          strokeWidth="2"
          strokeLinecap="round"
        />
        <defs>
          <linearGradient
            id="icon-grad-3"
            x1="12"
            y1="24"
            x2="36"
            y2="24"
            gradientUnits="userSpaceOnUse"
          >
            <stop stopColor="#A45CFF" />
            <stop offset="1" stopColor="#5CFFF9" />
          </linearGradient>
        </defs>
      </svg>
    ),
    title: "Burnout kills creators",
    text: "You shouldn't have to choose between growth and your sanity.",
  },
];

export function WhyMattersSection() {
  return (
    <LandingSection
      id="why-matters"
      className="dark:bg-gradient-to-b dark:from-transparent dark:via-brand-400/[0.03] dark:to-transparent"
    >
      <SectionHeader
        title={
          <>
            Growth doesn&apos;t come from more content.
            <br />
            <span className="gradient-text">It comes from consistency.</span>
          </>
        }
        description="Most creators don't fail because they run out of ideas. They fail because editing kills their momentum. YouTube, Shorts, Reels and TikTok reward creators who show up daily — but cutting, captioning and formatting clips eats all your time."
      />

      <div className="grid md:grid-cols-3 gap-6">
        {PROBLEMS.map((problem) => (
          <GlassCard key={problem.title} className="text-center">
            <div className="flex justify-center mb-6">{problem.icon}</div>
            <h3 className="text-xl font-semibold mb-2">{problem.title}</h3>
            <p className="text-muted-foreground">{problem.text}</p>
          </GlassCard>
        ))}
      </div>
    </LandingSection>
  );
}
