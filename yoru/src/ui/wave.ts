import { Store } from "./store";

/** Basit dalga formu: videonun sesini çözer, pencere içinde tepe değerleri çizer; satır kenarları sürüklenebilir. */
export class Waveform {
  private peaks: Float32Array | null = null;
  private pps = 200; // saniyedeki tepe örneği
  zoom = 1; // 1 = 10 sn pencere
  center = 0;
  private drag: { idx: number; edge: "start" | "end" } | null = null;
  constructor(private cv: HTMLCanvasElement, private store: Store, private seek: (ms: number) => void) {
    cv.addEventListener("mousedown", (e) => this.down(e));
    window.addEventListener("mousemove", (e) => this.move(e));
    window.addEventListener("mouseup", () => { this.drag = null; });
    cv.addEventListener("wheel", (e) => { e.preventDefault(); this.zoom = Math.min(20, Math.max(0.1, this.zoom * (e.deltaY < 0 ? 1.2 : 1 / 1.2))); this.draw(); }, { passive: false });
  }
  async load(blobUrl: string) {
    try {
      const buf = await (await fetch(blobUrl)).arrayBuffer();
      const ctx = new AudioContext();
      const ab = await ctx.decodeAudioData(buf);
      const ch = ab.getChannelData(0);
      const step = Math.floor(ab.sampleRate / this.pps);
      const n = Math.floor(ch.length / step);
      const pk = new Float32Array(n);
      for (let i = 0; i < n; i++) { let m = 0; for (let j = 0; j < step; j += 4) { const v = Math.abs(ch[i * step + j]); if (v > m) m = v; } pk[i] = m; }
      this.peaks = pk; ctx.close();
    } catch { this.peaks = null; }
    this.draw();
  }
  private winMs() { return 10000 / this.zoom; }
  private t2x(ms: number) { return ((ms - (this.center - this.winMs() / 2)) / this.winMs()) * this.cv.width; }
  private x2t(x: number) { return this.center - this.winMs() / 2 + (x / this.cv.width) * this.winMs(); }
  private mx(e: MouseEvent) { const r = this.cv.getBoundingClientRect(); return ((e.clientX - r.left) / r.width) * this.cv.width; }
  private down(e: MouseEvent) {
    const x = this.mx(e);
    for (const i of this.store.selected.length ? this.store.selected : [this.store.active]) {
      const ev = this.store.file.events[i]; if (!ev) continue;
      if (Math.abs(this.t2x(ev.start) - x) < 6) { this.drag = { idx: i, edge: "start" }; this.store.checkpoint(); return; }
      if (Math.abs(this.t2x(ev.end) - x) < 6) { this.drag = { idx: i, edge: "end" }; this.store.checkpoint(); return; }
    }
    this.seek(Math.max(0, Math.round(this.x2t(x))));
  }
  private move(e: MouseEvent) {
    if (!this.drag) return;
    const ev = this.store.file.events[this.drag.idx]; if (!ev) return;
    const t = Math.max(0, Math.round(this.x2t(this.mx(e)) / 10) * 10);
    if (this.drag.edge === "start") ev.start = Math.min(t, ev.end); else ev.end = Math.max(t, ev.start);
    this.store.dirty = true; this.store.emit();
  }
  setTime(ms: number) { this.center = ms; this.draw(); }
  draw() {
    const cv = this.cv; const c = cv.getContext("2d")!;
    cv.width = cv.clientWidth * devicePixelRatio || 800; cv.height = cv.clientHeight * devicePixelRatio || 120;
    const css = getComputedStyle(document.documentElement);
    c.fillStyle = css.getPropertyValue("--wave-bg") || "#111"; c.fillRect(0, 0, cv.width, cv.height);
    const t0 = this.center - this.winMs() / 2, t1 = this.center + this.winMs() / 2;
    // altyazı çubukları
    for (const [i, e] of this.store.file.events.entries()) {
      if (e.end < t0 || e.start > t1) continue;
      const a = this.t2x(e.start), b = this.t2x(e.end);
      c.fillStyle = this.store.sel.has(i) ? "rgba(255,160,60,.28)" : "rgba(90,140,255,.18)";
      c.fillRect(a, 0, b - a, cv.height);
      c.fillStyle = this.store.sel.has(i) ? "#ffa03c" : "#5a8cff";
      c.fillRect(a, 0, 2, cv.height); c.fillRect(b - 2, 0, 2, cv.height);
    }
    if (this.peaks) {
      c.fillStyle = "#7fe7a6"; const mid = cv.height / 2;
      for (let x = 0; x < cv.width; x++) {
        const k = Math.floor(((t0 + (x / cv.width) * this.winMs()) / 1000) * this.pps);
        const v = this.peaks[k] ?? 0; const h = v * mid * 0.95;
        c.fillRect(x, mid - h, 1, h * 2 || 1);
      }
    } else { c.fillStyle = "#667"; c.fillText("Ses yok — video yükleyin", 10, 20); }
    c.fillStyle = "#f55"; c.fillRect(cv.width / 2, 0, 1, cv.height);
  }
}
