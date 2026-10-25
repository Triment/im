// Browser-side capture and playback. Everything crossing to Rust is raw: 48 kHz stereo i16 PCM in
// 20 ms frames (audio) and RGBA frames (video). Opus / H.264 / FEC / networking live in Rust.
import { api, demoMode } from "./api";

const CAPTURE_WORKLET = `
class Capture extends AudioWorkletProcessor {
  constructor() { super(); this.buf = new Int16Array(1920); this.n = 0; }
  process(inputs) {
    const inp = inputs[0]; if (!inp || !inp[0]) return true;
    const l = inp[0], r = inp[1] || inp[0];
    for (let i = 0; i < l.length; i++) {
      this.buf[this.n++] = Math.max(-1, Math.min(1, l[i])) * 32767;
      this.buf[this.n++] = Math.max(-1, Math.min(1, r[i])) * 32767;
      if (this.n === 1920) { const out = this.buf; this.port.postMessage(out.buffer, [out.buffer]); this.buf = new Int16Array(1920); this.n = 0; }
    }
    return true;
  }
}
registerProcessor("xmp-capture", Capture);
`;

const PLAYBACK_WORKLET = `
class Playback extends AudioWorkletProcessor {
  constructor() {
    super();
    this.cap = 48000 * 2 * 0.4; this.ring = new Float32Array(this.cap);
    this.r = 0; this.w = 0; this.fill = 0; this.started = false; this.muted = false;
    this.port.onmessage = (e) => {
      const pcm = new Int16Array(e.data);
      // keep latency low: if we are >120 ms behind, drop the oldest audio
      if (this.fill > 48000 * 2 * 0.12) { const drop = this.fill - 48000 * 2 * 0.04; this.r = (this.r + drop) % this.cap; this.fill -= drop; }
      for (let i = 0; i < pcm.length; i++) { this.ring[this.w] = pcm[i] / 32768; this.w = (this.w + 1) % this.cap; }
      this.fill = Math.min(this.cap, this.fill + pcm.length);
    };
  }
  process(_i, outputs) {
    const o = outputs[0], L = o[0], R = o[1] || o[0];
    if (!this.started && this.fill >= 48000 * 2 * 0.03) this.started = true;
    for (let i = 0; i < L.length; i++) {
      if (this.started && this.fill >= 2) {
        L[i] = this.ring[this.r]; this.r = (this.r + 1) % this.cap;
        R[i] = this.ring[this.r]; this.r = (this.r + 1) % this.cap; this.fill -= 2;
      } else { L[i] = 0; R[i] = 0; if (this.fill < 2) this.started = false; }
    }
    return true;
  }
}
registerProcessor("xmp-playback", Playback);
`;

const blobUrl = (src: string) => URL.createObjectURL(new Blob([src], { type: "application/javascript" }));

export interface VideoQuality { w: number; h: number; fps: number }
export const HD: VideoQuality = { w: 1280, h: 720, fps: 30 };
export const SD: VideoQuality = { w: 640, h: 360, fps: 30 };

export class MediaEngine {
  stream: MediaStream | null = null;
  private ctx: AudioContext | null = null;
  private playback: AudioWorkletNode | null = null;
  private capture: AudioWorkletNode | null = null;
  private videoEl: HTMLVideoElement | null = null;
  private stopped = false;
  private remote: HTMLCanvasElement | null = null;
  private lastRemoteFrame: { w: number; h: number; data: Uint8ClampedArray } | null = null;
  muted = false;

