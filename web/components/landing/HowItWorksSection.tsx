"use client";

import { LandingSection, SectionHeader } from "./LandingSection";

const STEPS = [
  {
    number: 1,
    title: "Paste your link",
    text: "Drop in any YouTube video, podcast, interview or tutorial.",
  },
  {
    number: 2,
    title: "AI finds the moments",
    text: "Our engine surfaces emotional spikes, hooks and high-energy clips automatically.",
  },
  {
    number: 3,
    title: "Export ready-to-post clips",
    text: "Vertical, captioned and framed — ready for Shorts, Reels and TikTok.",
  },
];

export function HowItWorksSection() {
  return (
    <LandingSection id="how-it-works">
      <SectionHeader
        title={
          <>
            Your long videos become a{" "}
            <span className="gradient-text">daily clip engine.</span>
          </>
        }
        description="Three simple steps. Everything else is automatic."
      />

      {/* Steps */}
      <div className="flex flex-col md:flex-row items-center justify-center gap-6 mb-16">
        {STEPS.map((step, index) => (
          <div key={step.number} className="contents">
            <div className="flex-1 max-w-[300px] text-center">
              <div className="w-12 h-12 mx-auto mb-4 btn-primary-gradient rounded-full flex items-center justify-center text-xl font-bold">
                {step.number}
              </div>
              <h3 className="text-xl font-semibold mb-2">{step.title}</h3>
              <p className="text-muted-foreground">{step.text}</p>
            </div>

            {/* Connector */}
            {index < STEPS.length - 1 && (
              <div className="hidden md:block w-20 opacity-50">
                <svg viewBox="0 0 100 24" fill="none">
                  <path
                    d="M0 12H90M90 12L80 6M90 12L80 18"
                    stroke="url(#connector-grad)"
                    strokeWidth="2"
                    strokeLinecap="round"
                    strokeLinejoin="round"
                  />
                  <defs>
                    <linearGradient
                      id="connector-grad"
                      x1="0"
                      y1="12"
                      x2="100"
                      y2="12"
                      gradientUnits="userSpaceOnUse"
                    >
                      <stop stopColor="#A45CFF" stopOpacity="0.2" />
                      <stop offset="0.5" stopColor="#A45CFF" />
                      <stop offset="1" stopColor="#5CFFF9" />
                    </linearGradient>
                  </defs>
                </svg>
              </div>
            )}

            {/* Mobile connector */}
            {index < STEPS.length - 1 && (
              <div className="md:hidden rotate-90 w-16 opacity-50">
                <svg viewBox="0 0 100 24" fill="none">
                  <path
                    d="M0 12H90M90 12L80 6M90 12L80 18"
                    stroke="url(#connector-grad-mobile)"
                    strokeWidth="2"
                    strokeLinecap="round"
                    strokeLinejoin="round"
                  />
                  <defs>
                    <linearGradient
                      id="connector-grad-mobile"
                      x1="0"
                      y1="12"
                      x2="100"
                      y2="12"
                      gradientUnits="userSpaceOnUse"
                    >
                      <stop stopColor="#A45CFF" stopOpacity="0.2" />
                      <stop offset="0.5" stopColor="#A45CFF" />
                      <stop offset="1" stopColor="#5CFFF9" />
                    </linearGradient>
                  </defs>
                </svg>
              </div>
            )}
          </div>
        ))}
      </div>

      {/* Flow diagram */}
      <div className="glass-card rounded-3xl p-8 max-w-2xl mx-auto">
        <div className="flex flex-col md:flex-row items-center justify-center gap-8">
          <FlowItem
            icon={
              <svg viewBox="0 0 24 24" fill="none" className="w-6 h-6">
                <rect
                  x="3"
                  y="5"
                  width="18"
                  height="14"
                  rx="2"
                  stroke="currentColor"
                  strokeWidth="1.5"
                />
                <path d="M10 9L14 12L10 15V9Z" fill="currentColor" />
              </svg>
            }
            label="Long video"
            variant="input"
          />

          <span className="text-2xl text-brand-400 rotate-90 md:rotate-0">→</span>

          <FlowItem
            icon={
              <svg viewBox="0 0 24 24" fill="none" className="w-6 h-6">
                <circle cx="12" cy="12" r="3" stroke="currentColor" strokeWidth="1.5" />
                <path
                  d="M12 1V4M12 20V23M4.22 4.22L6.34 6.34M17.66 17.66L19.78 19.78M1 12H4M20 12H23M4.22 19.78L6.34 17.66M17.66 6.34L19.78 4.22"
                  stroke="currentColor"
                  strokeWidth="1.5"
                  strokeLinecap="round"
                />
              </svg>
            }
            label="AI engine"
            variant="engine"
          />

          <span className="text-2xl text-brand-400 rotate-90 md:rotate-0">→</span>

          <FlowItem
            icon={
              <svg viewBox="0 0 24 24" fill="none" className="w-6 h-6">
                <rect
                  x="3"
                  y="3"
                  width="7"
                  height="10"
                  rx="1"
                  stroke="currentColor"
                  strokeWidth="1.5"
                />
                <rect
                  x="14"
                  y="3"
                  width="7"
                  height="10"
                  rx="1"
                  stroke="currentColor"
                  strokeWidth="1.5"
                />
                <rect
                  x="8.5"
                  y="11"
                  width="7"
                  height="10"
                  rx="1"
                  stroke="currentColor"
                  strokeWidth="1.5"
                />
              </svg>
            }
            label="Viral clips"
            variant="output"
          />
        </div>
      </div>
    </LandingSection>
  );
}

function FlowItem({
  icon,
  label,
  variant,
}: {
  icon: React.ReactNode;
  label: string;
  variant: "input" | "engine" | "output";
}) {
  const variantClasses = {
    input:
      "bg-muted/50 dark:bg-white/5 border-border dark:border-white/10 text-brand-400",
    engine: "btn-primary-gradient text-brand-dark",
    output: "bg-brand-cyan/10 border-brand-cyan/20 text-brand-cyan",
  };

  return (
    <div className="flex flex-col items-center gap-2">
      <div
        className={`w-12 h-12 rounded-xl flex items-center justify-center border ${variantClasses[variant]}`}
      >
        {icon}
      </div>
      <span className="text-sm text-muted-foreground">{label}</span>
    </div>
  );
}
