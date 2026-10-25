import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { readFile } from "@tauri-apps/plugin-fs";
import { Check, ChevronDown, FileKey2, Loader2, Zap, X } from "lucide-react";
import { Xmp } from "../store";
import { demoMode } from "../api";
import { bytesToBase64, loadSettings, saveSettings, secrets, Settings } from "../settings";

export function Login({ xmp }: { xmp: Xmp }) {
  const [st, setSt] = useState<Settings>(loadSettings);
  const [mode, setMode] = useState<"login" | "register">("login");
  const [password, setPassword] = useState("");
  const [picking, setPicking] = useState(false);
  const patch = (p: Partial<Settings>) => setSt((s) => ({ ...s, ...p }));

  // Everything except the password is saved automatically on every change.
  useEffect(() => { saveSettings(st); }, [st]);

  // Prefill the remembered password from the secure store.
  useEffect(() => {
    if (!st.remember) return;
    let alive = true;
    void secrets.get("password").then((p) => { if (alive && p) setPassword(p); });
    return () => { alive = false; };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const pickCert = async () => {
    setPicking(true);
    try {
      if (demoMode) { patch({ certName: "server-cert.der", certB64: "MAo=" }); return; }
      const sel = await open({ multiple: false, directory: false, filters: [{ name: "证书文件", extensions: ["der", "cer", "crt", "pem"] }] });
      if (typeof sel !== "string") return;
      const bytes = await readFile(sel);
      if (bytes.length > 64 * 1024) throw new Error("证书文件过大");
      patch({ certName: decodeURIComponent(sel.split(/[\\/]/).pop() ?? "certificate"), certB64: bytesToBase64(bytes) });
    } catch (e) {
      xmp.toast(`无法读取证书文件：${e instanceof Error ? e.message : String(e)}`, "error");
    } finally { setPicking(false); }
  };

  const submit = (e: React.FormEvent) => {
    e.preventDefault();
    void xmp.login({ server: st.server, serverName: st.serverName, certB64: st.certB64, mode, username: st.username.trim(), password }, st.remember);
  };
  const valid = st.username.trim().length >= 3 && password.length >= 8;

  return (
    <div className="aurora h-full overflow-y-auto">
     <div className="flex min-h-full items-center justify-center p-6">
      <form onSubmit={submit} className="glass w-full max-w-[400px] animate-pop rounded-[28px] border border-white/10 p-8 shadow-2xl shadow-black/20">
        <div className="mb-7 flex flex-col items-center text-center">
          <div className="mb-4 flex h-16 w-16 items-center justify-center rounded-[20px] shadow-xl shadow-brand/40" style={{ background: "linear-gradient(135deg,#6366f1,#8b5cf6 55%,#ec4899)" }}>
            <Zap className="h-8 w-8 text-white" fill="white" />
          </div>
          <h1 className="text-2xl font-semibold tracking-tight">XMP</h1>
          <p className="mt-1 text-sm text-dim">极速消息 · 超清语音 · 低延迟视频</p>
        </div>

        <div className="mb-5 grid grid-cols-2 rounded-xl bg-raised p-1 text-sm font-medium">
          {(["login", "register"] as const).map((m) => (
            <button type="button" key={m} onClick={() => setMode(m)} className={`rounded-lg py-2 transition ${mode === m ? "bg-surface shadow-sm" : "text-dim hover:text-ink"}`}>
              {m === "login" ? "登录" : "注册"}
            </button>
          ))}
        </div>

        <div className="space-y-3">
          <input className="field" placeholder="用户名（3-32 位字母数字下划线）" autoFocus autoCapitalize="none" autoCorrect="off" value={st.username} onChange={(e) => patch({ username: e.target.value })} />
          <input className="field" type="password" placeholder="密码（至少 8 位）" value={password} onChange={(e) => setPassword(e.target.value)} />
        </div>

        <label className="mt-3 flex cursor-pointer items-center gap-2.5 text-sm text-dim transition hover:text-ink">
          <input type="checkbox" className="peer sr-only" checked={st.remember} onChange={(e) => patch({ remember: e.target.checked })} />
          <span className="flex h-[18px] w-[18px] items-center justify-center rounded-md border border-line bg-raised transition peer-checked:border-transparent peer-checked:bg-brand peer-focus-visible:ring-4 peer-focus-visible:ring-brand/20 [&>svg]:scale-0 peer-checked:[&>svg]:scale-100">
            <Check className="h-3 w-3 text-white transition" strokeWidth={3.5} />
          </span>
          记住密码
          <span className="text-[11px] text-dim/70">· 仅保存在本设备</span>
        </label>

        <button type="button" onClick={() => patch({ advancedOpen: !st.advancedOpen })} className="mt-4 flex items-center gap-1 text-xs text-dim transition hover:text-ink">
          <ChevronDown className={`h-3.5 w-3.5 transition ${st.advancedOpen ? "rotate-180" : ""}`} /> 高级设置
        </button>
        {st.advancedOpen && (
          <div className="mt-3 animate-fade space-y-3">
            <input className="field" placeholder="服务器地址 host:port" value={st.server} onChange={(e) => patch({ server: e.target.value })} />
            <input className="field" placeholder="TLS 服务器名" value={st.serverName} onChange={(e) => patch({ serverName: e.target.value })} />
            {st.certName ? (
              <div className="flex items-center gap-3 rounded-xl border border-line bg-raised/60 px-3.5 py-2.5">
                <FileKey2 className="h-[18px] w-[18px] shrink-0 text-brand" />
                <div className="min-w-0 flex-1">
                  <div className="truncate text-sm">{st.certName}</div>
                  <div className="text-[11px] text-dim">已固定的服务器证书</div>
                </div>
                <button type="button" title="更换" onClick={() => void pickCert()} className="text-xs font-medium text-brand hover:underline">更换</button>
                <button type="button" title="移除" onClick={() => patch({ certName: undefined, certB64: undefined })} className="flex h-6 w-6 items-center justify-center rounded-full text-dim transition hover:bg-line hover:text-ink"><X className="h-3.5 w-3.5" /></button>
              </div>
            ) : (
              <button type="button" onClick={() => void pickCert()} disabled={picking} className="flex w-full items-center gap-3 rounded-xl border border-dashed border-line bg-raised/40 px-3.5 py-3 text-left text-sm text-dim transition hover:border-brand/60 hover:bg-raised hover:text-ink">
                {picking ? <Loader2 className="h-[18px] w-[18px] animate-spin" /> : <FileKey2 className="h-[18px] w-[18px]" />}
                <span className="min-w-0 flex-1">
                  <span className="block">选择服务器证书文件</span>
                  <span className="block text-[11px] text-dim/70">自建服务器使用 · 支持 .der / .pem</span>
                </span>
              </button>
            )}
            <p className="text-[11px] text-dim/70">设置会自动保存</p>
          </div>
        )}

        <button className="btn-brand mt-6 w-full py-3" disabled={!valid || xmp.busy}>
          {xmp.busy ? <Loader2 className="h-4 w-4 animate-spin" /> : mode === "login" ? "登录" : "创建账号"}
        </button>
        <p className="mt-4 text-center text-[11px] leading-relaxed text-dim">连接使用 QUIC 加密传输；通话媒体采用 Noise 协议端到端协商密钥。</p>
      </form>
     </div>
    </div>
  );
}
