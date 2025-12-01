import type { Config } from "tailwindcss";

const config: Config = {
  darkMode: "class",
  content: ["./app/**/*.{js,ts,jsx,tsx}", "./components/**/*.{js,ts,jsx,tsx}"],
  theme: {
    extend: {
      colors: {
        gray: {
          900: "#121212",
          800: "#1e1e1e",
          700: "#2d2d2d",
        },
      },
    },
  },
  plugins: [],
};

export default config;
