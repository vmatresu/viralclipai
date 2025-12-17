"use client";

import { LandingSection, SectionHeader } from "./LandingSection";

const BENEFITS = [
  "Post daily without opening an editor.",
  "Turn one video into a week of content.",
  "Stay visible across every platform.",
  "Grow without sacrificing your time or sanity.",
];

export function ForCreatorsSection() {
  return (
    <LandingSection id="for-creators">
      <SectionHeader
        title={
          <>
            More than clips.{" "}
            <span className="gradient-text">It makes you consistent.</span>
          </>
        }
      />

      <div className="grid lg:grid-cols-2 gap-12 items-center">
        {/* Benefits list */}
        <div>
          <h3 className="text-2xl font-semibold mb-8">
            Viral Clip AI upgrades who you are as a creator.
          </h3>
          <ul className="space-y-5">
            {BENEFITS.map((benefit) => (
              <li key={benefit} className="flex items-center gap-4 text-lg">
                <span className="flex-shrink-0">
                  <svg viewBox="0 0 24 24" fill="none" className="w-6 h-6">
                    <path
                      d="M5 12L10 17L19 7"
                      stroke="url(#check-grad)"
                      strokeWidth="2"
                      strokeLinecap="round"
                      strokeLinejoin="round"
                    />
                    <defs>
                      <linearGradient
                        id="check-grad"
                        x1="5"
                        y1="12"
                        x2="19"
                        y2="12"
                        gradientUnits="userSpaceOnUse"
                      >
                        <stop stopColor="#A45CFF" />
                        <stop offset="1" stopColor="#5CFFF9" />
                      </linearGradient>
                    </defs>
                  </svg>
                </span>
                <span>{benefit}</span>
              </li>
            ))}
          </ul>
        </div>

        {/* Momentum engine card */}
        <div className="relative">
          <div className="absolute top-0 right-0 w-full h-full bg-[radial-gradient(circle_at_top_right,rgba(164,92,255,0.25),transparent_70%)] animate-glow-pulse pointer-events-none" />

          <div className="glass-card rounded-3xl p-8 relative overflow-hidden">
            <h4 className="text-xl font-semibold text-brand-cyan mb-4">
              This is a momentum engine.
            </h4>
            <p className="text-muted-foreground leading-relaxed mb-8">
              You don&apos;t just get clips. You get an always-on system that keeps your
              content moving, even when you&apos;re not working.
            </p>

            {/* Momentum visual */}
            <div className="relative h-10">
              <div
                className="absolute left-0 top-1/2 h-0.5 bg-gradient-to-r from-brand-400 to-brand-cyan rounded-full animate-[wave_2s_ease-in-out_infinite]"
                style={{ width: "60%" }}
              />
              <div
                className="absolute left-0 h-0.5 bg-gradient-to-r from-brand-400 to-brand-cyan rounded-full opacity-60 animate-[wave_2s_ease-in-out_0.3s_infinite]"
                style={{ width: "80%", top: "40%" }}
              />
              <div
                className="absolute left-0 h-0.5 bg-gradient-to-r from-brand-400 to-brand-cyan rounded-full opacity-30 animate-[wave_2s_ease-in-out_0.6s_infinite]"
                style={{ width: "100%", top: "60%" }}
              />
            </div>
          </div>
        </div>
      </div>
    </LandingSection>
  );
}
