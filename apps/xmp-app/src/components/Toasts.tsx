import { AlertCircle, CheckCircle2 } from "lucide-react";
import { Toast } from "../store";

export function Toasts({ toasts }: { toasts: Toast[] }) {
  return (
    <div className="pointer-events-none fixed inset-x-0 top-4 z-[60] flex flex-col items-center gap-2">
      {toasts.map((t) => (
        <div key={t.id} className="glass flex max-w-[520px] animate-pop items-center gap-2.5 rounded-2xl border border-line px-4 py-2.5 text-sm shadow-xl">
          {t.kind === "error" ? <AlertCircle className="h-4 w-4 shrink-0 text-bad" /> : <CheckCircle2 className="h-4 w-4 shrink-0 text-good" />}
          <span className="break-words">{t.text}</span>
        </div>
      ))}
    </div>
  );
}
