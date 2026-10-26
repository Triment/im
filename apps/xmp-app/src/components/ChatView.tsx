import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { Download, FileText, MessageCircle, Paperclip, Phone, SendHorizontal, ShieldCheck, Video, ChevronLeft } from "lucide-react";
import { save, open } from "@tauri-apps/plugin-dialog";
import { api, demoMode, Msg } from "../api";
import { Xmp } from "../store";
import { Avatar } from "./Avatar";
import { dayKey, fmtBytes, fmtDay, fmtTime } from "../util";

function FileBubble({ m, mine, toast }: { m: Msg; mine: boolean; toast: Xmp["toast"] }) {
  const [state, setState] = useState<"idle" | "busy" | "done">("idle");
  const f = m.file!;
  const download = async () => {
    try {
      const dest = demoMode ? "/tmp/" + f.name : await save({ defaultPath: f.name });
      if (!dest) return;
      setState("busy");
      await api.downloadFile(f.hash, f.size, dest);
      setState("done");
      toast(`已保存：${f.name}`);
    } catch (e) { setState("idle"); toast(String(e), "error"); }
  };
  return (
    <div className="flex items-center gap-3">
      <div className={`flex h-11 w-11 items-center justify-center rounded-xl ${mine ? "bg-white/20" : "bg-brand/12 text-brand"}`}><FileText className="h-5 w-5" /></div>
      <div className="min-w-0 max-w-[210px]">
        <div className="truncate text-sm font-medium">{f.name}</div>
        <div className={`text-xs ${mine ? "text-white/70" : "text-dim"}`}>{fmtBytes(f.size)}</div>
      </div>
      <button onClick={download} disabled={state === "busy"} className={`flex h-9 w-9 items-center justify-center rounded-full transition active:scale-90 ${mine ? "bg-white/20 hover:bg-white/30" : "bg-brand/12 text-brand hover:bg-brand/20"}`}>
        <Download className={`h-4 w-4 ${state === "busy" ? "animate-bounce" : ""}`} />
      </button>
    </div>
  );
}

