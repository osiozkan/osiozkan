import { AssFile, Event, newFile } from "../core/ass";

type Listener = () => void;

/** Uygulama durumu + geri al/yinele (anlık görüntü tabanlı). */
export class Store {
  file: AssFile = newFile();
  name = "adsiz.ass";
  dirty = false;
  sel = new Set<number>();
  active = -1;
  fps = 23.976;
  private undoStack: string[] = [];
  private redoStack: string[] = [];
  private listeners = new Set<Listener>();
  private lastPush = 0;

  on(l: Listener) { this.listeners.add(l); return () => this.listeners.delete(l); }
  emit() { this.listeners.forEach((l) => l()); }

  private snap() { return JSON.stringify({ i: this.file.info, s: this.file.styles, e: this.file.events }); }
  private restore(s: string) { const o = JSON.parse(s); this.file.info = o.i; this.file.styles = o.s; this.file.events = o.e; }

  /** Değişiklikten ÖNCE çağır. `merge` ile ardışık küçük düzenlemeler tek adıma birleşir. */
  checkpoint(merge = false) {
    const now = Date.now();
    if (merge && now - this.lastPush < 800 && this.undoStack.length) { this.lastPush = now; return; }
    this.lastPush = now;
    this.undoStack.push(this.snap());
    if (this.undoStack.length > 200) this.undoStack.shift();
    this.redoStack = [];
    this.dirty = true;
  }
  undo() { const s = this.undoStack.pop(); if (!s) return; this.redoStack.push(this.snap()); this.restore(s); this.fixSel(); this.emit(); }
  redo() { const s = this.redoStack.pop(); if (!s) return; this.undoStack.push(this.snap()); this.restore(s); this.fixSel(); this.emit(); }
  private fixSel() {
    const n = this.file.events.length;
    this.sel = new Set([...this.sel].filter((i) => i < n));
    if (this.active >= n) this.active = n - 1;
  }

  load(f: AssFile, name: string) {
    this.file = f; this.name = name; this.dirty = false; this.sel = new Set(f.events.length ? [0] : []);
    this.active = f.events.length ? 0 : -1; this.undoStack = []; this.redoStack = []; this.emit();
  }
  select(i: number, mode: "one" | "toggle" | "range" = "one") {
    if (mode === "one") { this.sel = new Set([i]); }
    else if (mode === "toggle") { this.sel.has(i) ? this.sel.delete(i) : this.sel.add(i); }
    else { const a = Math.min(this.active < 0 ? i : this.active, i), b = Math.max(this.active < 0 ? i : this.active, i); this.sel = new Set(); for (let k = a; k <= b; k++) this.sel.add(k); }
    if (mode !== "range") this.active = i;
    this.emit();
  }
  get activeEvent(): Event | undefined { return this.file.events[this.active]; }
  get selected(): number[] { return [...this.sel].sort((a, b) => a - b); }
  /** Dışarıdan (Lua, işlemler) yeni dosya içeriği uygula. */
  replaceFile(f: AssFile, sel?: number[]) {
    this.checkpoint();
    this.file = f;
    if (sel) { this.sel = new Set(sel.filter((i) => i < f.events.length)); this.active = sel[0] ?? this.active; }
    this.fixSel(); this.emit();
  }
}
