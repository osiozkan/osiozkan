-- Aegisub-compatible karaskel.lua (subset: collect_head, preproc_lines, syllable splitting)
local util = require("utils")
local karaskel = {}

function karaskel.collect_head(subs, generate_furigana)
  local meta, styles = { res_x = 0, res_y = 0 }, { n = 0 }
  local first_style_line = nil
  for i = 1, #subs do
    local l = subs[i]
    if l.class == "info" then
      local k = l.key:lower()
      if k == "playresx" then meta.res_x = math.floor(tonumber(l.value) or 0)
      elseif k == "playresy" then meta.res_y = math.floor(tonumber(l.value) or 0)
      elseif k == "wrapstyle" then meta.wrap_style = tonumber(l.value)
      elseif k == "scaledborderandshadow" then meta.scaled_border_and_shadow = l.value:lower() == "yes" end
    elseif l.class == "style" then
      styles.n = styles.n + 1; styles[styles.n] = l; styles[l.name] = l
      l.margin_v = l.margin_t
      first_style_line = first_style_line or i
    end
  end
  if meta.res_x == 0 and meta.res_y == 0 then meta.res_x = 384; meta.res_y = 288
  elseif meta.res_x == 0 then meta.res_x = math.floor(meta.res_y / 3 * 4)
  elseif meta.res_y == 0 then meta.res_y = math.floor(meta.res_x * 3 / 4) end
  for i = 1, styles.n do
    local s = styles[i]
    local w, h, d, e = aegisub.text_extents(s, "")
    s.height, s.descent, s.extlead = h, d, e
  end
  return meta, styles
end

local function set_pos(line, meta)
  local st = line.styleref
  local ml = (line.margin_l > 0) and line.margin_l or st.margin_l
  local mr = (line.margin_r > 0) and line.margin_r or st.margin_r
  local mv = (line.margin_t > 0) and line.margin_t or st.margin_t
  line.margin_v = mv
  local al = st.align
  if al == 1 or al == 4 or al == 7 then line.halign = "left"
  elseif al == 3 or al == 6 or al == 9 then line.halign = "right" else line.halign = "center" end
  if al >= 7 then line.valign = "top" elseif al >= 4 then line.valign = "middle" else line.valign = "bottom" end
  line.left, line.right = ml, meta.res_x - mr
  line.width = meta.res_x - ml - mr
  line.x = (line.halign == "left" and line.left) or (line.halign == "right" and line.right) or meta.res_x / 2
  line.y = (line.valign == "top" and mv) or (line.valign == "bottom" and meta.res_y - mv) or meta.res_y / 2
end

function karaskel.preproc_line_text(meta, styles, line)
  line.text_stripped = line.text:gsub("%b{}", "")
  local w, h, d, e = aegisub.text_extents(line.styleref, line.text_stripped)
  line.width, line.height, line.descent, line.extlead = w, h, d, e
end

function karaskel.preproc_line_size(meta, styles, line) karaskel.preproc_line_text(meta, styles, line) end

function karaskel.preproc_line_pos(meta, styles, line)
  local w, h = line.width, line.height
  set_pos(line, meta); line.width = w; line.height = h
end

function karaskel.parse_syllables(line)
  local sylls, cur, pos = {}, nil, 0
  local text = line.text
  line.kara = {n = 0}
  local i = 1
  local start = 0
  local pending = {dur = 0, tag = "k", text = "", tags = ""}
  local started = false
  for blk, rest in (text .. "{"):gmatch("(.-){([^}]*)}") do
    if blk ~= "" then pending.text = pending.text .. blk end
    local k, d = rest:match("\\([kK][fo]?)(%d+)")
    if k then
      if started or pending.text ~= "" then
        line.kara.n = line.kara.n + 1; local s = pending
        s.i = line.kara.n; s.start_time = start; s.duration = s.dur * 10; s.end_time = start + s.duration
        s.text_stripped = s.text; start = s.end_time; line.kara[line.kara.n] = s
      end
      pending = {dur = tonumber(d), tag = k, text = "", tags = rest:gsub("\\[kK][fo]?%d+", "")}
      started = true
    end
  end
  if started or pending.text ~= "" then
    line.kara.n = line.kara.n + 1; local s = pending
    s.i = line.kara.n; s.start_time = start; s.duration = s.dur * 10; s.end_time = start + s.duration
    s.text_stripped = s.text; line.kara[line.kara.n] = s
  end
end

function karaskel.preproc_line(subs, meta, styles, line)
  line.styleref = styles[line.style]
  if not line.styleref then line.styleref = styles[1] end
  line.duration = line.end_time - line.start_time
  line.kdur = 0
  line.time = 0
  karaskel.preproc_line_text(meta, styles, line)
  karaskel.preproc_line_pos(meta, styles, line)
  karaskel.parse_syllables(line)
end

function karaskel.preproc_lines(subs, meta, styles, lines)
  for i, l in ipairs(lines or {}) do karaskel.preproc_line(subs, meta, styles, l) end
end

function karaskel.use_furigana(line) end
return karaskel