  async start(wantVideo: boolean, q: VideoQuality = SD) {
    if (demoMode) { this.stopped = false; return; } // design preview: no devices needed
    this.stream = await navigator.mediaDevices.getUserMedia({
      audio: { echoCancellation: true, noiseSuppression: true, autoGainControl: true, channelCount: { ideal: 2 }, sampleRate: { ideal: 48000 } },
      video: wantVideo ? { width: { ideal: q.w }, height: { ideal: q.h }, frameRate: { ideal: q.fps } } : false,
    });
    const ctx = new AudioContext({ sampleRate: 48000, latencyHint: "interactive" });
    this.ctx = ctx;
    await ctx.audioWorklet.addModule(blobUrl(CAPTURE_WORKLET));
    await ctx.audioWorklet.addModule(blobUrl(PLAYBACK_WORKLET));
    const src = ctx.createMediaStreamSource(this.stream);
    this.capture = new AudioWorkletNode(ctx, "xmp-capture", { channelCount: 2, channelCountMode: "explicit" });
    this.capture.port.onmessage = (e) => { if (!this.muted && !this.stopped) void api.pushAudio(new Uint8Array(e.data as ArrayBuffer)); };
    src.connect(this.capture);
    this.playback = new AudioWorkletNode(ctx, "xmp-playback", { outputChannelCount: [2] });
    this.playback.connect(ctx.destination);
    if (ctx.state === "suspended") await ctx.resume();
    if (wantVideo) this.startVideoLoop(q);
  }

  private startVideoLoop(q: VideoQuality) {
    const v = document.createElement("video");
    v.muted = true; v.playsInline = true; v.srcObject = this.stream;
    void v.play();
    this.videoEl = v;
    const cv = document.createElement("canvas");
    cv.width = q.w; cv.height = q.h;
    const g = cv.getContext("2d", { willReadFrequently: true })!;
    let busy = false, last = 0;
    const tick = (t: number) => {
      if (this.stopped) return;
      if (!busy && t - last >= 1000 / q.fps - 2 && v.readyState >= 2 && this.stream?.getVideoTracks()[0]?.enabled) {
        last = t; busy = true;
        g.drawImage(v, 0, 0, q.w, q.h);
        const img = g.getImageData(0, 0, q.w, q.h);
        api.pushVideo(new Uint8Array(img.data.buffer), q.w, q.h).finally(() => { busy = false; });
      }
      requestAnimationFrame(tick);
    };
    requestAnimationFrame(tick);
  }

  attachLocalPreview(el: HTMLVideoElement | null) {
    if (el && this.stream && el.srcObject !== this.stream) { el.srcObject = this.stream; void el.play(); }
  }

  attachRemoteCanvas(c: HTMLCanvasElement | null) {
    this.remote = c;
    if (c && this.lastRemoteFrame) this.paint(this.lastRemoteFrame);
  }

  onRemoteAudio = (pcm: ArrayBuffer) => { this.playback?.port.postMessage(pcm, [pcm]); };

  onRemoteVideo = (buf: ArrayBuffer) => {
    const dv = new DataView(buf);
    const w = dv.getUint32(0, true), h = dv.getUint32(4, true);
    const frame = { w, h, data: new Uint8ClampedArray(buf, 8, w * h * 4) };
    this.lastRemoteFrame = frame;
    this.paint(frame);
  };

  private paint(f: { w: number; h: number; data: Uint8ClampedArray }) {
    const c = this.remote;
    if (!c) return;
    if (c.width !== f.w || c.height !== f.h) { c.width = f.w; c.height = f.h; }
    c.getContext("2d")!.putImageData(new ImageData(f.data as unknown as Uint8ClampedArray<ArrayBuffer>, f.w, f.h), 0, 0);
  }

  setMuted(m: boolean) { this.muted = m; this.stream?.getAudioTracks().forEach((t) => (t.enabled = !m)); }
  setCamera(on: boolean) { this.stream?.getVideoTracks().forEach((t) => (t.enabled = on)); }
  get hasVideo() { return !!this.stream?.getVideoTracks().length; }

  stop() {
    this.stopped = true;
    this.stream?.getTracks().forEach((t) => t.stop());
    void this.ctx?.close();
    this.videoEl?.remove();
    this.stream = null; this.ctx = null; this.remote = null;
  }
}
