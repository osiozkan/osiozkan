/** Küçük modal ve Aegisub diyalog (config) oluşturucu. */
export function modal(title: string, body: HTMLElement, buttons: { label: string; primary?: boolean; value: string }[]): Promise<string | null> {
  return new Promise((resolve) => {
    const ov = document.createElement("div"); ov.className = "modal-ov";
    const box = document.createElement("div"); box.className = "modal";
    const h = document.createElement("h3"); h.textContent = title;
    const foot = document.createElement("div"); foot.className = "modal-foot";
    const done = (v: string | null) => { ov.remove(); document.removeEventListener("keydown", onKey, true); resolve(v); };
    const onKey = (e: KeyboardEvent) => { if (e.key === "Escape") { e.stopPropagation(); done(null); } };
    document.addEventListener("keydown", onKey, true);
    for (const b of buttons) {
      const el = document.createElement("button"); el.textContent = b.label; if (b.primary) el.className = "primary";
      el.onclick = () => done(b.value); foot.appendChild(el);
    }
    box.append(h, body, foot); ov.appendChild(box); document.body.appendChild(ov);
    (body.querySelector("input,textarea,select") as HTMLElement | null)?.focus();
  });
}

export function field(label: string, input: HTMLElement): HTMLElement {
  const l = document.createElement("label"); l.className = "fld"; const s = document.createElement("span"); s.textContent = label;
  l.append(s, input); return l;
}
export function inp(value: string | number = "", type = "text"): HTMLInputElement {
  const i = document.createElement("input"); i.type = type; i.value = String(value); return i;
}

/** Aegisub aegisub.dialog.display(config) -> DOM; sonuç {button, values}. */
export async function luaDialog(config: any[], buttons: string[]): Promise<{ button: string | false; values: Record<string, any> }> {
  const grid = document.createElement("div"); grid.className = "lua-dlg";
  const getters: Record<string, () => any> = {};
  let maxX = 0, maxY = 0;
  for (const c of config) { maxX = Math.max(maxX, (c.x ?? 0) + (c.width ?? 1)); maxY = Math.max(maxY, (c.y ?? 0) + (c.height ?? 1)); }
  grid.style.gridTemplateColumns = `repeat(${Math.max(maxX, 1)}, minmax(70px, 1fr))`;
  for (const c of config) {
    let el: HTMLElement;
    const cls = c.class as string;
    if (cls === "label") { el = document.createElement("span"); el.textContent = c.label ?? ""; }
    else if (cls === "checkbox") {
      const i = document.createElement("input"); i.type = "checkbox"; i.checked = !!c.value;
      const l = document.createElement("label"); l.append(i, " " + (c.label ?? "")); el = l; if (c.name) getters[c.name] = () => i.checked;
    } else if (cls === "dropdown") {
      const s = document.createElement("select");
      for (const it of c.items ?? []) { const o = document.createElement("option"); o.textContent = String(it); s.appendChild(o); }
      s.value = c.value ?? (c.items?.[0] ?? ""); el = s; if (c.name) getters[c.name] = () => s.value;
    } else if (cls === "textbox") {
      const t = document.createElement("textarea"); t.value = c.text ?? c.value ?? ""; el = t; if (c.name) getters[c.name] = () => t.value;
    } else if (cls === "intedit" || cls === "floatedit" || cls === "scrollbar") {
      const i = inp(c.value ?? 0, "number"); if (c.min !== undefined) i.min = c.min; if (c.max !== undefined) i.max = c.max; if (cls === "floatedit") i.step = c.step ?? "any";
      el = i; if (c.name) getters[c.name] = () => (cls === "intedit" ? Math.round(+i.value) : +i.value);
    } else if (cls === "color" || cls === "coloralpha") {
      const i = document.createElement("input"); i.type = "color";
      const m = /&?H?([0-9a-f]{2})?([0-9a-f]{2})([0-9a-f]{2})([0-9a-f]{2})&?$/i.exec(String(c.value ?? "&H00FFFFFF&"));
      i.value = m ? `#${m[4]}${m[3]}${m[2]}` : "#ffffff"; el = i;
      if (c.name) getters[c.name] = () => { const h = i.value.slice(1); return `&H00${h.slice(4, 6)}${h.slice(2, 4)}${h.slice(0, 2)}&`.toUpperCase().replace("&H", "&H"); };
    } else { // edit, bilinmeyen
      const i = inp(c.text ?? c.value ?? ""); el = i; if (c.name) getters[c.name] = () => i.value;
    }
    el.title = c.hint ?? "";
    el.style.gridColumn = `${(c.x ?? 0) + 1} / span ${c.width ?? 1}`; el.style.gridRow = `${(c.y ?? 0) + 1} / span ${c.height ?? 1}`;
    grid.appendChild(el);
  }
  const res = await modal("Betik", grid, buttons.map((b, i) => ({ label: b, value: b, primary: i === 0 })));
  const values: Record<string, any> = {};
  for (const [k, g] of Object.entries(getters)) values[k] = g();
  return { button: res ?? false, values };
}
