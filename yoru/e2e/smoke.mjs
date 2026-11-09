// Tarayıcı duman testi: node e2e/smoke.mjs  (önce `npm run build`)
import { chromium } from "playwright";
import { spawn } from "node:child_process";
import { writeFileSync, mkdirSync } from "node:fs";
const out = process.argv[2] ?? "/tmp/yoru-shot.png";
const srv = spawn("npx", ["vite", "preview", "--port", "4399", "--strictPort"], { stdio: "ignore" });
await new Promise((r) => setTimeout(r, 2500));
const b = await chromium.launch(); const p = await b.newPage({ viewport: { width: 1400, height: 850 } });
const errs = []; p.on("pageerror", (e) => errs.push(String(e))); p.on("console", (m) => m.type() === "error" && errs.push(m.text()));
await p.goto("http://localhost:4399/");
mkdirSync("/tmp/yoru-e2e", { recursive: true });
writeFileSync("/tmp/yoru-e2e/t.lua", `script_name="Test"
aegisub.register_macro("Büyük harf","d",function(subs,sel)
  for _,i in ipairs(sel) do local l=subs[i]; l.text=l.text:upper(); subs[i]=l end
  aegisub.log("ok %d", #sel)
end)`);
await p.click("#b-auto");
await p.setInputFiles("#f-lua", "/tmp/yoru-e2e/t.lua");
await p.waitForSelector(".list .it");
await p.click(".list .it");
await p.waitForSelector("pre.log");
await p.click(".modal-foot button");
const t = await p.inputValue("#e-text");
await p.screenshot({ path: out });
console.log("text:", t, "errors:", JSON.stringify(errs));
await b.close(); srv.kill();
if (!/YORU/.test(t) || errs.length) process.exit(1);
