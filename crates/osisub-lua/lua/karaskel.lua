-- osisub: Aegisub karaskel.lua uyumlu karaoke iskeleti
include("utils.lua")

karaskel = {}

function karaskel.collect_head(subs, generate_furigana)
  local meta = { res_x = 0, res_y = 0 }
  local styles = { n = 0 }
  for i = 1, #subs do
    local l = subs[i]
    if l.class == "style" then
      styles.n = styles.n + 1
      styles[styles.n] = l
      styles[l.name] = l
      l.margin_v = l.margin_t
    elseif l.class == "info" then
      local k = l.key:lower()
      if k == "playresx" then meta.res_x = math.floor(tonumber(l.value) or 0)
      elseif k == "playresy" then meta.res_y = math.floor(tonumber(l.value) or 0)
      elseif k == "wrapstyle" then meta.wrap_style = tonumber(l.value)
      elseif k == "scaledborderandshadow" then meta.scaled_border_and_shadow = l.value:lower() == "yes"
      end
    elseif l.class == "dialogue" then
      break
    end
  end
  if meta.res_x == 0 and meta.res_y == 0 then
    meta.res_x, meta.res_y = 384, 288
  elseif meta.res_x == 0 then
    meta.res_x = meta.res_y / 3 * 4
  elseif meta.res_y == 0 then
    meta.res_y = meta.res_x * 3 / 4
  end
  local vx, vy = aegisub.video_size()
  meta.video_x_correction = 1
  if vx and vy then meta.video_x_correction = (vx / vy) / (meta.res_x / meta.res_y) end
  for i = 1, styles.n do
    local s = styles[i]
    s.width, s.height, s.descent, s.extlead = aegisub.text_extents(s, "Hesobyo")
  end
  return meta, styles
end

local function split_syllables(text)
  local syls = {}
  local cur = nil
  local function finish()
    if cur then table.insert(syls, cur); cur = nil end
  end
  local pos = 1
  while pos <= #text do
    local s, e = text:find("%b{}", pos)
    local plain_end = (s or (#text + 1)) - 1
    if plain_end >= pos then
      cur = cur or { duration = 0, tag = "\\k", text = "" }
      cur.text = cur.text .. text:sub(pos, plain_end)
    end
    if not s then break end
    local block = text:sub(s + 1, e - 1)
    local ks, ke, ktag, kdur = block:find("\\([kK][fo]?)(%-?[%d%.]+)")
    if ks then
      finish()
      local rest = block:sub(1, ks - 1) .. block:sub(ke + 1)
      cur = { duration = math.floor(tonumber(kdur) * 10 + 0.5), tag = "\\" .. ktag, text = "" }
      if rest ~= "" then cur.text = "{" .. rest .. "}" end
    else
      cur = cur or { duration = 0, tag = "\\k", text = "" }
      cur.text = cur.text .. text:sub(s, e)
    end
    pos = e + 1
  end
  finish()
  return syls
end

function karaskel.parse_syllables(line)
  local kara = { n = 0 }
  local t = 0
  for _, syl in ipairs(split_syllables(line.text)) do
    kara.n = kara.n + 1
    syl.i = kara.n
    syl.start_time = t
    syl.end_time = t + syl.duration
    t = syl.end_time
    syl.text_stripped = syl.text:gsub("%b{}", "")
    local pre, body, post = syl.text_stripped:match("^(%s*)(.-)(%s*)$")
    syl.text_spacestripped = body
    syl.prespace_text, syl.postspace_text = pre, post
    kara[kara.n] = syl
  end
  return kara
end

function karaskel.preproc_line_text(meta, styles, line)
  line.text_stripped = line.text:gsub("%b{}", "")
  line.duration = line.end_time - line.start_time
  line.kara = karaskel.parse_syllables(line)
end

function karaskel.preproc_line_size(meta, styles, line)
  local st = line.styleref
  line.width, line.height, line.descent, line.extlead = aegisub.text_extents(st, line.text_stripped)
  for i = 1, line.kara.n do
    local syl = line.kara[i]
    syl.width, syl.height = aegisub.text_extents(st, syl.text_spacestripped)
    syl.prespace = aegisub.text_extents(st, syl.prespace_text)
    syl.postspace = aegisub.text_extents(st, syl.postspace_text)
  end
end

function karaskel.preproc_line_pos(meta, styles, line)
  local st = line.styleref
  line.margin_v = line.margin_t
  line.eff_margin_l = line.margin_l > 0 and line.margin_l or st.margin_l
  line.eff_margin_r = line.margin_r > 0 and line.margin_r or st.margin_r
  line.eff_margin_t = line.margin_t > 0 and line.margin_t or st.margin_t
  line.eff_margin_b = line.margin_b > 0 and line.margin_b or st.margin_b
  line.eff_margin_v = line.eff_margin_t
  local align = st.align
  local ha = (align - 1) % 3
  if ha == 0 then
    line.halign = "left"
    line.left = line.eff_margin_l
    line.center = line.left + line.width / 2
    line.right = line.left + line.width
    line.x = line.left
  elseif ha == 1 then
    line.halign = "center"
    line.left = (meta.res_x - line.eff_margin_l - line.eff_margin_r - line.width) / 2 + line.eff_margin_l
    line.center = line.left + line.width / 2
    line.right = line.left + line.width
    line.x = line.center
  else
    line.halign = "right"
    line.right = meta.res_x - line.eff_margin_r
    line.left = line.right - line.width
    line.center = line.left + line.width / 2
    line.x = line.right
  end
  if align >= 7 then
    line.valign = "top"
    line.top = line.eff_margin_t
    line.middle = line.top + line.height / 2
    line.bottom = line.top + line.height
    line.y = line.top
  elseif align >= 4 then
    line.valign = "middle"
    line.middle = meta.res_y / 2
    line.top = line.middle - line.height / 2
    line.bottom = line.top + line.height
    line.y = line.middle
  else
    line.valign = "bottom"
    line.bottom = meta.res_y - line.eff_margin_b
    line.middle = line.bottom - line.height / 2
    line.top = line.bottom - line.height
    line.y = line.bottom
  end
  line.vcenter = line.middle
  -- hece konumları
  local x = line.left
  for i = 1, line.kara.n do
    local syl = line.kara[i]
    syl.left = x + syl.prespace
    syl.center = syl.left + syl.width / 2
    syl.right = syl.left + syl.width
    x = syl.right + syl.postspace
  end
end

function karaskel.preproc_line(subs, meta, styles, line)
  line.styleref = styles[line.style]
  if not line.styleref then
    aegisub.log(0, "Bilinmeyen stil: %s\n", tostring(line.style))
    line.styleref = styles[1]
  end
  karaskel.preproc_line_text(meta, styles, line)
  karaskel.preproc_line_size(meta, styles, line)
  karaskel.preproc_line_pos(meta, styles, line)
end

function karaskel.do_basic_layout(subs, meta, styles, line)
  karaskel.preproc_line(subs, meta, styles, line)
end
