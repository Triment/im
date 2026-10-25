import { gradientFor, initials } from "../util";

export function Avatar({ name, size = 44, online, ring }: { name: string; size?: number; online?: boolean; ring?: boolean }) {
  return (
    <div className="relative shrink-0" style={{ width: size, height: size }}>
      <div
        className={`flex h-full w-full items-center justify-center rounded-full font-semibold text-white ${ring ? "ring-4 ring-white/20" : ""}`}
        style={{ background: gradientFor(name), fontSize: size * 0.38 }}
      >
        {initials(name)}
      </div>
      {online !== undefined && (
        <span
          className={`absolute bottom-0 right-0 rounded-full border-2 border-surface ${online ? "bg-good" : "bg-dim/50"}`}
          style={{ width: size * 0.28, height: size * 0.28 }}
        />
      )}
    </div>
  );
}
