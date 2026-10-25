import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { api, demoMode, CallStats, ConnectArgs, Friend, Msg, XmpEvent } from "./api";
import { HD, MediaEngine, SD } from "./media";
import { loadSettings, secrets } from "./settings";

export interface Chat { convId?: number; msgs: Msg[]; unread: number }
export interface Incoming { callId: string; peer: number; video: boolean }
export interface ActiveCall {
  peer: number; video: boolean; status: "calling" | "connected";
  sas?: string; startedAt?: number; muted: boolean; camera: boolean; hd: boolean; engine: MediaEngine; stats?: CallStats;
}
export interface Toast { id: number; text: string; kind: "info" | "error" }
const errText = (e: unknown) => (typeof e === "string" ? e : e instanceof Error ? e.message : JSON.stringify(e));

export function useXmp() {
  const [me, setMe] = useState<{ id: number; username: string } | null>(null);
  const [friends, setFriends] = useState<Friend[]>([]);
  const [online, setOnline] = useState<Record<number, boolean>>({});
  const [chats, setChats] = useState<Record<number, Chat>>({});
  const [selected, setSelected] = useState<number | null>(null);
  const [incoming, setIncoming] = useState<Incoming | null>(null);
  const [call, setCall] = useState<ActiveCall | null>(null);
  const [toasts, setToasts] = useState<Toast[]>([]);
  const [busy, setBusy] = useState(false);
  const callRef = useRef<ActiveCall | null>(null);
  const meRef = useRef<typeof me>(null);
  const selRef = useRef<number | null>(null);
  const incRef = useRef<Incoming | null>(null);
  callRef.current = call; meRef.current = me; selRef.current = selected; incRef.current = incoming;

  const toast = useCallback((text: string, kind: Toast["kind"] = "info") => {
    const id = Date.now() + Math.random();
    setToasts((t) => [...t, { id, text, kind }]);
    setTimeout(() => setToasts((t) => t.filter((x) => x.id !== id)), 4200);
  }, []);

  const patchChat = useCallback((peer: number, f: (c: Chat) => Chat) => {
    setChats((all) => ({ ...all, [peer]: f(all[peer] ?? { msgs: [], unread: 0 }) }));
  }, []);

  const endCallLocal = useCallback(async (notifyBackend: boolean) => {
    const c = callRef.current;
    if (!c) return;
    c.engine.stop();
    setCall(null);
    if (notifyBackend) { try { await api.callHangup(); } catch { /* already closed */ } }
  }, []);

  const handleEvent = useCallback((e: XmpEvent) => {
    switch (e.type) {
      case "message": {
        const m = e.message;
        if (m.sender === meRef.current?.id) return;
        patchChat(m.sender, (c) => c.msgs.some((x) => x.seq === m.seq && x.conv === m.conv) ? c : {
          ...c, convId: m.conv, msgs: [...c.msgs, m], unread: selRef.current === m.sender ? 0 : c.unread + 1,
        });
        break;
      }
      case "presence": setOnline((o) => ({ ...o, [e.user]: e.online })); break;
      case "incoming_call":
        if (callRef.current) break; // busy
        setIncoming({ callId: e.call_id, peer: e.peer, video: e.video });
        break;
      case "call_ended":
        if (incRef.current?.callId === e.call_id) setIncoming(null);
        if (callRef.current && callRef.current.peer === e.peer) { toast("通话已结束"); void endCallLocal(true); }
        break;
      default: break;
    }
  }, [patchChat, endCallLocal, toast]);

  const bootstrap = useCallback(async (uid: number, username: string) => {
    setMe({ id: uid, username });
    const [fs, convs] = await Promise.all([api.friends(), api.conversations()]);
    setFriends(fs);
    const entries = await Promise.all(convs.map(async (c) => [c.peer, { convId: c.conv, msgs: await api.sync(c.conv, 0), unread: 0 } as Chat] as const));
    setChats(Object.fromEntries(entries));
    const first = fs.find((f) => f.status === "friends");
    if (first) setSelected(first.id);
  }, []);

  const unlisten = useRef<(() => void) | null>(null);
  /** `remember`: keep the password in the secure store; the session token is always kept for auto-login. */
  const login = useCallback(async (a: ConnectArgs, remember = false): Promise<boolean> => {
    setBusy(true);
    try {
      const r = await api.connect(a);
      unlisten.current?.();
      unlisten.current = await api.onEvent(handleEvent);
      const username = a.username ?? loadSettings().username ?? "me";
      await bootstrap(r.user_id, username);
      if (a.mode !== "resume") {
        const tokenRes = await secrets.set("token", r.token);
        const pwRes = remember && a.password ? await secrets.set("password", a.password) : (await secrets.remove("password"), "keychain");
        if (tokenRes === "failed" || pwRes === "failed") toast("系统钥匙串不可用：本次未保存登录凭据", "error");
      }
      return true;
    } catch (e) {
      if (a.mode === "resume") await secrets.remove("token"); // stale token: fall back to the login form
      else toast(errText(e), "error");
      return false;
    } finally { setBusy(false); }
  }, [bootstrap, handleEvent, toast]);

  const logout = useCallback(async () => {
    await endCallLocal(true);
    unlisten.current?.(); unlisten.current = null;
    try { await api.logout(); } catch { /* ignore */ }
    await secrets.remove("token");
    setMe(null); setFriends([]); setChats({}); setSelected(null); setOnline({});
  }, [endCallLocal]);

  const refreshFriends = useCallback(async () => setFriends(await api.friends()), []);

  const selectPeer = useCallback((id: number) => {
    setSelected(id);
    patchChat(id, (c) => ({ ...c, unread: 0 }));
  }, [patchChat]);

  const sendText = useCallback(async (text: string) => {
    const peer = selRef.current, my = meRef.current;
    if (!peer || !my || !text.trim()) return;
    const tmp: Msg = { conv: chats[peer]?.convId ?? 0, seq: -Date.now(), sender: my.id, kind: 0, ts: Date.now(), text };
    patchChat(peer, (c) => ({ ...c, msgs: [...c.msgs, tmp] }));
    try {
      const ack = await api.sendText(peer, text);
      patchChat(peer, (c) => ({ ...c, convId: ack.conv, msgs: c.msgs.map((m) => (m.seq === tmp.seq ? { ...m, seq: ack.seq, conv: ack.conv, ts: ack.ts } : m)) }));
    } catch (e) {
      patchChat(peer, (c) => ({ ...c, msgs: c.msgs.filter((m) => m.seq !== tmp.seq) }));
      toast(`发送失败：${errText(e)}`, "error");
    }
  }, [chats, patchChat, toast]);

  const sendFile = useCallback(async (path: string) => {
    const peer = selRef.current;
    if (!peer) return;
    toast("正在上传文件…");
    try {
      await api.sendFile(peer, path);
      const cs = await api.conversations();
      const conv = cs.find((c) => c.peer === peer);
      if (conv) {
        const msgs = await api.sync(conv.conv, 0);
        patchChat(peer, (c) => ({ ...c, convId: conv.conv, msgs }));
      }
      toast("文件已发送");
    } catch (e) { toast(`发送失败：${errText(e)}`, "error"); }
  }, [patchChat, toast]);

  const addFriend = useCallback(async (username: string) => {
    try { await api.friendRequest(username.trim()); await refreshFriends(); toast(`已向 ${username} 发送好友请求`); return true; }
    catch (e) { toast(errText(e), "error"); return false; }
  }, [refreshFriends, toast]);

  const acceptFriend = useCallback(async (id: number) => {
    try { await api.friendAccept(id); await refreshFriends(); } catch (e) { toast(errText(e), "error"); }
  }, [refreshFriends, toast]);

  const runCall = useCallback(async (peer: number, video: boolean, start: (e: MediaEngine) => Promise<{ sas: string }>, onMediaFail?: () => void) => {
    const engine = new MediaEngine();
    try {
      await engine.start(video, video ? SD : SD);
    } catch (e) {
      engine.stop();
      onMediaFail?.();
      toast(`无法访问麦克风/摄像头：${errText(e)}`, "error");
      return;
    }
    setCall({ peer, video, status: "calling", muted: false, camera: video, hd: false, engine });
    setSelected(peer);
    try {
      const info = await start(engine);
      setCall((c) => (c ? { ...c, status: "connected", sas: info.sas, startedAt: Date.now() } : c));
    } catch (e) {
      engine.stop();
      setCall(null);
      toast(`通话失败：${errText(e)}`, "error");
    }
  }, [toast]);

  const startCall = useCallback((video: boolean) => {
    const peer = selRef.current;
    if (!peer || callRef.current) return;
    return runCall(peer, video, (engine) => api.callStart(peer, video, { onAudio: engine.onRemoteAudio, onVideo: engine.onRemoteVideo }));
  }, [runCall]);

  const acceptIncoming = useCallback(() => {
    const inc = incRef.current;
    if (!inc) return;
    setIncoming(null);
    return runCall(inc.peer, inc.video, (engine) => api.callAccept(inc.callId, inc.peer, inc.video, { onAudio: engine.onRemoteAudio, onVideo: engine.onRemoteVideo }),
      () => { void api.callReject(inc.callId, inc.peer).catch(() => {}); });
  }, [runCall]);

  const rejectIncoming = useCallback(async () => {
    const inc = incRef.current;
    setIncoming(null);
    if (inc) { try { await api.callReject(inc.callId, inc.peer); } catch { /* ignore */ } }
  }, []);

  const hangup = useCallback(() => endCallLocal(true), [endCallLocal]);
  const toggleMute = useCallback(() => setCall((c) => { if (!c) return c; c.engine.setMuted(!c.muted); return { ...c, muted: !c.muted }; }), []);
  const toggleCamera = useCallback(() => setCall((c) => { if (!c || !c.video) return c; c.engine.setCamera(!c.camera); return { ...c, camera: !c.camera }; }), []);

  // poll call stats
  useEffect(() => {
    if (call?.status !== "connected") return;
    const t = setInterval(async () => { try { const s = await api.callStats(); setCall((c) => (c ? { ...c, stats: s } : c)); } catch { /* ignore */ } }, 1000);
    return () => clearInterval(t);
  }, [call?.status]);

  // auto-resume a saved session
  const resumed = useRef(false);
  useEffect(() => {
    if (resumed.current || demoMode) return;
    resumed.current = true;
    void (async () => {
      const token = await secrets.get("token");
      const st = loadSettings();
      if (token && st.username) {
        await login({ server: st.server, serverName: st.serverName, certB64: st.certB64, mode: "resume", token, username: st.username });
      }
    })();
  }, [login]);

  const friendsOnly = useMemo(() => friends.filter((f) => f.status === "friends"), [friends]);
  const requests = useMemo(() => friends.filter((f) => f.status === "received"), [friends]);
  void HD;

  return { me, friends, friendsOnly, requests, online, chats, selected, incoming, call, toasts, busy,
    login, logout, selectPeer, sendText, sendFile, addFriend, acceptFriend, startCall, acceptIncoming, rejectIncoming, hangup, toggleMute, toggleCamera, toast };
}
export type Xmp = ReturnType<typeof useXmp>;
