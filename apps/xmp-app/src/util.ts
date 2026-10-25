export const initials = (name: string) => {
  const t = name.trim();
  if (!t) return "?";
  const parts = t.split(/\s+/);
  const s = parts.length > 1 ? parts[0][0] + parts[1][0] : Array.from(t).slice(0, 2).join("");
  return s.toUpperCase();
};
const PALETTE = [
  ["#6366f1", "#a855f7"], ["#0ea5e9", "#6366f1"], ["#10b981", "#0ea5e9"], ["#f59e0b", "#ef4444"],
  ["#ec4899", "#8b5cf6"], ["#14b8a6", "#22c55e"], ["#f97316", "#ec4899"], ["#3b82f6", "#06b6d4"],
];
export const gradientFor = (seed: string) => {
  let h = 0;
  for (const c of seed) h = (h * 31 + c.charCodeAt(0)) >>> 0;
  const [a, b] = PALETTE[h % PALETTE.length];
  return `linear-gradient(135deg, ${a}, ${b})`;
};
export const fmtBytes = (n: number) => (n < 1024 ? `${n} B` : n < 1048576 ? `${(n / 1024).toFixed(1)} KB` : n < 1073741824 ? `${(n / 1048576).toFixed(1)} MB` : `${(n / 1073741824).toFixed(2)} GB`);
export const fmtTime = (ts: number) => new Date(ts).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", hour12: false });
export const dayKey = (ts: number) => new Date(ts).toDateString();
export const fmtDay = (ts: number) => {
  const d = new Date(ts), today = new Date();
  const diff = Math.round((new Date(today.toDateString()).getTime() - new Date(d.toDateString()).getTime()) / 86_400_000);
  if (diff === 0) return "今天";
  if (diff === 1) return "昨天";
  return d.toLocaleDateString("zh-CN", { month: "long", day: "numeric", weekday: "short" });
};
export const fmtShort = (ts: number) => {
  const diff = Math.round((new Date(new Date().toDateString()).getTime() - new Date(new Date(ts).toDateString()).getTime()) / 86_400_000);
  if (diff === 0) return fmtTime(ts);
  if (diff === 1) return "昨天";
  return new Date(ts).toLocaleDateString("zh-CN", { month: "numeric", day: "numeric" });
};
export const fmtDuration = (ms: number) => {
  const s = Math.max(0, Math.floor(ms / 1000));
  return `${String(Math.floor(s / 60)).padStart(2, "0")}:${String(s % 60).padStart(2, "0")}`;
};
