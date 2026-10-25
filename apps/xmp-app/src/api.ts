import { invoke, Channel } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export type FriendStatus = "sent" | "received" | "friends";
export interface Friend { id: number; username: string; display_name: string; status: FriendStatus }
export interface FileRef { name: string; size: number; hash: string }
export interface Msg {
  conv: number; seq: number; sender: number; kind: number; ts: number;
  text?: string; file?: FileRef | null;
}
export interface Conv { conv: number; peer: number; last_seq: number }
export interface Me { user_id: number; token: string }
export interface ConnectArgs {
  server: string; serverName: string; certB64?: string;
  mode: "login" | "register" | "resume"; username?: string; password?: string; token?: string;
}
export interface CallInfo { sas: string }
export interface CallStats { path: "p2p" | "relay" | "none"; video_target_kbps: number; tx: number; rx: number; retransmits: number; fec_recovered: number }

export type XmpEvent =
  | { type: "message"; message: Msg }
  | { type: "presence"; user: number; online: boolean }
  | { type: "incoming_call"; call_id: string; peer: number; video: boolean }
  | { type: "call_ended"; call_id: string; peer: number }
  | { type: "disconnected" }
  | { type: "other" };

export const isTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
export const demoMode = !isTauri || (typeof location !== "undefined" && location.hash.includes("demo"));

export interface MediaSinks {
  onAudio: (pcm: ArrayBuffer) => void;
  onVideo: (frame: ArrayBuffer) => void;
}

function channel(cb: (b: ArrayBuffer) => void) {
  const c = new Channel<ArrayBuffer>();
  c.onmessage = (d) => cb(d instanceof ArrayBuffer ? d : new Uint8Array(d as unknown as number[]).buffer);
  return c;
}

const real = {
  connect: (a: ConnectArgs) => invoke<Me>("connect", { args: a }),
  logout: () => invoke<void>("logout"),
  friends: () => invoke<Friend[]>("friends"),
  friendRequest: (username: string) => invoke<void>("friend_request", { username }),
  friendAccept: (userId: number) => invoke<void>("friend_accept", { userId }),
  conversations: () => invoke<Conv[]>("conversations"),
  sync: (conv: number, after: number) => invoke<Msg[]>("sync", { conv, after, limit: 200 }),
  sendText: (to: number, text: string) => invoke<{ conv: number; seq: number; ts: number }>("send_text", { to, text }),
  sendFile: (to: number, path: string) => invoke<void>("send_file", { to, path }),
  downloadFile: (hash: string, size: number, dest: string) => invoke<void>("download_file", { hash, size, dest }),
  callStart: (peer: number, video: boolean, s: MediaSinks) =>
    invoke<CallInfo>("call_start", { peer, video, audio: channel(s.onAudio), videoCh: channel(s.onVideo) }),
  callAccept: (callId: string, peer: number, video: boolean, s: MediaSinks) =>
    invoke<CallInfo>("call_accept", { callId, peer, video, audio: channel(s.onAudio), videoCh: channel(s.onVideo) }),
  callReject: (callId: string, peer: number) => invoke<void>("call_reject", { callId, peer }),
  callHangup: () => invoke<void>("call_hangup"),
  callStats: () => invoke<CallStats>("call_stats"),
  pushAudio: (pcm: Uint8Array) => invoke<void>("push_audio", pcm),
  pushVideo: (rgba: Uint8Array, w: number, h: number) => invoke<void>("push_video", rgba, { headers: { "x-w": String(w), "x-h": String(h) } }),
  onEvent: (cb: (e: XmpEvent) => void) => listen<XmpEvent>("xmp-event", (e) => cb(e.payload)),
};

