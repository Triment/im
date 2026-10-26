import { useEffect, useRef, useState } from "react";
import { Mic, MicOff, PhoneOff, ShieldCheck, Video, VideoOff, Zap, Minimize2, Maximize2 } from "lucide-react";
import { Xmp } from "../store";
import { Avatar } from "./Avatar";
import { fmtDuration } from "../util";

export function CallView({ xmp }: { xmp: Xmp }) {
  const call = xmp.call!;
  const peer = xmp.friends.find((f) => f.id === call.peer);
  const name = peer?.display_name ?? "通话";
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const selfRef = useRef<HTMLVideoElement>(null);
  const [now, setNow] = useState(Date.now());
  const [showSas, setShowSas] = useState(false);
  const [hasRemote, setHasRemote] = useState(false);

  useEffect(() => { call.engine.attachRemoteCanvas(canvasRef.current); return () => call.engine.attachRemoteCanvas(null); }, [call.engine]);
  useEffect(() => { call.engine.attachLocalPreview(selfRef.current); }, [call.engine, call.status]);
  useEffect(() => { const t = setInterval(() => setNow(Date.now()), 500); return () => clearInterval(t); }, []);
  useEffect(() => {
    if (!call.video || call.status !== "connected") return;
    call.engine.onRemoteFrame = () => setHasRemote(true);
  }, [call.video, call.status, call.engine]);

  const connected = call.status === "connected";
  const path = call.stats?.path;

  if (call.minimized) {
    return (
      <div
        className="fixed bottom-6 right-6 z-50 flex h-36 w-64 animate-pop cursor-pointer overflow-hidden rounded-2xl bg-black shadow-2xl transition-all hover:scale-[1.02] border border-white/10"
        onClick={() => xmp.setMinimized(false)}
      >
        <div className="call-aurora absolute inset-0 opacity-50" />
        <canvas ref={canvasRef} className={`absolute inset-0 h-full w-full object-cover ${call.video && connected && hasRemote ? "opacity-100" : "opacity-0"}`} />
        <div className="absolute inset-0 flex flex-col items-center justify-center bg-black/20 backdrop-blur-[2px]">
          {(!call.video || !connected || !hasRemote) && <Avatar name={name} size={48} ring />}
          <div className="mt-2 text-xs font-medium text-white shadow-sm">{name}</div>
          <div className="text-[10px] text-white/70">{connected ? fmtDuration(now - (call.startedAt ?? now)) : "正在呼叫…"}</div>
        </div>
        <button className="absolute right-2 top-2 flex h-7 w-7 items-center justify-center rounded-full bg-black/40 text-white backdrop-blur-md transition hover:bg-black/60" onClick={(e) => { e.stopPropagation(); xmp.setMinimized(false); }}>
          <Maximize2 className="h-4 w-4" />
        </button>
      </div>
    );
  }

  return (
    <div className="absolute inset-0 z-40 animate-fade overflow-hidden bg-black text-white">
      {/* backdrop: remote video or ambient gradient */}
      <div className="call-aurora absolute inset-0" />
      <canvas ref={canvasRef} className={`absolute inset-0 h-full w-full object-cover transition-opacity duration-500 ${call.video && connected && hasRemote ? "opacity-100" : "opacity-0"}`} />
      <div className="pointer-events-none absolute inset-x-0 top-0 h-40 bg-gradient-to-b from-black/60 to-transparent" />
      <div className="pointer-events-none absolute inset-x-0 bottom-0 h-56 bg-gradient-to-t from-black/70 to-transparent" />

      {/* identity */}
      {(!call.video || !connected || !hasRemote) && (
        <div className="absolute inset-0 flex flex-col items-center justify-center">
          <div className="relative mb-6">
            {!connected && <><span className="absolute inset-0 animate-ring rounded-full bg-white/25" /><span className="absolute inset-0 animate-ring rounded-full bg-white/25 [animation-delay:.9s]" /></>}
            <Avatar name={name} size={132} ring />
          </div>
          <div className="text-3xl font-semibold tracking-tight">{name}</div>
          <div className="mt-2 text-white/70">{connected ? fmtDuration(now - (call.startedAt ?? now)) : "正在呼叫…"}</div>
        </div>
      )}

      {/* top bar */}
      <div className="absolute inset-x-0 top-0 flex items-start justify-between p-5">
        <div className="flex items-center gap-2.5">
          <button onClick={() => xmp.setMinimized(true)} className="flex h-9 w-9 items-center justify-center rounded-full bg-black/35 backdrop-blur-xl transition hover:bg-black/50" title="最小化">
            <Minimize2 className="h-5 w-5" />
          </button>
          {connected && (
            <div className="flex items-center gap-2 rounded-full bg-black/35 px-3.5 py-1.5 text-xs font-medium backdrop-blur-xl">
              <Zap className={`h-3.5 w-3.5 ${path === "p2p" ? "text-good" : "text-amber-300"}`} fill="currentColor" />
              {path === "p2p" ? "P2P 直连" : path === "relay" ? "服务器中继" : "连接中"}
              {call.video && call.stats && <span className="text-white/60">· {call.stats.video_target_kbps} kbps</span>}
            </div>
          )}
          {connected && call.sas && (
            <button onClick={() => setShowSas((v) => !v)} className="flex items-center gap-1.5 rounded-full bg-black/35 px-3.5 py-1.5 text-xs font-medium backdrop-blur-xl transition hover:bg-black/50">
              <ShieldCheck className="h-3.5 w-3.5 text-good" />{showSas ? <span className="tracking-[.2em]">{call.sas}</span> : "端到端加密"}
            </button>
          )}
        </div>
        {call.video && connected && hasRemote && <div className="rounded-full bg-black/30 px-3.5 py-1.5 text-sm font-medium backdrop-blur-xl">{name} · {fmtDuration(now - (call.startedAt ?? now))}</div>}
      </div>
      {showSas && <div className="absolute inset-x-0 top-16 mx-auto w-fit animate-pop rounded-2xl bg-black/50 px-5 py-3 text-center text-xs text-white/80 backdrop-blur-xl">与对方核对验证码 <b className="ml-1 tracking-[.25em] text-white">{call.sas}</b>，一致即无中间人。</div>}

      {/* self preview */}
      {call.video && call.camera && (
        <div className="absolute bottom-28 right-5 h-[112px] w-[200px] overflow-hidden rounded-2xl border border-white/20 bg-black shadow-2xl">
          <video ref={selfRef} muted playsInline className="h-full w-full -scale-x-100 object-cover" />
        </div>
      )}

      {/* controls */}
      <div className="absolute inset-x-0 bottom-7 flex justify-center">
        <div className="flex items-center gap-4 rounded-full bg-black/35 p-2.5 backdrop-blur-2xl">
          <button onClick={xmp.toggleMute} className={`flex h-14 w-14 items-center justify-center rounded-full transition active:scale-90 ${call.muted ? "bg-white text-black" : "bg-white/15 hover:bg-white/25"}`} title={call.muted ? "取消静音" : "静音"}>
            {call.muted ? <MicOff className="h-6 w-6" /> : <Mic className="h-6 w-6" />}
          </button>
          {call.video && (
            <button onClick={xmp.toggleCamera} className={`flex h-14 w-14 items-center justify-center rounded-full transition active:scale-90 ${!call.camera ? "bg-white text-black" : "bg-white/15 hover:bg-white/25"}`} title={call.camera ? "关闭摄像头" : "打开摄像头"}>
              {call.camera ? <Video className="h-6 w-6" /> : <VideoOff className="h-6 w-6" />}
            </button>
          )}
          <button onClick={() => void xmp.hangup()} className="flex h-14 w-20 items-center justify-center rounded-full bg-bad shadow-lg shadow-bad/40 transition hover:brightness-110 active:scale-90" title="挂断">
            <PhoneOff className="h-6 w-6" />
          </button>
        </div>
      </div>
    </div>
  );
}
