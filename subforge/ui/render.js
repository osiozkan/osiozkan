/* SubForge canvas ASS renderer (preview quality, not libass-exact).
   Supports: styles, \an \pos \move \org \fad/\fade \clip/\iclip(rect) \fn \fs \fscx \fscy \fsp \b \i \u \s \bord \xbord \ybord
   \shad \xshad \yshad \blur(approx) \c/\1c-\4c \alpha/\1a-\4a \frz \frx/\fry(ignored) \k \kf \ko \K \t(...) animations, \r, \N \n \h,
   \p drawings (m n l b c), word wrap (WrapStyle 0/1/2). */
(function (g) {
  "use strict";
  const NUM = "-?\\d*\\.?\\d+(?:[eE][-+]?\\d+)?";
  const num = (s, d) => { const v = parseFloat(s); return isNaN(v) ? d : v; };

  function colorOf(c) { // &HAABBGGRR or &HBBGGRR& -> {r,g,b,a}
    c = String(c).trim();
    let m = /^&H([0-9a-fA-F]{1,8})&?$/i.exec(c);
    let v = m ? parseInt(m[1], 16) : (parseInt(c, 10) || 0);
    return { r: v & 255, g: (v >> 8) & 255, b: (v >> 16) & 255, a: c.replace(/&/g, "").length > 8 - 2 && m && m[1].length > 6 ? (v >>> 24) & 255 : 0 };
  }
  function bgr(c) { // tag colour "&HBBGGRR&"
    const m = /&?H?([0-9a-fA-F]{1,6})&?/i.exec(String(c));
    const v = m ? parseInt(m[1], 16) : 0;
    return { r: v & 255, g: (v >> 8) & 255, b: (v >> 16) & 255 };
  }
  function alphaOf(s) { const m = /&?H?([0-9a-fA-F]{1,2})&?/i.exec(String(s)); return m ? parseInt(m[1], 16) : 0; }
  const rgba = (c, a) => `rgba(${c.r},${c.g},${c.b},${Math.max(0, Math.min(1, 1 - a / 255))})`;
  const lerp = (a, b, t) => a + (b - a) * t;

  function baseState(st, wrapInfo) {
    return {
      fn: st.fontname, fs: st.fontsize, b: st.bold, i: st.italic, u: st.underline, s: st.strikeout,
      fscx: st.scale_x, fscy: st.scale_y, fsp: st.spacing, frz: st.angle,
      c: [colorOf(st.colors[0]), colorOf(st.colors[1]), colorOf(st.colors[2]), colorOf(st.colors[3])],
      a: [colorOf(st.colors[0]).a, colorOf(st.colors[1]).a, colorOf(st.colors[2]).a, colorOf(st.colors[3]).a],
      xbord: st.outline, ybord: st.outline, xshad: st.shadow, yshad: st.shadow, blur: 0, p: 0,
      kmode: null, kdur: 0, kstart: 0,
    };
  }

  // ---- tag tokenizer: returns [{name,arg}] where arg is raw string (may include parens contents)
  function tokenize(block) {
    const out = []; let i = 0;
    while (i < block.length) {
      if (block[i] !== "\\") { i++; continue; }
      i++;
      const m = /^(fscx|fscy|fsp|fad|fade|frx|fry|frz|fr|fax|fay|fn|fs|bord|xbord|ybord|shad|xshad|yshad|be|blur|an|a|pos|move|org|clip|iclip|alpha|[1-4]a|[1-4]c|c|kf|ko|kt|k|K|q|r|t|b|i|u|s|p)/.exec(block.slice(i));
      if (!m) continue;
      const name = m[1]; i += name.length;
      let arg = "";
      if (block[i] === "(") { // balanced paren arg
        let d = 0, j = i;
        for (; j < block.length; j++) { if (block[j] === "(") d++; else if (block[j] === ")") { d--; if (d === 0) break; } }
        arg = block.slice(i + 1, j); i = j + 1;
      } else {
        let j = i; while (j < block.length && block[j] !== "\\") j++;
        arg = block.slice(i, j); i = j;
      }
      out.push({ name, arg });
    }
    return out;
  }

  function applyTag(S, ctx, t, now, line) {
    const n = t.name, a = t.arg.trim();
    switch (n) {
      case "fn": S.fn = a; break;
      case "fs": S.fs = num(a, S.fs); break;
      case "fscx": S.fscx = num(a, S.fscx); break;
      case "fscy": S.fscy = num(a, S.fscy); break;
      case "fsp": S.fsp = num(a, S.fsp); break;
      case "b": S.b = a === "1" || num(a, 0) >= 600; break;
      case "i": S.i = a === "1"; break;
      case "u": S.u = a === "1"; break;
      case "s": S.s = a === "1"; break;
      case "bord": S.xbord = S.ybord = num(a, 0); break;
      case "xbord": S.xbord = num(a, 0); break;
      case "ybord": S.ybord = num(a, 0); break;
      case "shad": S.xshad = S.yshad = num(a, 0); break;
      case "xshad": S.xshad = num(a, 0); break;
      case "yshad": S.yshad = num(a, 0); break;
      case "be": case "blur": S.blur = num(a, 0); break;
      case "frz": case "fr": S.frz = num(a, S.frz); break;
      case "c": case "1c": S.c[0] = Object.assign(bgr(a), {}); break;
      case "2c": S.c[1] = bgr(a); break;
      case "3c": S.c[2] = bgr(a); break;
      case "4c": S.c[3] = bgr(a); break;
      case "alpha": { const v = alphaOf(a); S.a = [v, v, v, v]; break; }
      case "1a": S.a[0] = alphaOf(a); break;
      case "2a": S.a[1] = alphaOf(a); break;
      case "3a": S.a[2] = alphaOf(a); break;
      case "4a": S.a[3] = alphaOf(a); break;
      case "k": case "K": S.kmode = "k"; S.kdur = num(a, 0) * 10; break;
      case "kf": S.kmode = "kf"; S.kdur = num(a, 0) * 10; break;
      case "ko": S.kmode = "ko"; S.kdur = num(a, 0) * 10; break;
      case "p": S.p = num(a, 0); break;
      case "an": ctx.an = num(a, ctx.an); break;
      case "a": { const v = num(a, 2); const h = (v - 1) % 4, row = v < 5 ? 0 : v < 9 ? 2 : 1; const map = [1, 2, 3]; ctx.an = (row === 0 ? 0 : row === 2 ? 6 : 3) + map[Math.min(2, h)]; break; }
      case "q": ctx.wrap = num(a, ctx.wrap); break;
      case "pos": { const p = a.split(",").map(Number); if (p.length >= 2) ctx.pos = p; break; }
      case "org": { const p = a.split(",").map(Number); if (p.length >= 2) ctx.org = p; break; }
      case "move": { const p = a.split(",").map(Number); if (p.length >= 4) ctx.move = p; break; }
      case "fad": { const p = a.split(",").map(Number); if (p.length >= 2) ctx.fad = [p[0], p[1]]; break; }
      case "fade": { const p = a.split(",").map(Number); if (p.length >= 7) ctx.fade = p; break; }
      case "clip": { const p = a.split(",").map(Number); if (p.length === 4) ctx.clip = { r: p, inv: false }; break; }
      case "iclip": { const p = a.split(",").map(Number); if (p.length === 4) ctx.clip = { r: p, inv: true }; break; }
      case "t": applyT(S, ctx, a, now, line); break;
    }
  }

  function applyT(S, ctx, a, now, line) {
    // \t([t1,t2,][accel,]tags)
    const m = /^\s*(?:(-?\d+)\s*,\s*(-?\d+)\s*,\s*)?(?:(\d*\.?\d+)\s*,\s*)?(.*)$/s.exec(a);
    if (!m) return;
    let t1 = m[1] !== undefined ? +m[1] : 0, t2 = m[2] !== undefined ? +m[2] : line.dur;
    const accel = m[3] !== undefined ? +m[3] : 1, tags = m[4];
    const el = now - line.start;
    let p = t2 === t1 ? (el >= t1 ? 1 : 0) : (el - t1) / (t2 - t1);
    p = Math.max(0, Math.min(1, p)); p = Math.pow(p, accel);
    const to = tokenize(tags);
    for (const t of to) {
      const n = t.name, v = t.arg.trim();
      const mix = (cur, target) => lerp(cur, num(v, cur), p);
      switch (n) {
        case "fs": S.fs = mix(S.fs); break;
        case "fscx": S.fscx = mix(S.fscx); break;
        case "fscy": S.fscy = mix(S.fscy); break;
        case "fsp": S.fsp = mix(S.fsp); break;
        case "frz": case "fr": S.frz = mix(S.frz); break;
        case "bord": { const to2 = num(v, S.xbord); S.xbord = lerp(S.xbord, to2, p); S.ybord = lerp(S.ybord, to2, p); break; }
        case "xbord": S.xbord = mix(S.xbord); break;
        case "ybord": S.ybord = mix(S.ybord); break;
        case "shad": { const to2 = num(v, S.xshad); S.xshad = lerp(S.xshad, to2, p); S.yshad = lerp(S.yshad, to2, p); break; }
        case "blur": case "be": S.blur = mix(S.blur); break;
        case "c": case "1c": S.c[0] = mixc(S.c[0], bgr(v), p); break;
        case "2c": S.c[1] = mixc(S.c[1], bgr(v), p); break;
        case "3c": S.c[2] = mixc(S.c[2], bgr(v), p); break;
        case "4c": S.c[3] = mixc(S.c[3], bgr(v), p); break;
        case "alpha": { const to2 = alphaOf(v); S.a = S.a.map(x => lerp(x, to2, p)); break; }
        case "1a": S.a[0] = lerp(S.a[0], alphaOf(v), p); break;
        case "3a": S.a[2] = lerp(S.a[2], alphaOf(v), p); break;
        case "4a": S.a[3] = lerp(S.a[3], alphaOf(v), p); break;
        case "clip": case "iclip": { const q = v.split(",").map(Number); if (q.length === 4 && ctx.clip) ctx.clip = { r: ctx.clip.r.map((x, i) => lerp(x, q[i], p)), inv: n === "iclip" }; else if (q.length === 4) ctx.clip = { r: q, inv: n === "iclip" }; break; }
      }
    }
  }
  function mixc(a, b, p) { return { r: lerp(a.r, b.r, p), g: lerp(a.g, b.g, p), b: lerp(a.b, b.b, p) }; }

  // ---- parse event into runs
  function parseEvent(ev, styles, now) {
    const st = styles.find(s => s.name === ev.style) || styles[0] || { name: "Default", fontname: "Arial", fontsize: 48, colors: ["&H00FFFFFF", "&H000000FF", "&H00000000", "&H00000000"], bold: false, italic: false, underline: false, strikeout: false, scale_x: 100, scale_y: 100, spacing: 0, angle: 0, outline: 2, shadow: 2, align: 2, ml: 10, mr: 10, mv: 10, border_style: 1 };
    const line = { start: ev.start, end: ev.end, dur: ev.end - ev.start };
    const ctx = { an: st.align, wrap: 0, pos: null, org: null, move: null, fad: null, fade: null, clip: null, st };
    let S = baseState(st);
    const base = JSON.parse(JSON.stringify(S));
    const runs = []; // {text, S(copy), brk}
    const re = /\{([^}]*)\}|([^{]+)/g; let m; let ksum = 0; let curK = null;
    let kclock = 0;
    const snap = () => JSON.parse(JSON.stringify(S));
    while ((m = re.exec(ev.text))) {
      if (m[1] !== undefined) {
        for (const t of tokenize(m[1])) {
          if (t.name === "r") { const nst = styles.find(s => s.name === t.arg.trim()); S = nst ? baseState(nst) : JSON.parse(JSON.stringify(base)); continue; }
          if (t.name === "k" || t.name === "K" || t.name === "kf" || t.name === "ko") { applyTag(S, ctx, t, now, line); S.kstart = kclock; kclock += S.kdur; continue; }
          applyTag(S, ctx, t, now, line);
        }
      } else {
        // split into text and breaks
        const parts = m[2].split(/(\\N|\\n|\\h)/);
        for (const p of parts) {
          if (p === "\\N") runs.push({ brk: true });
          else if (p === "\\n") runs.push({ soft: true });
          else if (p === "\\h") runs.push({ text: " ", S: snap() });
          else if (p) runs.push({ text: p, S: snap() });
        }
      }
    }
    return { runs, ctx, line, st };
  }

  const probe = document.createElement("canvas").getContext("2d");
  const ratioCache = {};
  function fontSpec(S, sy) {
    return `${S.i ? "italic " : ""}${S.b ? "bold " : ""}`;
  }
  // GDI-style: ASS font size = full cell height (ascent+descent) -> canvas em size
  function emFor(S) {
    const key = S.fn + "|" + S.b + "|" + S.i;
    if (!ratioCache[key]) {
      probe.font = `${fontSpec(S)}100px "${S.fn}", sans-serif`;
      const m = probe.measureText("Hg");
      const cell = (m.fontBoundingBoxAscent + m.fontBoundingBoxDescent) || 120;
      ratioCache[key] = { cell: cell / 100, asc: (m.fontBoundingBoxAscent || 90) / 100, desc: (m.fontBoundingBoxDescent || 30) / 100 };
    }
    return ratioCache[key];
  }

  function measure(text, S, sx, sy) {
    const r = emFor(S);
    const px = S.fs / r.cell * sy;
    probe.font = `${fontSpec(S)}${px}px "${S.fn}", sans-serif`;
    let w = probe.measureText(text).width + S.fsp * sx * [...text].length;
    return { w: w * S.fscx / 100, px, r };
  }

  function drawPath(ctx2, d, scale, ox, oy) {
    const t = d.trim().split(/\s+/); let i = 0, cx = 0, cy = 0; let started = false;
    const P = () => [parseFloat(t[i++]) * scale + ox, parseFloat(t[i++]) * scale + oy];
    ctx2.beginPath();
    while (i < t.length) {
      const c = t[i++];
      if (c === "m" || c === "n") { const [x, y] = P(); ctx2.moveTo(x, y); started = true; }
      else if (c === "l") { while (i < t.length && !/[a-z]/i.test(t[i])) { const [x, y] = P(); ctx2.lineTo(x, y); } }
      else if (c === "b") { while (i + 5 < t.length + 0 && !/[a-z]/i.test(t[i])) { const a = P(), b = P(), c2 = P(); ctx2.bezierCurveTo(a[0], a[1], b[0], b[1], c2[0], c2[1]); } }
      else if (c === "c") { ctx2.closePath(); }
    }
  }
  function pathBox(d, scale) {
    const nums = d.match(/-?\d*\.?\d+/g) || []; let x0 = 1e9, y0 = 1e9, x1 = -1e9, y1 = -1e9;
    for (let i = 0; i + 1 < nums.length; i += 2) { const x = +nums[i], y = +nums[i + 1]; x0 = Math.min(x0, x); x1 = Math.max(x1, x); y0 = Math.min(y0, y); y1 = Math.max(y1, y); }
    return { x0: x0 * scale, y0: y0 * scale, x1: x1 * scale, y1: y1 * scale };
  }

  /** Draw all events active at `now` (ms) into canvas context `c` of size (cw,ch). */
  function draw(c, cw, ch, data) {
    c.clearRect(0, 0, cw, ch);
    const [rx, ry] = data.res; const sx = cw / rx, sy = ch / ry;
    const now = data.now;
    const evs = data.events.filter(e => !e.comment && now >= e.start && now < e.end).sort((a, b) => a.layer - b.layer);
    for (const ev of evs) {
      try { drawEvent(c, ev, data, sx, sy, rx, ry, now); } catch (e) { /* never break the preview */ }
    }
  }

  function drawEvent(c, ev, data, sx, sy, rx, ry, now) {
    const { runs, ctx, line, st } = parseEvent(ev, data.styles, now);
    const el = now - ev.start;
    // fade
    let alphaMul = 1;
    if (ctx.fad) { const [a, b] = ctx.fad; if (a > 0 && el < a) alphaMul = el / a; if (b > 0 && ev.end - now < b) alphaMul = Math.min(alphaMul, (ev.end - now) / b); }
    if (ctx.fade) { const [a1, a2, a3, t1, t2, t3, t4] = ctx.fade; let a; if (el < t1) a = a1; else if (el < t2) a = lerp(a1, a2, (el - t1) / (t2 - t1)); else if (el < t3) a = a2; else if (el < t4) a = lerp(a2, a3, (el - t3) / (t4 - t3)); else a = a3; alphaMul = 1 - a / 255; }
    const an = ctx.an, ha = (an - 1) % 3, va = an <= 3 ? 0 : an <= 6 ? 1 : 2; // ha 0 left 1 center 2 right; va 0 bottom 1 middle 2 top
    const ml = (ev.ml || st.ml) * sx, mr = (ev.mr || st.mr) * sx, mv = (ev.mv || st.mv) * sy;
    const wrapW = (rx * sx) - ml - mr;
    // position
    let pos = ctx.pos;
    if (ctx.move) { const [x1, y1, x2, y2, t1 = 0, t2 = line.dur] = ctx.move; const p = t2 === t1 ? 1 : Math.max(0, Math.min(1, (el - t1) / (t2 - t1))); pos = [lerp(x1, x2, p), lerp(y1, y2, p)]; }

    // build lines of segments
    const lines = [[]];
    const curLine = () => lines[lines.length - 1];
    const wrap = ctx.wrap;
    let drawing = null;
    for (const r of runs) {
      if (r.brk) { lines.push([]); continue; }
      if (r.soft) { if (wrap === 2) lines.push([]); else curLine().push({ text: " ", S: r.S || {}, sp: true }); continue; }
      if (r.S.p > 0) { drawing = drawing || []; drawing.push({ d: r.text, S: r.S }); continue; }
      // word split for wrapping
      const words = r.text.split(/(?<= )/);
      for (const w of words) curLine().push({ text: w, S: r.S });
    }
    if (drawing) { drawDrawing(c, drawing, ctx, pos, ev, st, sx, sy, an, ha, va, ml, mr, mv, rx, ry, alphaMul, now); return; }
    // measure + wrap
    const out = [];
    for (const ln of lines) {
      let cur = { segs: [], w: 0 }; out.push(cur);
      for (const sg of ln) {
        if (!sg.S || !sg.S.fn) { continue; }
        const m = measure(sg.text, sg.S, sx, sy);
        sg.w = m.w; sg.px = m.px; sg.r = m.r;
        if (wrap !== 2 && pos === null && cur.w + m.w > wrapW + 0.5 && cur.segs.length) { cur = { segs: [], w: 0 }; out.push(cur); if (sg.text.trim() === "") continue; }
        cur.segs.push(sg); cur.w += m.w;
      }
    }
    // line metrics
    let totalH = 0;
    for (const L of out) {
      let h = 0, asc = 0, desc = 0;
      for (const s of L.segs) { const hh = s.S.fs * sy * s.S.fscy / 100; h = Math.max(h, hh); asc = Math.max(asc, s.r.asc / s.r.cell * hh); desc = Math.max(desc, s.r.desc / s.r.cell * hh); }
      if (!L.segs.length) { h = st.fontsize * sy; asc = h * 0.8; desc = h * 0.2; }
      L.h = h; L.asc = asc; L.desc = desc; totalH += h;
    }
    // anchor
    let ax, ay;
    if (pos) { ax = pos[0] * sx; ay = pos[1] * sy; }
    else {
      ax = ha === 0 ? ml : ha === 1 ? ml + wrapW / 2 : ml + wrapW;
      ay = va === 0 ? ry * sy - mv : va === 1 ? ry * sy / 2 : mv;
    }
    let top = va === 0 ? ay - totalH : va === 1 ? ay - totalH / 2 : ay;
    const org = ctx.org ? [ctx.org[0] * sx, ctx.org[1] * sy] : [ax, ay];
    const frz = out.length && out[0].segs.length ? out[0].segs[0].S.frz : st.angle;

    c.save();
    if (ctx.clip) {
      const [x1, y1, x2, y2] = ctx.clip.r; c.beginPath();
      if (ctx.clip.inv) { c.rect(0, 0, rx * sx, ry * sy); c.rect(x1 * sx, y1 * sy, (x2 - x1) * sx, (y2 - y1) * sy); c.clip("evenodd"); }
      else { c.rect(x1 * sx, y1 * sy, (x2 - x1) * sx, (y2 - y1) * sy); c.clip(); }
    }
    if (frz) { c.translate(org[0], org[1]); c.rotate(-frz * Math.PI / 180); c.translate(-org[0], -org[1]); }
    let y = top;
    for (const L of out) {
      let x = ha === 0 ? (pos ? ax : ml) : ha === 1 ? ax - L.w / 2 : ax - L.w;
      if (pos && ha === 0) x = ax;
      const base = y + L.asc;
      for (const s of L.segs) {
        const S = s.S;
        const px = s.px, fx = S.fscx / 100;
        c.font = `${fontSpec(S)}${px}px "${S.fn}", sans-serif`;
        c.textBaseline = "alphabetic";
        c.lineJoin = "round";
        const bw = S.xbord * sx, bh = S.ybord * sy;
        const cols = S.c, al = S.a;
        // karaoke fill progress
        let prog = 1, secondary = false, showOutline = true;
        if (S.kmode) {
          const k0 = S.kstart, k1 = S.kstart + S.kdur;
          if (S.kmode === "kf") prog = S.kdur <= 0 ? (el >= k0 ? 1 : 0) : Math.max(0, Math.min(1, (el - k0) / S.kdur));
          else prog = el >= k0 ? 1 : 0;
          if (S.kmode === "ko") showOutline = el >= k1 ? false : true;
        }
        const drawAt = (dx, dy, fillCol, outCol, fa, oa, clipFrac) => {
          c.save();
          c.translate(x + dx, base + dy); c.scale(fx, S.fscy / 100);
          if (S.blur > 0) c.filter = `blur(${S.blur * sx * 0.6}px)`;
          if (clipFrac !== undefined) { c.beginPath(); c.rect(clipFrac.x0 / fx - 2, -px * 2, clipFrac.w / fx + 4, px * 4); c.clip(); }
          if (bw > 0 || bh > 0) { c.lineWidth = Math.max(bw, bh) * 2 / Math.max(fx, 0.01); c.strokeStyle = rgba(outCol, oa * 1); if (showOutline) c.strokeText(s.text, 0, 0); }
          if (fillCol) { c.fillStyle = rgba(fillCol, fa); c.fillText(s.text, 0, 0); }
          c.restore();
        };
        const am = (a) => 255 - (255 - a) * alphaMul;
        // shadow
        const sdx = S.xshad * sx, sdy = S.yshad * sy;
        if ((sdx || sdy) && al[3] < 255) {
          c.save(); c.translate(sdx, sdy);
          const sc = S.kmode && prog < 1 ? null : null;
          c.globalAlpha = 1;
          const shadowFill = cols[3]; // filled with back colour
          const t = c;
          t.font = c.font;
          t.save(); t.translate(x, base); t.scale(fx, S.fscy / 100);
          if (S.blur > 0) t.filter = `blur(${S.blur * sx * 0.6}px)`;
          if (bw > 0 || bh > 0) { t.lineWidth = Math.max(bw, bh) * 2 / Math.max(fx, 0.01); t.strokeStyle = rgba(shadowFill, am(al[3])); t.strokeText(s.text, 0, 0); }
          t.fillStyle = rgba(shadowFill, am(al[3])); t.fillText(s.text, 0, 0);
          t.restore(); c.restore();
        }
        if (S.kmode === "kf" && prog > 0 && prog < 1) {
          drawAt(0, 0, cols[1], cols[2], am(al[1]), am(al[2]));
          drawAt(0, 0, cols[0], cols[2], am(al[0]), am(al[2]), { x0: 0, w: s.w * prog });
        } else if (S.kmode && prog < 1) {
          drawAt(0, 0, cols[1], cols[2], am(al[1]), am(al[2]));
        } else {
          drawAt(0, 0, cols[0], cols[2], am(al[0]), am(al[2]));
        }
        // underline / strike
        if (S.u || S.s) { c.fillStyle = rgba(cols[0], am(al[0])); const th = Math.max(1, px / 14); if (S.u) c.fillRect(x, base + px * 0.1, s.w, th); if (S.s) c.fillRect(x, base - px * 0.3, s.w, th); }
        x += s.w;
      }
      y += L.h;
    }
    c.restore();
  }

  function drawDrawing(c, drawing, ctx, pos, ev, st, sx, sy, an, ha, va, ml, mr, mv, rx, ry, alphaMul, now) {
    for (const dr of drawing) {
      const S = dr.S, scale = Math.pow(2, -(S.p - 1));
      const bb = pathBox(dr.d, scale);
      const w = (bb.x1 - bb.x0) * sx, h = (bb.y1 - bb.y0) * sy;
      let ax, ay;
      if (pos) { ax = pos[0] * sx; ay = pos[1] * sy; } else { ax = ha === 0 ? ml : ha === 1 ? rx * sx / 2 : rx * sx - mr; ay = va === 0 ? ry * sy - mv : va === 1 ? ry * sy / 2 : mv; }
      const left = ha === 0 ? ax : ha === 1 ? ax - w / 2 : ax - w;
      const top = va === 0 ? ay - h : va === 1 ? ay - h / 2 : ay;
      c.save();
      if (ctx.clip) { const [x1, y1, x2, y2] = ctx.clip.r; c.beginPath(); c.rect(x1 * sx, y1 * sy, (x2 - x1) * sx, (y2 - y1) * sy); c.clip(); }
      c.translate(left - bb.x0 * sx, top - bb.y0 * sy); c.scale(sx, sy);
      drawPath(c, dr.d, scale, 0, 0);
      const am = a => 255 - (255 - a) * alphaMul;
      c.fillStyle = rgba(S.c[0], am(S.a[0])); c.fill();
      if (S.xbord > 0) { c.lineWidth = S.xbord * 2; c.lineJoin = "round"; c.strokeStyle = rgba(S.c[2], am(S.a[2])); c.stroke(); }
      c.restore();
    }
  }

  g.SFRender = { draw, parseEvent, tokenize };
})(window);