// ---------------------------------------------------------------------------------------------
// Demo backend: lets the UI run in a plain browser / for design previews (`#demo`).
// ---------------------------------------------------------------------------------------------
const now = Date.now();
const demoFriends: Friend[] = [
  { id: 2, username: "mia", display_name: "Mia Chen", status: "friends" },
  { id: 3, username: "leo", display_name: "Leo Park", status: "friends" },
  { id: 4, username: "sofia", display_name: "Sofia Alvarez", status: "friends" },
  { id: 5, username: "noah", display_name: "Noah Wright", status: "received" },
];
const demoMsgs: Record<number, Msg[]> = {
  2: [
    { conv: 1, seq: 1, sender: 2, kind: 0, ts: now - 3_600_000 * 5, text: "周末的发布会彩排你来吗？" },
    { conv: 1, seq: 2, sender: 1, kind: 0, ts: now - 3_600_000 * 5 + 60_000, text: "来！我把新的演示版本也带上" },
    { conv: 1, seq: 3, sender: 2, kind: 0, ts: now - 3_600_000 * 2, text: "太好了。视频通话的延迟现在多少？" },
    { conv: 1, seq: 4, sender: 1, kind: 0, ts: now - 3_600_000 * 2 + 90_000, text: "P2P 直连下大概 40ms，丢包 10% 也很稳 ✨" },
    { conv: 1, seq: 5, sender: 2, kind: 1, ts: now - 600_000, file: { name: "launch-keynote.pdf", size: 4_820_113, hash: "a".repeat(64) } },
    { conv: 1, seq: 6, sender: 2, kind: 0, ts: now - 540_000, text: "这是最新的讲稿，看看有没有要改的 🙌" },
  ],
  3: [{ conv: 2, seq: 1, sender: 3, kind: 0, ts: now - 86_400_000, text: "Ping me when you're free" }],
};
let demoHandler: ((e: XmpEvent) => void) | null = null;
const wait = (ms: number) => new Promise((r) => setTimeout(r, ms));

const demo: typeof real = {
  connect: async (a) => { await wait(350); return { user_id: 1, token: a.username ?? "demo" }; },
  logout: async () => {},
  friends: async () => demoFriends,
  friendRequest: async () => { await wait(200); },
  friendAccept: async (id) => { const f = demoFriends.find((x) => x.id === id); if (f) f.status = "friends"; },
  conversations: async () => [{ conv: 1, peer: 2, last_seq: 6 }, { conv: 2, peer: 3, last_seq: 1 }],
  sync: async (conv) => Object.values(demoMsgs).flat().filter((m) => m.conv === conv),
  sendText: async () => { await wait(120); return { conv: 1, seq: 100 + Math.floor(Math.random() * 1000), ts: Date.now() }; },
  sendFile: async () => { await wait(600); },
  downloadFile: async () => { await wait(400); },
  callStart: async (_p, _v, s) => { await wait(1400); startFakeMedia(s); return { sas: "359 202" }; },
  callAccept: async (_c, _p, _v, s) => { await wait(500); startFakeMedia(s); return { sas: "359 202" }; },
  callReject: async () => {},
  callHangup: async () => { stopFakeMedia(); },
  callStats: async () => ({ path: "p2p", video_target_kbps: 2400 + Math.round(Math.random() * 300), tx: 1200, rx: 1900, retransmits: 3, fec_recovered: 11 }),
  pushAudio: async () => {},
  pushVideo: async () => {},
  onEvent: async (cb) => {
    demoHandler = cb;
    setTimeout(() => { cb({ type: "presence", user: 2, online: true }); cb({ type: "presence", user: 4, online: true }); }, 300);
    if (location.hash.includes("incoming")) setTimeout(() => cb({ type: "incoming_call", call_id: "1", peer: 3, video: true }), 900);
    return () => { demoHandler = null; };
  },
};
void demoHandler;

let fakeTimer: number | undefined;
function startFakeMedia(s: MediaSinks) {
  const w = 320, h = 180;
  let t = 0;
  fakeTimer = window.setInterval(() => {
    t += 1;
    const buf = new ArrayBuffer(8 + w * h * 4);
    const dv = new DataView(buf);
    dv.setUint32(0, w, true); dv.setUint32(4, h, true);
    const px = new Uint8ClampedArray(buf, 8);
    for (let y = 0; y < h; y++) for (let x = 0; x < w; x++) {
      const i = (y * w + x) * 4;
      px[i] = 60 + 90 * Math.sin((x + t * 2) / 38) ;
      px[i + 1] = 80 + 70 * Math.sin((y - t) / 30 + x / 90);
      px[i + 2] = 150 + 80 * Math.cos((x + y + t * 3) / 55);
      px[i + 3] = 255;
    }
    s.onVideo(buf);
  }, 66);
}
function stopFakeMedia() { if (fakeTimer) clearInterval(fakeTimer); fakeTimer = undefined; }

export const api = demoMode ? demo : real;
