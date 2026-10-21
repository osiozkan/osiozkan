// UI smoke test: node tests/ui_smoke.cjs  (expects `subforge serve tests/fixtures/sample.ass` on :8765)
const { chromium } = require(process.env.PW_PATH || "playwright");
const fs = require("fs");
(async () => {
  const b = await chromium.launch({ executablePath: "/opt/pw-browsers/chromium" });
  const p = await b.newPage({ viewport: { width: 1400, height: 850 } });
  const errs = [];
  p.on("pageerror", e => errs.push("pageerror: " + e.message));
  p.on("console", m => { if (m.type() === "error") errs.push("console: " + m.text()); });
  const ok = (c, m) => { if (!c) { errs.push("FAIL: " + m); } else console.log("ok  " + m); };
  await p.goto("http://127.0.0.1:8765/");
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
  // styles modal
  await p.click("#bStyles");
  await p.fill('#mbox [data-k="fontsize"]', "52");
  await p.click("#sOk");
  await p.waitForTimeout(200);
  const st = await (await fetch("http://127.0.0.1:8765/api/state")).json();
  ok(st.styles[0].fontsize === 52, "style edit persisted");
  await p.keyboard.press("Escape");
  await p.screenshot({ path: "/tmp/ui.png" });
  await b.close();
  const fails = errs.filter(e => e.startsWith("FAIL") || e.startsWith("pageerror") || e.startsWith("console"));
  if (fails.length) { console.log(fails.join("\n")); process.exit(1); }
  console.log("UI smoke: all passed");
})();