export function ChatView({ xmp }: { xmp: Xmp }) {
  const peer = xmp.friends.find((f) => f.id === xmp.selected);
  const msgs = useMemo(() => (peer ? xmp.chats[peer.id]?.msgs ?? [] : []), [peer, xmp.chats]);
  const [text, setText] = useState("");
  const endRef = useRef<HTMLDivElement>(null);
  const taRef = useRef<HTMLTextAreaElement>(null);

  useLayoutEffect(() => { endRef.current?.scrollIntoView({ block: "end" }); }, [msgs.length, peer?.id]);
  useEffect(() => { setText(""); taRef.current?.focus(); }, [peer?.id]);
  useEffect(() => { const t = taRef.current; if (t) { t.style.height = "0"; t.style.height = Math.min(140, t.scrollHeight) + "px"; } }, [text]);

  if (!peer) {
    return (
      <main className="aurora flex flex-1 flex-col items-center justify-center text-center">
        <div className="mb-4 flex h-20 w-20 items-center justify-center rounded-3xl bg-surface/70 shadow-xl"><MessageCircle className="h-9 w-9 text-brand" /></div>
        <div className="text-lg font-medium">选择一位好友开始聊天</div>
        <div className="mt-1 text-sm text-dim">文字、文件、语音与视频通话，都在这里</div>
      </main>
    );
  }

  const isOnline = !!xmp.online[peer.id];
  const send = () => { const t = text; setText(""); void xmp.sendText(t); };
  const attach = async () => {
    const p = demoMode ? "/tmp/demo.pdf" : await open({ multiple: false });
    if (typeof p === "string") void xmp.sendFile(p);
  };

  return (
    <main className="flex min-w-0 flex-1 flex-col bg-bg">
      <header className="glass z-10 flex items-center gap-2 border-b border-line px-4 py-3 sm:gap-3 sm:px-6">
        <button className="icon-btn -ml-1 sm:hidden" onClick={() => xmp.selectPeer(0)}>
          <ChevronLeft className="h-6 w-6" />
        </button>
        <Avatar name={peer.display_name} online={isOnline} size={40} />
        <div className="min-w-0 flex-1">
          <div className="truncate text-base font-semibold leading-tight">{peer.display_name}</div>
          <div className="truncate text-xs text-dim">{isOnline ? "在线" : "离线"} · @{peer.username}</div>
        </div>
        <div className="flex items-center gap-0.5 sm:gap-1">
          <button className="icon-btn" title="语音通话" disabled={!isOnline} onClick={() => void xmp.startCall(false)}><Phone className="h-5 w-5" /></button>
          <button className="icon-btn" title="视频通话" disabled={!isOnline} onClick={() => void xmp.startCall(true)}><Video className="h-5 w-5" /></button>
        </div>
      </header>

      <div className="flex-1 overflow-y-auto px-4 py-4 sm:px-6 sm:py-5">
        {msgs.length === 0 && (
          <div className="mx-auto mt-16 flex max-w-xs flex-col items-center text-center text-sm text-dim px-4">
            <ShieldCheck className="mb-3 h-8 w-8 text-brand/70" />与 {peer.display_name} 的对话从这里开始
          </div>
        )}
        {msgs.map((m, i) => {
          const mine = m.sender === xmp.me?.id;
          const prev = msgs[i - 1], next = msgs[i + 1];
          const newDay = !prev || dayKey(prev.ts) !== dayKey(m.ts);
          const first = newDay || prev.sender !== m.sender;
          const last = !next || next.sender !== m.sender || dayKey(next.ts) !== dayKey(m.ts);
          return (
            <div key={`${m.conv}-${m.seq}`}>
              {newDay && <div className="my-4 text-center text-xs font-medium text-dim">{fmtDay(m.ts)}</div>}
              <div className={`flex animate-pop ${mine ? "justify-end" : "justify-start"} ${first ? "mt-3" : "mt-0.5"}`}>
                <div
                  className={`group relative max-w-[min(520px,85%)] sm:max-w-[min(520px,72%)] px-3.5 py-2 sm:px-4 sm:py-2.5 text-[15px] leading-relaxed ${mine ? "text-white" : "bg-surface shadow-sm"} ${m.seq < 0 ? "opacity-60" : ""}`}
                  style={{
                    background: mine ? "linear-gradient(135deg,#6366f1,#8b5cf6)" : undefined,
                    borderRadius: 18,
                    borderBottomRightRadius: mine && last ? 4 : 18,
                    borderBottomLeftRadius: !mine && last ? 4 : 18,
                  }}
                >
                  {m.kind === 1 && m.file ? <FileBubble m={m} mine={mine} toast={xmp.toast} /> : <span className="whitespace-pre-wrap break-words [overflow-wrap:anywhere]">{m.text}</span>}
                  {last && <div className={`mt-1 text-right text-[10px] ${mine ? "text-white/65" : "text-dim"}`}>{fmtTime(m.ts)}</div>}
                </div>
              </div>
            </div>
          );
        })}
        <div ref={endRef} />
      </div>

      <div className="px-4 pb-4 pt-2 sm:px-6 sm:pb-5">
        <div className="glass flex items-end gap-1 rounded-[22px] border border-line p-1.5 shadow-lg shadow-black/5 transition focus-within:border-brand/60">
          <button className="icon-btn shrink-0" title="发送文件" onClick={() => void attach()}><Paperclip className="h-5 w-5" /></button>
          <textarea
            ref={taRef}
            rows={1}
            className="max-h-36 min-h-10 flex-1 resize-none bg-transparent px-2 py-2.5 text-[15px] outline-none placeholder:text-dim/70"
            placeholder="输入消息…"
            value={text}
            onChange={(e) => setText(e.target.value)}
            onKeyDown={(e) => { if (e.key === "Enter" && !e.shiftKey && !e.nativeEvent.isComposing) { e.preventDefault(); send(); } }}
          />
          <button onClick={send} disabled={!text.trim()} className="flex h-10 w-10 shrink-0 items-center justify-center rounded-full text-white transition active:scale-90 disabled:opacity-30" style={{ background: "linear-gradient(135deg,#6366f1,#8b5cf6)" }}>
            <SendHorizontal className="h-[18px] w-[18px]" />
          </button>
        </div>
      </div>
    </main>
  );
}
