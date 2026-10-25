// Persistent client settings (everything except secrets). Saved automatically on every change.
import { invoke } from "@tauri-apps/api/core";
import { isTauri } from "./api";

export interface Settings {
  server: string;
  serverName: string;
  /** Pinned server certificate: display name + base64 of the file content. */
  certName?: string;
  certB64?: string;
  username: string;
  remember: boolean;
  advancedOpen: boolean;
}

const KEY = "xmp.settings.v2";
export const DEFAULT_SETTINGS: Settings = {
  server: "127.0.0.1:4433",
  serverName: "localhost",
  username: "",
  remember: false,
  advancedOpen: false,
};

export function loadSettings(): Settings {
  try {
    const raw = JSON.parse(localStorage.getItem(KEY) ?? "null");
    return raw ? { ...DEFAULT_SETTINGS, ...raw } : { ...DEFAULT_SETTINGS };
  } catch {
    return { ...DEFAULT_SETTINGS };
  }
}

export function saveSettings(s: Settings) {
  localStorage.setItem(KEY, JSON.stringify(s));
}

// ---------------------------------------------------------------------------------------------
// Secrets (password, session token): OS keychain on desktop. On mobile the backend reports
// "unsupported" and we use the app-private sandbox storage. If a desktop keychain exists but fails
// (e.g. no Secret Service on Linux) nothing is stored, so secrets never silently land in plaintext.
// ---------------------------------------------------------------------------------------------
const localKey = (k: string) => `xmp.secret.${k}`;
const unsupported = (e: unknown) => String(e).includes("unsupported");

export type SecretResult = "keychain" | "sandbox" | "failed";

export const secrets = {
  async set(key: string, value: string): Promise<SecretResult> {
    if (!isTauri) { localStorage.setItem(localKey(key), value); return "sandbox"; }
    try { await invoke("secret_set", { key, value }); return "keychain"; }
    catch (e) {
      if (unsupported(e)) { localStorage.setItem(localKey(key), value); return "sandbox"; }
      return "failed";
    }
  },
  async get(key: string): Promise<string | null> {
    if (!isTauri) return localStorage.getItem(localKey(key));
    try { return (await invoke<string | null>("secret_get", { key })) ?? null; }
    catch (e) { return unsupported(e) ? localStorage.getItem(localKey(key)) : null; }
  },
  async remove(key: string): Promise<void> {
    localStorage.removeItem(localKey(key));
    if (!isTauri) return;
    try { await invoke("secret_delete", { key }); } catch { /* nothing to delete / unsupported */ }
  },
};

export function bytesToBase64(bytes: Uint8Array): string {
  let s = "";
  for (let i = 0; i < bytes.length; i += 0x8000) s += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
  return btoa(s);
}
