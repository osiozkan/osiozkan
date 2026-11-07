-- OsioSub karaskel.lua : Aegisub karaskel-compatible helpers
local util = require "util"
local karaskel = {}

local function style_extents(style, text)
  local w, h, d, e = aegisub.text_extents(style, text)
  return w, h, d, e
end

function karaskel.collect_head(subs, generate_furigana)
  local meta = { res_x = 0, res_y = 0 }
  local styles = { n = 0 }
  for i = 1, #subs do
    local l = subs[i]
    if l.class == "info" then
      local k = l.key:lower()
      if k == "playresx" then meta.res_x = math.floor(tonumber(l.value) or 0)
      elseif k == "playresy" then meta.res_y = math.floor(tonumber(l.value) or 0)
      elseif k == "wrapstyle" then meta.wrap_style = tonumber(l.value)
      elseif k == "scaledborderandshadow" then meta.scaled_border_and_shadow = l.value:lower() == "yes" end
    elseif l.class == "style" then
      styles.n = styles.n + 1
      styles[styles.n] = l
      styles[l.name] = l
      l.margin_v = l.margin_t
      l.width, l.height, l.descent, l.extlead = style_extents(l, "A")
    elseif l.class == "dialogue" then
      break
    end
  end
  if meta.res_x == 0 and meta.res_y == 0 then meta.res_x = 384; meta.res_y = 288
  elseif meta.res_x == 0 then meta.res_x = meta.res_y / 3 * 4
  elseif meta.res_y == 0 then meta.res_y = meta.res_x * 3 / 4 end
  meta.video_x_correct_factor = 1
  return meta, styles
end

function karaskel.parse_syls(line)
  -- split by {\k..} tags into syllables
  local kara = { n = 0 }
  local text = line.text
  local pos, cur = 1, nil
  local function push(dur, txt, tags)
    kara.n = kara.n + 1
    local s = { i = kara.n - 1, duration = dur * 10, text = txt, tag = tags, text_stripped = txt:gsub("%b{}", "") }
    kara[kara.n] = s
  end
  local pre = text:match("^([^{]*)")
  local started, lastdur, lasttag = false, 0, "k"
  local acc = ""
  for block, rest in text:gmatch("(%b{})([^{]*)") do
    local tag, dur = block:match("\\([kK][fo]?)(%d+)")
    if tag then
      if started then push(lastdur, acc, lasttag) end
      started, lastdur, lasttag, acc = true, tonumber(dur), tag:lower(), rest
    else
      if started then acc = acc .. block .. rest end
    end
  end
  if started then push(lastdur, acc, lasttag) end
  return kara
end

function karaskel.preproc_line_text(meta, styles, line)
  line.styleref = styles[line.style] or styles["Default"] or styles[1]
  line.text_stripped = line.text:gsub("%b{}", "")
  line.duration = line.end_time - line.start_time
  line.dur = line.duration
  line.kara = karaskel.parse_syls(line)
  line.kara_n = line.kara.n
  local t = 0
  for i, s in ipairs(line.kara) do
    s.start_time = t; t = t + s.duration; s.end_time = t
    s.text_spacestripped = s.text_stripped:gsub("^%s+", ""):gsub("%s+$", "")
    s.prespace = #(s.text_stripped:match("^%s*"))
    s.postspace = #(s.text_stripped:match("%s*$"))
  end
end

function karaskel.preproc_line_size(meta, styles, line)
  local st = line.styleref
  line.width, line.height, line.descent, line.extlead = aegisub.text_extents(st, line.text_stripped)
  local x = 0
  for _, s in ipairs(line.kara or {}) do
    s.width, s.height, s.descent, s.extlead = aegisub.text_extents(st, s.text_stripped)
    s.left = x; s.center = x + s.width / 2; s.right = x + s.width
    x = x + s.width
  end
end

function karaskel.preproc_line_pos(meta, styles, line)
  local st = line.styleref
  line.margin_l = line.margin_l > 0 and line.margin_l or st.margin_l
  line.margin_r = line.margin_r > 0 and line.margin_r or st.margin_r
  line.margin_v = (line.margin_t or 0) > 0 and line.margin_t or st.margin_t
  line.margin_t, line.margin_b = line.margin_v, line.margin_v
  local a = st.align
  line.halign = ({ "left", "center", "right" })[(a - 1) % 3 + 1]
  line.valign = a >= 7 and "top" or (a >= 4 and "middle" or "bottom")
  if line.halign == "left" then line.left = line.margin_l; line.center = line.left + line.width / 2; line.right = line.left + line.width
  elseif line.halign == "center" then line.left = (meta.res_x - line.margin_l - line.margin_r - line.width) / 2 + line.margin_l; line.center = line.left + line.width / 2; line.right = line.left + line.width
  else line.right = meta.res_x - line.margin_r; line.center = line.right - line.width / 2; line.left = line.right - line.width end
  if line.valign == "top" then line.top = line.margin_v; line.middle = line.top + line.height / 2; line.bottom = line.top + line.height
  elseif line.valign == "middle" then line.top = (meta.res_y - line.height) / 2; line.middle = line.top + line.height / 2; line.bottom = line.top + line.height
  else line.bottom = meta.res_y - line.margin_v; line.middle = line.bottom - line.height / 2; line.top = line.bottom - line.height end
  line.x = line.halign == "left" and line.left or (line.halign == "center" and line.center or line.right)
  line.y = line.valign == "top" and line.top or (line.valign == "middle" and line.middle or line.bottom)
  for _, s in ipairs(line.kara or {}) do
    s.left = s.left + line.left; s.center = s.center + line.left; s.right = s.right + line.left
  end
end

function karaskel.preproc_line(subs, meta, styles, line)
  karaskel.preproc_line_text(meta, styles, line)
  karaskel.preproc_line_size(meta, styles, line)
  karaskel.preproc_line_pos(meta, styles, line)
end

function karaskel.use_furigana(...) return end
return karaskel
