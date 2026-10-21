// UI smoke test: node tests/ui_smoke.cjs  (expects `subforge serve tests/fixtures/sample.ass` on :8765)
const { chromium } = require(process.env.PW_PATH || "playwright");
const process_cwd = process.cwd();
const fs = require("fs");
(async () => {
  const b = await chromium.launch({ executablePath: "/opt/pw-browsers/chromium" });
  const ctx = await b.newContext({ viewport: { width: 1400, height: 850 }, permissions: ["clipboard-read", "clipboard-write"] });
  await ctx.grantPermissions(["clipboard-read", "clipboard-write"], { origin: "http://127.0.0.1:" + (process.env.SF_PORT || 8765) });
  const p = await ctx.newPage();
  const errs = [];
  p.on("pageerror", e => errs.push("pageerror: " + e.message));
  p.on("console", m => { if (m.type() === "error") errs.push("console: " + m.text()); });
  const ok = (c, m) => { if (!c) { errs.push("FAIL: " + m); } else console.log("ok  " + m); };
  await p.goto("http://127.0.0.1:" + (process.env.SF_PORT || 8765) + "/");
  await p.waitForSelector(".row");
  ok((await p.locator(".row").count()) === 3, "grid renders 3 rows");
  await p.locator('.row[data-i="1"]').click();
  ok((await p.inputValue("#eText")) === "Hello  world", "edit box shows selected text");
  await p.fill("#eText", "Hello there");
  await p.locator("#eText").blur();
  await p.waitForFunction(() => document.querySelector('.row[data-i="1"] .txt').textContent === "Hello there");
  ok(true, "text edit commits");
  ok(!(await p.locator("#bUndo").isDisabled()), "undo enabled");
  await p.click("#bUndo");
  await p.waitForFunction(() => document.querySelector('.row[data-i="1"] .txt').textContent === "Hello  world");
  ok(true, "undo restores text");
  // time edit
  await p.locator('.row[data-i="0"]').click();
  await p.fill('#eFields [data-k="start"]', "0:00:02.00");
  await p.locator('#eFields [data-k="start"]').dispatchEvent("change");
  await p.waitForFunction(() => document.querySelectorAll(".row")[0].children[3].textContent === "0:00:02.00");
  ok(true, "start time edit");
  // tools: shift all by 1000
  await p.click("#bTools");
  await p.fill("#tShift", "1000");
  await p.click('[data-t="shift"]');
  await p.waitForFunction(() => document.querySelectorAll(".row")[0].children[3].textContent === "0:00:03.00");
  ok(true, "shift tool");
  await p.click('[data-t="stats"]');
  await p.waitForFunction(() => document.querySelector("#tMsg").textContent.includes("Words:"));
  ok(true, "stats report");
  await p.click('[data-t="fonts"]');
  await p.waitForFunction(() => document.querySelector("#tMsg").textContent.includes("Arial"));
  ok(true, "fonts report");
  await p.fill("#rsX", "1920"); await p.fill("#rsY", "1080"); await p.click('[data-t="resample"]');
  await p.waitForFunction(() => S.res[0] === 1920 && S.res[1] === 1080);
  ok(true, "resample tool");
  await p.keyboard.press("Escape");
  // add / dup / delete
  await p.click("#bAdd");
  await p.waitForFunction(() => document.querySelectorAll(".row").length === 4);
  ok(true, "add line");
  await p.click("#bDel");
  await p.waitForFunction(() => document.querySelectorAll(".row").length === 3);
  ok(true, "delete line");
  // QA
  await p.click("#bQa");
  await p.waitForSelector(".issue");
  ok((await p.locator(".issue").count()) >= 1, "QA finds issues");
  // Lua macro
  await p.click('#tabs [data-p="mac"]');
  await p.setInputFiles("#fLua", "tests/fixtures/test_macro.lua");
  await p.waitForSelector(".macro");
  ok((await p.locator(".macro").count()) >= 5, "macros listed (uploaded + built-in)");
  await p.locator(".macro", { hasText: "Collapse spaces" }).locator("button").click();
  await p.waitForFunction(() => [...document.querySelectorAll(".row .txt")].some(e => e.textContent === "Hello world"));
  ok(true, "macro edited text through Rust/Lua");
  ok((await p.textContent("#macLog")).includes("fixed"), "macro log shown");
  // waveform
  await p.setInputFiles("#fVideo", "tests/fixtures/tone.wav");
  await p.waitForFunction(() => W.peaks && W.peaks.length > 100, null, { timeout: 10000 });
  ok(true, "waveform decoded");
  await p.waitForTimeout(100);
  const painted = await p.evaluate(() => { const c = document.querySelector("#wave"); const d = c.getContext("2d").getImageData(0, 0, c.width, c.height).data; let n = 0; for (let i = 0; i < d.length; i += 4) if (d[i + 1] > 150 && d[i] < 120) n++; return n; });
  ok(painted > 500, "waveform pixels painted (" + painted + ")");
  // drag end handle of event 0 (ends 3.00s) right by ~0.5s
  await p.evaluate(() => { W.t0 = 0; W.span = 10000; drawWave(); });
  await p.locator('.row[data-i="0"]').click();
  const box = await p.locator("#wave").boundingBox();
  const before = await p.evaluate(() => S.events[0].end);
  const xEnd = box.x + before / 10000 * box.width, y = box.y + box.height / 2;
  await p.mouse.move(xEnd, y); await p.mouse.down(); await p.mouse.move(xEnd + box.width * 0.05, y, { steps: 5 }); await p.mouse.up();
  await p.waitForFunction((b) => S.events[0].end > b + 300, before);
  ok(true, "dragging waveform handle retimes line: " + before + " -> " + (await p.evaluate(() => S.events[0].end)));
  // server-side media: ffmpeg peaks + range streaming
  await p.evaluate(async (cwd) => { await loadMediaPath(cwd + "/tests/fixtures/tone.wav"); }, process_cwd);
  await p.waitForFunction(() => W.peaks && W.peaks.length > 2000 && /ffmpeg/.test(document.querySelector("#waveInfo").textContent), null, { timeout: 15000 });
  ok(true, "ffmpeg waveform via /api/peaks (" + (await p.evaluate(() => W.peaks.length / 2)) + " bins)");
  const rng = await p.evaluate(async (cwd) => { const r = await fetch("/media?path=" + encodeURIComponent(cwd + "/tests/fixtures/tone.wav") + "&token=" + TOKEN, { headers: { Range: "bytes=0-99" } }); return [r.status, (await r.arrayBuffer()).byteLength, r.headers.get("content-range")]; }, process_cwd);
  ok(rng[0] === 206 && rng[1] === 100, "range streaming: " + rng.join(" "));
  const noTok = await p.evaluate(async (cwd) => (await fetch("/media?path=" + encodeURIComponent(cwd + "/tests/fixtures/tone.wav"))).status, process_cwd);
  ok(noTok === 403, "media without token is rejected");
  // region selection in the waveform -> Enter applies it to the current line
  await p.evaluate(() => { W.t0 = 0; W.span = 10000; drawWave(); selectOnly(1); });
  const wb = await p.locator("#wave").boundingBox();
  const regionY = wb.y + wb.height / 2, xa = wb.x + wb.width * 0.88, xb = wb.x + wb.width * 0.97;
  await p.mouse.move(xa, regionY); await p.mouse.down(); await p.mouse.move(xb, regionY, { steps: 5 }); await p.mouse.up();
  const hasRegion = await p.evaluate(() => !!W.region && Math.abs(W.region.b - W.region.a) > 500);
  ok(hasRegion, "dragging empty waveform space selects a region");
  await p.evaluate(() => document.activeElement && document.activeElement.blur());
  await p.keyboard.press("Enter");
  await p.waitForFunction(() => S.events[1].start >= 8500 && S.events[1].end >= 9000, null, { timeout: 5000 });
  ok(true, "Enter applies the region to the selected line");
  // renderer: typeset fixture
  const ts = require("fs").readFileSync("tests/fixtures/typeset.ass");
  await p.evaluate(async (t) => { await sfetch("/api/open_raw?name=typeset.ass", { method: "POST", body: t }).then(r => r.json()).then(st => setState(st)); }, ts.toString());
  await p.waitForTimeout(300);
  await p.evaluate(() => { vid.style.display = "none"; selectOnly(0); updateOverlay(); });
  await p.waitForTimeout(200);
  const lit = await p.evaluate(() => { const c = document.querySelector("#ovc"); const d = c.getContext("2d").getImageData(0, 0, c.width, c.height).data; let w = 0, y = 0, o = 0; for (let i = 0; i < d.length; i += 4) { if (d[i] > 230 && d[i + 1] > 230 && d[i + 2] > 230) w++; if (d[i] > 200 && d[i + 1] > 200 && d[i + 2] < 80) y++; if (d[i] > 200 && d[i + 1] < 90 && d[i + 2] < 90) o++; } return { w, y, o }; });
  ok(lit.w > 300, "renderer draws white text (" + lit.w + ")");
  ok(lit.o > 50, "renderer draws red outline / transform colour (" + lit.o + ")");
  await p.locator("#video").screenshot({ path: "/tmp/claude-0/render.png" });
  // server-side browse/open/save-in-place
  await p.evaluate(() => { vid.style.display = "none"; });
  await p.click("#bBrowse");
  await p.waitForSelector("#fPath");
  await p.fill("#fPath", process.cwd() + "/tests/fixtures"); await p.press("#fPath", "Enter");
  await p.waitForSelector('#mbox .issue[data-n="sample.ass"]');
  await p.click('#mbox .issue[data-n="sample.ass"]');
  await p.waitForFunction(() => S.path && S.path.endsWith("sample.ass"));
  ok(true, "open from disk via server browser");
  // Lua script from disk (real path => includes/requires relative to script work)
  await p.click('#tabs [data-p="mac"]');
  await p.click("#bLuaDisk"); await p.waitForSelector("#fPath");
  await p.fill("#fPath", process.cwd() + "/tests/fixtures"); await p.press("#fPath", "Enter");
  await p.waitForSelector('#mbox .issue[data-n="test_macro.lua"]'); await p.click('#mbox .issue[data-n="test_macro.lua"]');
  await p.waitForFunction(() => document.querySelectorAll(".macro").length >= 5);
  ok(true, "Lua script added from disk");
  await p.waitForFunction(async () => (await api("/api/settings")).settings.scripts.some(x => x.endsWith("test_macro.lua")), null, { timeout: 10000 });
  ok(true, "script path persisted in settings.json");
  // save as to tmp then save in place
  await p.click("#bSaveAs"); await p.waitForSelector("#fName");
  await p.fill("#fPath", "/tmp/claude-0"); await p.evaluate(() => { document.querySelector("#fPath").dataset.stale = "1"; }); await p.press("#fPath", "Enter");
  await p.waitForFunction(() => document.querySelector("#fPath") && !document.querySelector("#fPath").dataset.stale && document.querySelector("#fName"));
  await p.fill("#fName", "ui_saved.ass"); await p.click("#fOk");
  await p.waitForFunction(() => S.path === "/tmp/claude-0/ui_saved.ass");
  ok(require("fs").readFileSync("/tmp/claude-0/ui_saved.ass", "utf8").includes("[Events]"), "save as wrote the file");
  // reference subtitle + bilingual merge
  await p.evaluate(async (cwd) => { const st = await api("/api/open_path", { path: cwd + "/tests/fixtures/sample.ass" }); setState(st); selectOnly(0); }, process_cwd);
  await p.setInputFiles("#fRef", "tests/fixtures/ref.srt");
  await p.waitForFunction(() => REF.length === 2);
  await p.evaluate(() => { const i = S.events.findIndex(e => e.start >= 1000); selectOnly(i); });
  await p.waitForFunction(() => document.querySelector("#refBox").textContent.includes("source"));
  ok(true, "reference text shown for selected line");
  const merged = await p.evaluate(async () => { const st = await api("/api/tool", { name: "merge_ref", stack: true }); setState(st); return st.message; });
  ok(/merged/.test(merged), "merge reference tool: " + merged);
  // visual typesetting: drag sets \pos, shift-drag sets \clip
  await p.evaluate(async (cwd) => { setState(await api("/api/open_path", { path: cwd + "/tests/fixtures/typeset.ass" })); selectOnly(0); vid.style.display = "none"; updateOverlay(); }, process_cwd);
  await p.click("#bTypeset");
  const ob = await p.locator("#ovi").boundingBox();
  await p.mouse.move(ob.x + ob.width * 0.25, ob.y + ob.height * 0.5); await p.mouse.down();
  await p.mouse.move(ob.x + ob.width * 0.5, ob.y + ob.height * 0.5, { steps: 4 }); await p.mouse.up();
  await p.waitForFunction(() => { const m = /^\{\\pos\(([\d.]+),([\d.]+)\)\}/.exec(S.events[0].text); return m && Math.abs(m[1] - 640) < 12 && Math.abs(m[2] - 360) < 12; });
  ok(true, "dragging on the preview sets \\pos: " + (await p.evaluate(() => S.events[0].text.slice(0, 20))));
  await p.keyboard.down("Shift"); await p.mouse.move(ob.x + ob.width * 0.1, ob.y + ob.height * 0.1); await p.mouse.down();
  await p.mouse.move(ob.x + ob.width * 0.5, ob.y + ob.height * 0.5, { steps: 3 }); await p.mouse.up(); await p.keyboard.up("Shift");
  await p.waitForFunction(() => /\\clip\(/.test(S.events[0].text));
  ok(true, "shift-drag adds \\clip");
  await p.click("#bTypeset");
  // timing tools via API
  const lead = await p.evaluate(async () => { const before = S.events[1].start; const st = await api("/api/tool", { name: "lead", in: 100, out: 0 }); setState(st); return [before, S.events[1].start]; });
  ok(lead[1] <= lead[0], "lead-in tool ran");
  // interactive script dialog
  await p.evaluate(async (cwd) => { setState(await api("/api/open_path", { path: cwd + "/tests/fixtures/sample.ass" })); selectOnly(0); }, process_cwd);
  await p.click('#tabs [data-p="mac"]');
  await p.waitForSelector(".macro");
  await p.locator(".macro", { hasText: "Dialog" }).first().locator("button").click();
  await p.waitForSelector('#mbox [data-n="ms"]');
  ok((await p.inputValue('#mbox [data-n="ms"]')) === "100", "script dialog rendered with defaults");
  await p.fill('#mbox [data-n="ms"]', "333"); await p.fill('#mbox [data-n="tag"]', "ui"); await p.click('#mbox [data-b="Apply"]');
  await p.waitForFunction(() => S.events[0].effect === "ui" && S.events[0].start === 1333, null, { timeout: 10000 });
  ok(true, "dialog values reached the Lua macro");
  await p.locator(".macro", { hasText: "Dialog" }).first().locator("button").click();
  await p.waitForSelector('#mbox [data-n="ms"]'); await p.click('#mbox [data-b="Cancel"]');
  await p.waitForFunction(() => document.querySelector("#macLog").textContent.includes("cancelled"), null, { timeout: 10000 });
  ok(true, "Cancel button cancels the macro (document unchanged)");
  // aegisub.dialog.open -> server file browser
  await p.evaluate(async (cwd) => { setState(await api("/api/open_path", { path: cwd + "/tests/fixtures/sample.ass" })); selectOnly(0); }, process_cwd);
  await p.click('#tabs [data-p="mac"]');
  await p.setInputFiles("#fLua", "tests/fixtures/file_dialog.lua");
  await p.waitForSelector(".macro:has-text('pick') button, .macro b:text-is('pick')");
  await p.locator(".macro", { hasText: "pick" }).first().locator("button").click();
  await p.waitForSelector("#fPath");
  await p.fill("#fPath", process_cwd + "/tests/fixtures"); await p.evaluate(() => { document.querySelector("#fPath").dataset.stale = "1"; }); await p.press("#fPath", "Enter");
  await p.waitForFunction(() => document.querySelector("#fPath") && !document.querySelector("#fPath").dataset.stale && document.querySelector('#mbox .issue[data-n="ref.srt"]'));
  await p.click('#mbox .issue[data-n="ref.srt"]');
  await p.waitForFunction(() => S.events[0].effect.endsWith("ref.srt"), null, { timeout: 10000 });
  ok(true, "aegisub.dialog.open returned the picked path to Lua");
  // bundled karaoke templater driven from the Karaoke tab
  await p.evaluate(async (cwd) => { setState(await api("/api/open_path", { path: cwd + "/tests/fixtures/sample.ass" })); selectOnly(0); }, process_cwd);
  await p.click('#tabs [data-p="kara"]');
  await p.click("#kTplIns");
  await p.waitForFunction(() => S.events.filter(e => /^template/.test(e.effect)).length === 2);
  ok(true, "example templates inserted");
  await p.click("#kTplRun");
  await p.waitForFunction(() => S.events.filter(e => e.effect === "fx").length >= 3, null, { timeout: 15000 });
  ok(true, "bundled kara-templater generated fx lines (" + (await p.evaluate(() => S.events.filter(e => e.effect === "fx").length)) + ")");
  // find / select
  await p.evaluate(async (cwd) => { setState(await api("/api/open_path", { path: cwd + "/tests/fixtures/typeset.ass" })); selectOnly(0); }, process_cwd);
  await p.keyboard.press("Control+Shift+F");
  await p.waitForSelector("#fQ"); await p.fill("#fQ", "line"); await p.click("#fSel");
  await p.waitForFunction(() => document.querySelector("#fMsg").textContent.includes("selected"));
  ok((await p.evaluate(() => sel.size)) >= 1, "select all matches (" + (await p.evaluate(() => sel.size)) + ")");
  await p.keyboard.press("Escape");
  await p.evaluate(() => { sel = new Set(); cur = 0; renderAll(); });
  await p.fill("#eText", ""); // focus away from list shortcuts
  await p.evaluate(() => document.activeElement.blur());
  await p.keyboard.press("Control+a");
  ok((await p.evaluate(() => sel.size)) === 6, "Ctrl+A selects all lines");
  await p.evaluate(() => { sel = new Set([0]); cur = 0; });
  await p.evaluate(async () => { await copyLines(false); });
  const clip = await p.evaluate(() => Promise.race([navigator.clipboard.readText(), new Promise(r => setTimeout(() => r("no-clipboard"), 3000))]).catch(() => "no-clipboard"));
  ok(clip.startsWith("Dialogue:"), "copy lines to clipboard (" + clip.slice(0, 12) + ")");
  // paste: duplicate the copied line after itself
  await p.evaluate(async () => { await pasteLines(); });
  await p.waitForFunction(() => S.events.length === 7);
  ok(true, "paste inserts the copied line");
  // karaoke tab
  await p.evaluate(async (cwd) => { setState(await api("/api/open_path", { path: cwd + "/tests/fixtures/sample.ass" })); selectOnly(0); }, process_cwd);
  await p.click('#tabs [data-p="kara"]');
  await p.waitForSelector("#kChips input");
  const kvals = await p.$$eval("#kChips input", els => els.map(e => e.value));
  ok(kvals.join(",") === "20,30,50", "karaoke chips show syllable durations " + kvals);
  await p.fill('#kChips input[data-ki="1"]', "40"); await p.locator('#kChips input[data-ki="1"]').dispatchEvent("change");
  await p.waitForFunction(() => S.events[0].text.includes("\\k40"));
  ok(true, "editing a chip rewrites the \\k tag");
  await p.selectOption("#kMode", "char"); await p.click("#kSplit");
  await p.waitForFunction(() => (S.events[0].text.match(/\\kf/g) || []).length === 7);
  ok(true, "auto-split into characters");
  // styles modal
  await p.click("#bStyles");
  await p.fill('#mbox [data-k="fontsize"]', "52");
  await p.click("#sOk");
  await p.waitForTimeout(200);
  const st = await p.evaluate(() => api("/api/state"));
  ok(st.styles[0].fontsize === 52, "style edit persisted");
  await p.keyboard.press("Escape");
  await p.screenshot({ path: "/tmp/ui.png" });
  await b.close();
  const fails = errs.filter(e => e.startsWith("FAIL") || e.startsWith("pageerror") || (e.startsWith("console") && !/403/.test(e)));
  if (fails.length) { console.log(fails.join("\n")); process.exit(1); }
  console.log("UI smoke: all passed");
})();
