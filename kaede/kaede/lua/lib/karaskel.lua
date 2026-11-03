-- karaskel.lua: Aegisub-compatible karaoke skeleton helpers
local util = require "util"
local karaskel = {}

function karaskel.collect_head(subs, generate_furigana)
  local meta = { res_x = 0, res_y = 0 }
  local styles = { n = 0 }
  for i = 1, #subs do
    local l = subs[i]
    if l.class == "style" then
      styles.n = styles.n + 1
      styles[styles.n] = l
      styles[l.name] = l
      l.id = styles.n
    elseif l.class == "info" then
      local k = (l.key or ""):lower()
      if k == "playresx" then meta.res_x = math.floor(tonumber(l.value) or 0)
      elseif k == "playresy" then meta.res_y = math.floor(tonumber(l.value) or 0)
      elseif k == "wrapstyle" then meta.wrap_style = tonumber(l.value)
      elseif k == "scaledborderandshadow" then meta.scale_bs = (l.value or ""):lower() == "yes"
      elseif k == "title" then meta.title = l.value end
      meta.info = meta.info or {}
      meta.info[l.key] = l.value
    elseif l.class == "dialogue" then break end
  end
  if meta.res_x == 0 and meta.res_y == 0 then
    meta.res_x = 384; meta.res_y = 288
  elseif meta.res_y == 0 then meta.res_y = math.floor(meta.res_x * 3 / 4)
  elseif meta.res_x == 0 then meta.res_x = math.floor(meta.res_y * 4 / 3) end
  meta.video_x_correct_factor = 1
  if styles.n == 0 then
    local def = { class = "style", name = "Default", fontname = "Arial", fontsize = 48, color1 = "&H00FFFFFF", color2 = "&H000000FF",
      color3 = "&H00000000", color4 = "&H00000000", bold = false, italic = false, underline = false, strikeout = false,
      scale_x = 100, scale_y = 100, spacing = 0, angle = 0, borderstyle = 1, outline = 2, shadow = 2, align = 2,
      margin_l = 10, margin_r = 10, margin_t = 10, margin_b = 10, encoding = 1, relative_to = 2, section = "[V4+ Styles]" }
    styles.n = 1; styles[1] = def; styles.Default = def; def.id = 1
  end
  return meta, styles
end

function karaskel.preproc_line_text(meta, styles, line)
  line.styleref = styles[line.style]
  if not line.styleref then
    aegisub.log(2, "WARNING: Style not found: " .. tostring(line.style) .. "\n")
    line.styleref = styles["Default"] or styles[1]
  end
  line.text_stripped = line.text:gsub("%b{}", "")
  line.duration = line.end_time - line.start_time
end

function karaskel.preproc_line_size(meta, styles, line)
  local w, h, d, e = aegisub.text_extents(line.styleref, line.text_stripped)
  line.width, line.height, line.descent, line.extlead = w, h, d, e
end

function karaskel.preproc_line_pos(meta, styles, line)
  local st = line.styleref
  line.margin_v = line.margin_t
  line.eff_margin_l = (line.margin_l > 0) and line.margin_l or st.margin_l
  line.eff_margin_r = (line.margin_r > 0) and line.margin_r or st.margin_r
  line.eff_margin_t = (line.margin_t > 0) and line.margin_t or st.margin_t
  line.eff_margin_b = (line.margin_b > 0) and line.margin_b or st.margin_b
  line.eff_margin_v = line.eff_margin_t
  local a = st.align
  local m = a % 3
  line.halign = (m == 1 and "left") or (m == 2 and "center") or "right"
  line.valign = (a > 6 and "top") or (a > 3 and "middle") or "bottom"
  line.left = line.eff_margin_l
  line.right = meta.res_x - line.eff_margin_r
  line.center = line.left + (meta.res_x - line.eff_margin_l - line.eff_margin_r) / 2
  line.top = line.eff_margin_t
  line.bottom = meta.res_y - line.eff_margin_b
  line.middle = math.floor(meta.res_y / 2)
  line.x = (line.halign == "left" and line.left) or (line.halign == "center" and line.center) or line.right
  line.y = (line.valign == "top" and line.top) or (line.valign == "middle" and line.middle) or line.bottom
end

function karaskel.parse_karaoke_data(line, styles_or_meta)
  -- splits text into syllables by \k \kf \ko \K tags
  local kara = { n = 0 }
  local cur_start = 0
  local text = line.text
  local st = line.styleref
  local pos = 1
  local pending = { text = "", tag = "", dur = 0, inline = "" }
  local function push(syl)
    kara.n = kara.n + 1
    syl.i = kara.n
    syl.duration = syl.dur * 10
    syl.start_time = cur_start
    syl.end_time = cur_start + syl.duration
    cur_start = syl.end_time
    syl.text_stripped = syl.text:gsub("%b{}", "")
    syl.text_spacestripped = syl.text_stripped:gsub("^%s+", ""):gsub("%s+$", "")
    syl.prespace = #(syl.text_stripped:match("^%s*"))
    syl.postspace = #(syl.text_stripped:match("%s*$"))
    syl.kdur = syl.dur
    kara[kara.n] = syl
  end
  local syl = nil
  local found = false
  for pre, block, rest in (text .. "{}"):gmatch("([^{]*)(%b{})") do
    if pre ~= "" then
      if not syl then syl = { text = "", tag = "k", dur = 0 } end
      syl.text = syl.text .. pre
    end
    local kk = block:match("\\([kK][fo]?)(%-?%d+)")
    local ktag, kdur = block:match("\\([kK][fo]?)(%-?%d+)")
    if ktag then
      found = true
      if syl then push(syl) end
      syl = { text = "", tag = ktag, dur = tonumber(kdur), inline_fx = block:match("\\%-?([%w_]+)") }
      syl.text = block:gsub("\\[kK][fo]?%-?%d+", ""):gsub("^{}$", "")
      if syl.text == "{}" then syl.text = "" end
    elseif block ~= "{}" then
      if not syl then syl = { text = "", tag = "k", dur = 0 } end
      syl.text = syl.text .. block
    end
  end
  if syl then push(syl) end
  if not found and kara.n == 0 then
    kara.n = 0
  end
  -- measure
  local x = 0
  for i = 1, kara.n do
    local s = kara[i]
    local w, h, d, e = aegisub.text_extents(st, s.text_stripped)
    s.width, s.height, s.descent, s.extlead = w, h, d, e
    s.left = x
    s.center = x + w / 2
    s.right = x + w
    x = x + w
    s.style = st
    s.line = line
  end
  line.kara = kara
  return kara
end

function karaskel.preproc_line(subs, meta, styles, line)
  karaskel.preproc_line_text(meta, styles, line)
  karaskel.preproc_line_size(meta, styles, line)
  karaskel.preproc_line_pos(meta, styles, line)
  if line.text:find("\\[kK][fo]?%-?%d") then karaskel.parse_karaoke_data(line, meta) end
end

function karaskel.do_basic_layout(...) return karaskel.preproc_line(...) end

function karaskel.fix_line_time(line) end

return karaskel
