import { useMemo, useState } from "react";
import { Check, LogOut, Search, UserPlus, X } from "lucide-react";
import { Xmp } from "../store";
import { Avatar } from "./Avatar";
import { fmtShort } from "../util";

export function Sidebar({ xmp }: { xmp: Xmp }) {
  const [q, setQ] = useState("");
  const [adding, setAdding] = useState(false);
  const [name, setName] = useState("");

  const rows = useMemo(() => {
    const list = xmp.friendsOnly.map((f) => {
      const chat = xmp.chats[f.id];
      const last = chat?.msgs[chat.msgs.length - 1];
      return { f, last, unread: chat?.unread ?? 0 };
    });
    list.sort((a, b) => (b.last?.ts ?? 0) - (a.last?.ts ?? 0));
    const t = q.trim().toLowerCase();
    return t ? list.filter((r) => r.f.display_name.toLowerCase().includes(t) || r.f.username.includes(t)) : list;
  }, [xmp.friendsOnly, xmp.chats, q]);

  const preview = (m?: { kind: number; text?: string }) => (!m ? "开始聊天" : m.kind === 1 ? "📎 文件" : m.kind === 2 ? "🎤 语音" : m.text ?? "");

  return (
    <aside className="flex h-full w-full flex-col border-r border-line bg-surface">
      <div className="flex items-center gap-3 px-5 pb-3 pt-5">
        <Avatar name={xmp.me?.username ?? "?"} size={38} />
        <div className="min-w-0 flex-1">
          <div className="truncate text-[15px] font-semibold">{xmp.me?.username}</div>
          <div className="flex items-center gap-1.5 text-xs text-dim"><span className="h-1.5 w-1.5 rounded-full bg-good" />已连接</div>
        </div>
        <button className="icon-btn" title="添加好友" onClick={() => setAdding((v) => !v)}><UserPlus className="h-[18px] w-[18px]" /></button>
        <button className="icon-btn" title="退出登录" onClick={() => void xmp.logout()}><LogOut className="h-[18px] w-[18px]" /></button>
      </div>

      {adding && (
        <form
          className="mx-4 mb-3 flex animate-pop gap-2"
          onSubmit={async (e) => { e.preventDefault(); if (name.trim() && (await xmp.addFriend(name))) { setName(""); setAdding(false); } }}
        >
          <input autoFocus className="field !py-2" placeholder="输入对方用户名" value={name} onChange={(e) => setName(e.target.value)} />
          <button className="btn-brand !px-3.5" disabled={!name.trim()}>发送</button>
        </form>
      )}

      <div className="px-4 pb-2">
        <div className="relative">
          <Search className="pointer-events-none absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-dim" />
          <input className="field !bg-raised !pl-9" placeholder="搜索" value={q} onChange={(e) => setQ(e.target.value)} />
        </div>
      </div>

      {xmp.requests.length > 0 && (
        <div className="mx-4 mb-2 rounded-2xl bg-brand/10 p-3">
          <div className="mb-2 text-xs font-medium text-brand">好友请求 · {xmp.requests.length}</div>
          {xmp.requests.map((r) => (
            <div key={r.id} className="flex items-center gap-3 py-1">
              <Avatar name={r.display_name} size={32} />
              <span className="min-w-0 flex-1 truncate text-sm">{r.display_name}</span>
              <button className="flex h-8 w-8 items-center justify-center rounded-full bg-good/15 text-good transition hover:bg-good/25" onClick={() => void xmp.acceptFriend(r.id)}><Check className="h-4 w-4" /></button>
              <button className="flex h-8 w-8 items-center justify-center rounded-full bg-raised text-dim transition hover:text-ink"><X className="h-4 w-4" /></button>
            </div>
          ))}
        </div>
      )}

      <nav className="flex-1 overflow-y-auto px-2 pb-3">
        {rows.length === 0 && <div className="px-6 py-16 text-center text-sm text-dim">还没有好友<br />点击右上角添加</div>}
        {rows.map(({ f, last, unread }) => {
          const active = xmp.selected === f.id;
          return (
            <button key={f.id} onClick={() => xmp.selectPeer(f.id)} className={`mb-0.5 flex w-full items-center gap-3 rounded-2xl px-3 py-2.5 text-left transition ${active ? "bg-brand/12" : "hover:bg-raised"}`}>
              <Avatar name={f.display_name} online={!!xmp.online[f.id]} />
              <div className="min-w-0 flex-1">
                <div className="flex items-baseline justify-between gap-2">
                  <span className="truncate text-[15px] font-medium">{f.display_name}</span>
                  {last && <span className="shrink-0 text-[11px] text-dim">{fmtShort(last.ts)}</span>}
                </div>
                <div className="flex items-center justify-between gap-2">
                  <span className="truncate text-[13px] text-dim">{preview(last)}</span>
                  {unread > 0 && <span className="flex h-5 min-w-5 items-center justify-center rounded-full bg-brand px-1.5 text-[11px] font-semibold text-white">{unread}</span>}
                </div>
              </div>
            </button>
          );
        })}
      </nav>
    </aside>
  );
}
