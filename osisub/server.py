"""Local web server: JSON API + single-page UI. Run: python -m osisub [file.ass] [--port 8765]"""
from __future__ import annotations
import argparse, base64, json, os, threading, traceback, webbrowser
from dataclasses import asdict
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import urlparse, parse_qs

from . import formats, qc, tools, __version__
from .ass import Document, Event, Style, decode_bytes
from .lua.runtime import Script, discover, AutomationError

WEB = os.path.join(os.path.dirname(os.path.abspath(__file__)), "web")
ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


class State:
    def __init__(self):
        self.doc = Document()
        self.path = None
        self.undo, self.redo = [], []
        self.saved = self.doc.dumps()
        self.lock = threading.RLock()
        self.script_dirs = [os.path.join(ROOT, "automation"), os.path.expanduser("~/.osisub/automation")]
        self.scripts = {}   # path -> Script
        self.fps = 23.976

    def push(self):
        self.undo.append(self.doc.dumps()); self.undo = self.undo[-200:]; self.redo.clear()

    def view(self):
        d = self.doc
        return {"version": __version__, "path": self.path, "dirty": d.dumps() != self.saved,
                "can_undo": bool(self.undo), "can_redo": bool(self.redo), "info": d.info,
                "styles": [asdict(s) for s in d.styles],
                "events": [[e.comment, e.layer, e.start, e.end, e.style, e.actor, e.margin_l, e.margin_r, e.margin_t, e.effect, e.text]
                           for e in d.events], "play_res": d.play_res}

    def load_scripts(self):
        self.scripts.clear(); out = []
        for p in discover(self.script_dirs):
            try:
                s = Script(p)
                self.scripts[p] = s
                out.append({"path": p, "file": os.path.relpath(p, ROOT) if p.startswith(ROOT) else p, "name": s.name,
                            "macros": s.macros, "filters": s.filters})
            except Exception as ex:
                out.append({"path": p, "file": os.path.basename(p), "name": os.path.basename(p), "error": str(ex), "macros": [], "filters": []})
        return out


S = State()
EVF = ["comment", "layer", "start", "end", "style", "actor", "margin_l", "margin_r", "margin_t", "effect", "text"]


def api(path: str, q: dict, body: dict):
    d = S.doc
    with S.lock:
        if path == "/api/state":
            return S.view()
        if path == "/api/new":
            S.push(); S.doc = Document(); S.path = None; S.undo.clear(); S.saved = S.doc.dumps(); return S.view()
        if path == "/api/open":
            if "path" in body:
                with open(os.path.expanduser(body["path"]), "rb") as f:
                    data = f.read()
                S.path = os.path.expanduser(body["path"])
            else:
                data = base64.b64decode(body["data"]); S.path = None
            S.doc = formats.load_bytes(data); S.undo.clear(); S.redo.clear(); S.saved = S.doc.dumps()
            return S.view()
        if path == "/api/save":
            p = body.get("path") or S.path
            if not p: raise ValueError("Kayıt yolu yok")
            p = os.path.expanduser(p)
            fmt = os.path.splitext(p)[1].lower()
            txt = formats.to_srt(d) if fmt == ".srt" else formats.to_vtt(d) if fmt == ".vtt" else d.dumps()
            with open(p, "w", encoding="utf-8-sig" if fmt != ".srt" else "utf-8", newline="\n") as f:
                f.write(txt)
            if fmt not in (".srt", ".vtt"):
                S.path = p; S.saved = d.dumps()
            return S.view()
        if path == "/api/undo":
            if S.undo: S.redo.append(d.dumps()); S.doc = Document.loads(S.undo.pop())
            return S.view()
        if path == "/api/redo":
            if S.redo: S.undo.append(d.dumps()); S.doc = Document.loads(S.redo.pop())
            return S.view()
        if path == "/api/event":
            op = body["op"]; S.push()
            if op == "update":
                for i, fields in body["changes"]:
                    e = d.events[i]
                    for k, v in fields.items():
                        if k in ("start", "end", "layer", "margin_l", "margin_r", "margin_t"): v = int(v)
                        setattr(e, k, v)
            elif op == "insert":
                i = body.get("index", len(d.events))
                base = d.events[min(i, len(d.events) - 1)] if d.events else None
                st = base.end if base else 0
                d.events.insert(i + 1 if base else 0, Event(start=st, end=st + 2000, style=base.style if base else d.styles[0].name))
            elif op == "delete":
                for i in sorted(set(body["indices"]), reverse=True): del d.events[i]
            elif op == "duplicate":
                for i in sorted(set(body["indices"]), reverse=True): d.events.insert(i + 1, Event(**d.events[i].__dict__))
            elif op == "split":
                tools.split_event(d, body["index"], int(body["pos"]))
            elif op == "merge":
                tools.merge_events(d, body["indices"])
            elif op == "comment":
                for i in body["indices"]: d.events[i].comment = not d.events[i].comment
            return S.view()
        if path == "/api/tool":
            S.push(); n = body.get("name"); a = body.get("args", {}); sel = body.get("selection") or None
            res = None
            if n == "shift": tools.shift(d, int(a["ms"]), sel, a.get("start", True), a.get("end", True))
            elif n == "stretch": tools.stretch(d, float(a["factor"]), sel)
            elif n == "sort": tools.sort_events(d, a.get("key", "start"))
            elif n == "fix_overlaps": res = tools.fix_overlaps(d, sel)
            elif n == "snap_gaps": res = tools.snap_gaps(d, int(a.get("ms", 300)), sel)
            elif n == "find_replace": res = tools.find_replace(d, a["find"], a["repl"], a.get("regex", False), a.get("case", False), a.get("field", "text"), sel)
            elif n == "resample": tools.resample(d, int(a["x"]), int(a["y"]))
            else: S.undo.pop(); raise ValueError("bilinmeyen araç " + str(n))
            v = S.view(); v["result"] = res; return v
        if path == "/api/styles":
            S.push(); d.styles = [Style(**{k: v for k, v in s.items() if k in Style.__dataclass_fields__}) for s in body["styles"]]
            return S.view()
        if path == "/api/info":
            S.push(); d.set_info(body["key"], body["value"]); return S.view()
        if path == "/api/qc":
            opts = {k: float(v) if "." in v else int(v) for k, v in ((k, v[0]) for k, v in q.items())}
            return {"issues": [i.to_dict() for i in qc.check(d, **opts)]}
        if path == "/api/automation":
            return {"scripts": S.load_scripts()}
        if path == "/api/automation/run":
            sc = S.scripts.get(body["script"])
            if not sc: raise ValueError("script yok")
            sc.cb["fps"] = S.fps
            S.push()
            try:
                r = sc.run_macro(int(body["idx"]), d, body.get("selection", []), body.get("active", 0))
            except AutomationError:
                S.undo.pop(); raise
            S.doc = r.doc
            v = S.view(); v.update(log=r.log, selection=r.selection, active=r.active); return v
        if path == "/api/download":
            fmt = q.get("fmt", ["ass"])[0]
            return {"_raw": (formats.to_srt(d) if fmt == "srt" else formats.to_vtt(d) if fmt == "vtt" else d.dumps()),
                    "_name": "subtitles." + fmt}
    raise KeyError(path)


