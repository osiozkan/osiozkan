-- karaskel: karaoke/style preprocessing library (Aegisub-compatible API)
include("utils.lua")

karaskel = {}

local function res_defaults(meta)
  if meta.res_x == 0 and meta.res_y == 0 then
    meta.res_x, meta.res_y = 384, 288
  elseif meta.res_x == 0 then
    meta.res_x = meta.res_y / 3 * 4
  elseif meta.res_y == 0 then
    meta.res_y = meta.res_x * 3 / 4
  end
end

function karaskel.collect_head(subs, generate_furigana)
  local meta = {res_x = 0, res_y = 0, video_x_correct_factor = 1.0}
  local styles = {n = 0}
  local i = 1
  while i <= #subs do
    local l = subs[i]
    if l.class == "info" then
      local k = l.key:lower()
      if k == "playresx" then meta.res_x = math.floor(tonumber(l.value) or 0)
      elseif k == "playresy" then meta.res_y = math.floor(tonumber(l.value) or 0)
      elseif k == "wrapstyle" then meta.wrap_style = tonumber(l.value)
      elseif k == "scaledborderandshadow" then meta.scaled_border_and_shadow = l.value:lower() == "yes"
      elseif k == "title" then meta.title = l.value
      end
      meta[l.key] = l.value
    elseif l.class == "style" then
      l.margin_v = l.margin_t
      styles.n = styles.n + 1
      styles[styles.n] = l
      styles[l.name] = l
    elseif l.class == "dialogue" then
      break
    end
    i = i + 1
  end
  res_defaults(meta)
  -- video aspect correction (square pixels assumed when unknown)
  meta.video_x_correct_factor = 1.0
  if generate_furigana then
    local n = styles.n
    for j = 1, n do
      local s = styles[j]
      if not s.name:match("%-furigana$") then
        local f = table.copy(s)
        f.name = s.name .. "-furigana"
        f.fontsize = s.fontsize * 0.5
        f.margin_v = s.margin_v + s.fontsize
        styles[f.name] = f
        styles.n = styles.n + 1
        styles[styles.n] = f
      end
    end
  end
  return meta, styles
end

function karaskel.preproc_line_text(meta, styles, line)
  local style = line.styleref
  line.text_stripped = line.text:gsub("%b{}", "")
  line.kara = karaskel.parse_syllables(line, style)
  -- stripped text without trailing/leading override markers
  line.duration = line.end_time - line.start_time
end

function karaskel.parse_syllables(line, style)
  local kara = {n = 0}
  local text = line.text
  local pos = 1
  local cur_start_cs = 0
  local cur
  local pending_tags = ""
  local function new_syl(dur, tag, pre)
    local s = {duration = dur * 10, kdur = dur, tag = tag, text = "", tags = pre or "",
               start_time = cur_start_cs * 10, i = kara.n + 1, prespace = "", postspace = ""}
    cur_start_cs = cur_start_cs + dur
    s.end_time = s.start_time + s.duration
    kara.n = kara.n + 1
    kara[kara.n] = s
    return s
  end
  while pos <= #text do
    local bs, be = text:find("%b{}", pos)
    if not bs then
      if not cur then cur = new_syl(0, "k", pending_tags); pending_tags = "" end
      cur.text = cur.text .. text:sub(pos)
      break
    end
    if bs > pos then
      if not cur then cur = new_syl(0, "k", pending_tags); pending_tags = "" end
      cur.text = cur.text .. text:sub(pos, bs - 1)
    end
    local block = text:sub(bs, be)
    local tag, dur = block:match("\\([kK][fo]?)(%-?%d+%.?%d*)")
    if tag then
      cur = new_syl(math.floor(tonumber(dur)), tag:lower() == "k" and tag or tag:lower(), pending_tags)
      pending_tags = ""
      local rest = block:gsub("\\[kK][fo]?%-?%d+%.?%d*", "", 1)
      if rest ~= "{}" then cur.text = cur.text .. rest end
    else
      if cur then cur.text = cur.text .. block else pending_tags = pending_tags .. block end
    end
    pos = be + 1
  end
  for _, s in ipairs(kara) do
    s.text_stripped = s.text:gsub("%b{}", "")
    local pre, body, post = s.text_stripped:match("^(%s*)(.-)(%s*)$")
    s.prespace_text, s.text_spacestripped, s.postspace_text = pre, body, post
    s.prespace, s.postspace = #pre, #post
    s.style = style
    s.line = line
  end
  return kara
