"use client";

export function AnimatedBackground() {
  return (
    <div className="fixed inset-0 -z-10 overflow-hidden pointer-events-none">
      {/* Base gradient */}
      {/* Base gradient */}
      <div className="absolute inset-0 bg-background dark:bg-gradient-to-b dark:from-brand-dark dark:via-brand-darker dark:to-brand-dark" />

      {/* Animated gradient orbs */}
      <div
        className="gradient-orb opacity-20 dark:opacity-40"
        style={{
          width: "600px",
          height: "600px",
          background:
            "radial-gradient(circle, rgba(164, 92, 255, 0.3) 0%, transparent 70%)",
          top: "-10%",
          left: "-10%",
          animationDelay: "0s",
        }}
      />
      <div
        className="gradient-orb opacity-20 dark:opacity-40"
        style={{
          width: "500px",
          height: "500px",
          background:
            "radial-gradient(circle, rgba(92, 255, 249, 0.2) 0%, transparent 70%)",
          top: "50%",
          right: "-10%",
          animationDelay: "-7s",
        }}
      />
      <div
        className="gradient-orb opacity-20 dark:opacity-40"
        style={{
          width: "400px",
          height: "400px",
          background:
            "radial-gradient(circle, rgba(164, 92, 255, 0.25) 0%, transparent 70%)",
          bottom: "-10%",
          left: "30%",
          animationDelay: "-14s",
        }}
      />
    </div>
  );
}
