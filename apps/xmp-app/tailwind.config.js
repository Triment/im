/** @type {import('tailwindcss').Config} */
export default {
  content: ["./index.html", "./src/**/*.{ts,tsx}"],
  theme: {
    extend: {
      colors: {
        bg: "rgb(var(--bg) / <alpha-value>)",
        surface: "rgb(var(--surface) / <alpha-value>)",
        raised: "rgb(var(--raised) / <alpha-value>)",
        line: "rgb(var(--line) / <alpha-value>)",
        ink: "rgb(var(--ink) / <alpha-value>)",
        dim: "rgb(var(--dim) / <alpha-value>)",
        brand: "rgb(var(--brand) / <alpha-value>)",
        good: "rgb(var(--good) / <alpha-value>)",
        bad: "rgb(var(--bad) / <alpha-value>)",
      },
      fontFamily: {
        sans: ['Inter', 'SF Pro Text', '-apple-system', 'BlinkMacSystemFont', 'Segoe UI', 'Roboto', 'Noto Sans SC', 'PingFang SC', 'Microsoft YaHei', 'sans-serif'],
      },
      keyframes: {
        pop: { "0%": { opacity: 0, transform: "translateY(6px) scale(.98)" }, "100%": { opacity: 1, transform: "none" } },
        ring: { "0%": { transform: "scale(1)", opacity: 0.55 }, "100%": { transform: "scale(1.9)", opacity: 0 } },
        fade: { from: { opacity: 0 }, to: { opacity: 1 } },
      },
      animation: { pop: "pop .22s cubic-bezier(.2,.8,.2,1)", ring: "ring 1.8s ease-out infinite", fade: "fade .25s ease" },
    },
  },
  plugins: [],
};
