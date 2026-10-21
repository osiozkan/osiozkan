// UI smoke test: node tests/ui_smoke.cjs  (expects `subforge serve tests/fixtures/sample.ass` on :8765)
const { chromium } = require(process.env.PW_PATH || "playwright");
const process_cwd = process.cwd();
const fs = require("fs");
(async () => {
  const b = await chromium.launch({ executablePath: "/opt/pw-browsers/chromium" });
  const p = await b.newPage({ viewport: { width: 1400, height: 850 } });
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
  ok((await p.locator(".macro").count()) === 5, "5 macros listed");
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
  const fails = errs.filter(e => e.startsWith("FAIL") || e.startsWith("pageerror") || e.startsWith("console"));
  if (fails.length) { console.log(fails.join("\n")); process.exit(1); }
  console.log("UI smoke: all passed");
})();