end

function karaskel.preproc_line_size(meta, styles, line)
  local style = line.styleref
  local w, h, d, el = aegisub.text_extents(style, line.text_stripped)
  line.width, line.height, line.descent, line.extlead = w, h, d, el
  for _, s in ipairs(line.kara) do
    local sw, sh, sd, se = aegisub.text_extents(style, s.text_stripped)
    s.width, s.height, s.descent, s.extlead = sw, sh, sd, se
    s.prespace_width = aegisub.text_extents(style, s.prespace_text)
    s.postspace_width = aegisub.text_extents(style, s.postspace_text)
  end
end

function karaskel.preproc_line_pos(meta, styles, line)
  local style = line.styleref
  line.margin_v = line.margin_t
  line.eff_margin_l = line.margin_l > 0 and line.margin_l or style.margin_l
  line.eff_margin_r = line.margin_r > 0 and line.margin_r or style.margin_r
  line.eff_margin_t = line.margin_t > 0 and line.margin_t or style.margin_t
  line.eff_margin_b = line.margin_b > 0 and line.margin_b or style.margin_b
  line.eff_margin_v = line.eff_margin_t
  local a = style.align
  if a % 3 == 1 then line.halign = "left"
  elseif a % 3 == 2 then line.halign = "center"
  else line.halign = "right" end
  if a > 6 then line.valign = "top"
  elseif a > 3 then line.valign = "middle"
  else line.valign = "bottom" end
  if line.halign == "left" then
    line.left = line.eff_margin_l
    line.center = line.left + line.width / 2
    line.right = line.left + line.width
    line.x = line.left
  elseif line.halign == "center" then
    line.left = (meta.res_x - line.eff_margin_l - line.eff_margin_r - line.width) / 2 + line.eff_margin_l
    line.center = line.left + line.width / 2
    line.right = line.left + line.width
    line.x = line.center
  else
    line.left = meta.res_x - line.eff_margin_r - line.width
    line.center = line.left + line.width / 2
    line.right = line.left + line.width
    line.x = line.right
  end
  line.hcenter = line.center
  if line.valign == "top" then
    line.top = line.eff_margin_t
    line.middle = line.top + line.height / 2
    line.bottom = line.top + line.height
    line.y = line.top
  elseif line.valign == "middle" then
    line.top = (meta.res_y - line.eff_margin_t - line.eff_margin_b - line.height) / 2 + line.eff_margin_t
    line.middle = line.top + line.height / 2
    line.bottom = line.top + line.height
    line.y = line.middle
  else
    line.bottom = meta.res_y - line.eff_margin_b
    line.top = line.bottom - line.height
    line.middle = line.top + line.height / 2
    line.y = line.bottom
  end
  line.vcenter = line.middle
  -- syllable positions
  local cx = line.left
  for _, s in ipairs(line.kara) do
    s.left = cx + s.prespace_width
    s.center = s.left + s.width / 2
    s.right = s.left + s.width
    s.top, s.middle, s.bottom = line.top, line.middle, line.bottom
    s.hcenter, s.vcenter = s.center, s.middle
    cx = cx + s.prespace_width + s.width + s.postspace_width
  end
end

function karaskel.preproc_line(subs, meta, styles, line)
  line.styleref = styles[line.style]
  if not line.styleref then
    aegisub.log(2, "WARNING: Style not found: " .. tostring(line.style) .. "\n")
    line.styleref = styles[1] or styles["Default"]
  end
  karaskel.preproc_line_text(meta, styles, line)
  karaskel.preproc_line_size(meta, styles, line)
  karaskel.preproc_line_pos(meta, styles, line)
end

function karaskel.fix_line_style(line, styles) line.styleref = styles[line.style] end

return karaskel
