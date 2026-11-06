-- karaskel.lua uyumlu çekirdek (Aegisub 3.x). Metin ölçümü aegisub.text_extents'e dayanır.
include("utils.lua")
karaskel = {}

function karaskel.collect_head(subs, generate_furigana)
  local meta = { res_x = 0, res_y = 0, wrap_style = 0 }
  local styles = { n = 0 }
  local have_script_res = false
  for i = 1, #subs do
    local l = subs[i]
    if l.class == "style" then
      styles.n = styles.n + 1
      l.margin_v = l.margin_t
      styles[l.name] = l
      styles[styles.n] = l
    elseif l.class == "info" then
      local k = l.key:lower()
      meta[l.key] = l.value
      if k == "playresx" then meta.res_x = tonumber(l.value) or 0
      elseif k == "playresy" then meta.res_y = tonumber(l.value) or 0
      elseif k == "wrapstyle" then meta.wrap_style = tonumber(l.value) or 0 end
    end
  end
  if meta.res_x == 0 and meta.res_y == 0 then meta.res_x, meta.res_y = 384, 288
  elseif meta.res_x == 0 then meta.res_x = meta.res_y / 3 * 4
  elseif meta.res_y == 0 then meta.res_y = meta.res_x / 4 * 3 end
  meta.video_x_correct_factor = 1
  return meta, styles
end

function karaskel.use_furigana(styles) end

local function parse_kara(text)
  local out, cur = {}, nil
  local pos = 1
  local n = 0
  while pos <= #text do
    local b = text:find("{", pos, true)
    local seg = text:sub(pos, (b or #text + 1) - 1)
    if seg ~= "" then
      if not cur then cur = { duration = 0, kind = "k", tag = "k", text = "", tags = "" }; out[#out + 1] = cur end
      cur.text = cur.text .. seg
    end
    if not b then break end
    local e = text:find("}", b, true)
    if not e then break end
    local block = text:sub(b + 1, e - 1)
    local found = false
    for tag, val in block:gmatch("\\([kK][fo]?)(%-?%d+)") do
      found = true
      cur = { duration = tonumber(val) * 10, kind = tag, tag = tag, text = "", tags = "" }
      out[#out + 1] = cur
    end
    if not found and cur then cur.tags = cur.tags .. block end
    pos = e + 1
  end
  return out
end

function karaskel.preproc_line_text(meta, styles, line)
  line.text_stripped = line.text:gsub("%b{}", ""):gsub("\\[Nnh]", function(c) return c == "\\h" and " " or "" end)
  line.duration = line.end_time - line.start_time
  line.kara = parse_kara(line.text)
  local t = 0
  line.kara.n = #line.kara
  for i, s in ipairs(line.kara) do
    s.i = i
    s.start_time = t
    s.end_time = t + s.duration
    s.text_stripped = s.text:gsub("%b{}", "")
    s.text_spacestripped = s.text_stripped:gsub("^%s+", ""):gsub("%s+$", "")
    s.duration_cs = s.duration / 10
    t = t + s.duration
  end
  line.kdur = t
end

function karaskel.preproc_line_size(meta, styles, line)
  local st = line.styleref
  local w, h, d, el = aegisub.text_extents(st, line.text_stripped)
  line.width, line.height, line.descent, line.extlead = w, h, d, el
  for _, s in ipairs(line.kara or {}) do
    s.width, s.height, s.descent, s.extlead = aegisub.text_extents(st, s.text_stripped)
  end
  local left = 0
  for _, s in ipairs(line.kara or {}) do
    s.left = left
    s.center = left + s.width / 2
    s.right = left + s.width
    left = left + s.width
  end
end

function karaskel.preproc_line_pos(meta, styles, line)
  local st = line.styleref
  line.margin_l = (line.margin_l and line.margin_l > 0) and line.margin_l or st.margin_l
  line.margin_r = (line.margin_r and line.margin_r > 0) and line.margin_r or st.margin_r
  line.margin_v = (line.margin_t and line.margin_t > 0) and line.margin_t or st.margin_t
  line.eff_margin_l, line.eff_margin_r, line.eff_margin_t, line.eff_margin_b =
    line.margin_l, line.margin_r, line.margin_v, line.margin_v
  local a = st.align
  line.halign = (a - 1) % 3 == 0 and "left" or ((a - 1) % 3 == 1 and "center" or "right")
  line.valign = a <= 3 and "bottom" or (a <= 6 and "middle" or "top")
  line.hcenter = line.halign
  if line.halign == "left" then line.left = line.margin_l; line.center = line.left + line.width / 2; line.right = line.left + line.width
  elseif line.halign == "right" then line.right = meta.res_x - line.margin_r; line.center = line.right - line.width / 2; line.left = line.right - line.width
  else line.left = (meta.res_x - line.margin_l - line.margin_r - line.width) / 2 + line.margin_l; line.center = line.left + line.width / 2; line.right = line.left + line.width end
  if line.valign == "top" then line.top = line.margin_v; line.middle = line.top + line.height / 2; line.bottom = line.top + line.height
  elseif line.valign == "bottom" then line.bottom = meta.res_y - line.margin_v; line.middle = line.bottom - line.height / 2; line.top = line.bottom - line.height
  else line.top = (meta.res_y - line.height) / 2; line.middle = meta.res_y / 2; line.bottom = line.top + line.height end
  line.x = line.halign == "left" and line.left or (line.halign == "center" and line.center or line.right)
  line.y = line.valign == "top" and line.top or (line.valign == "middle" and line.middle or line.bottom)
  for _, s in ipairs(line.kara or {}) do
    s.left = s.left + line.left; s.center = s.center + line.left; s.right = s.right + line.left
    s.top, s.middle, s.bottom = line.top, line.middle, line.bottom
    s.x = line.halign == "left" and s.left or (line.halign == "center" and s.center or s.right)
    s.y = line.y
  end
end

function karaskel.preproc_line(subs, meta, styles, line)
  if line.class ~= "dialogue" then return end
  line.styleref = styles[line.style] or styles["Default"] or styles[1]
  karaskel.preproc_line_text(meta, styles, line)
  karaskel.preproc_line_size(meta, styles, line)
  karaskel.preproc_line_pos(meta, styles, line)
end

function karaskel.preproc(subs, meta, styles)
  for i = 1, #subs do
    local l = subs[i]
    if l.class == "dialogue" then karaskel.preproc_line(subs, meta, styles, l); subs[i] = l end
  end
end