class H(BaseHTTPRequestHandler):
    def log_message(self, *a): pass

    def _send(self, code, data, ctype="application/json", extra=None):
        b = data if isinstance(data, bytes) else data.encode("utf-8")
        self.send_response(code); self.send_header("Content-Type", ctype + "; charset=utf-8" if "text" in ctype or "json" in ctype else ctype)
        self.send_header("Content-Length", str(len(b)))
        for k, v in (extra or {}).items(): self.send_header(k, v)
        self.end_headers(); self.wfile.write(b)

    def _handle(self, body):
        u = urlparse(self.path)
        if u.path in ("/", "/index.html"):
            with open(os.path.join(WEB, "index.html"), "rb") as f:
                return self._send(200, f.read(), "text/html")
        if not u.path.startswith("/api/"):
            return self._send(404, "not found", "text/plain")
        try:
            r = api(u.path, parse_qs(u.query), body)
            if "_raw" in r:
                return self._send(200, r["_raw"], "text/plain", {"Content-Disposition": f'attachment; filename="{r["_name"]}"'})
            self._send(200, json.dumps(r, ensure_ascii=False))
        except Exception as ex:
            traceback.print_exc()
            self._send(400, json.dumps({"error": str(ex)}, ensure_ascii=False))

    def do_GET(self): self._handle({})

    def do_POST(self):
        n = int(self.headers.get("Content-Length") or 0)
        self._handle(json.loads(self.rfile.read(n) or b"{}"))


def main(argv=None):
    ap = argparse.ArgumentParser(prog="osisub")
    ap.add_argument("file", nargs="?"); ap.add_argument("--port", type=int, default=8765)
    ap.add_argument("--no-browser", action="store_true"); ap.add_argument("--host", default="127.0.0.1")
    a = ap.parse_args(argv)
    if a.file:
        api("/api/open", {}, {"path": a.file})
    srv = ThreadingHTTPServer((a.host, a.port), H)
    url = f"http://{a.host}:{a.port}/"
    print(f"OsiSub {__version__} -> {url}")
    if not a.no_browser:
        threading.Timer(0.5, lambda: webbrowser.open(url)).start()
    try:
        srv.serve_forever()
    except KeyboardInterrupt:
        pass
