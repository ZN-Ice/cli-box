/** @type {import('tailwindcss').Config} */
export default {
  content: ["./index.html", "./src/**/*.{ts,tsx}"],
  theme: {
    extend: {
      colors: {
        term: {
          bg: "#1a1b26",
          fg: "#a9b1d6",
          surface: "#24283b",
          border: "#3b4261",
          accent: "#7aa2f7",
          muted: "#565f89",
        },
      },
      fontFamily: {
        mono: [
          '"SF Mono"',
          '"Menlo"',
          '"Monaco"',
          '"Cascadia Code"',
          '"JetBrains Mono"',
          "monospace",
        ],
      },
    },
  },
  plugins: [],
};
