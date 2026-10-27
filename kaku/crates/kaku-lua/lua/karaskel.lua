-- kaku karaskel.lua: Aegisub-compatible karaoke skeleton (clean-room implementation)
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
      l.id = styles.n
      styles[l.name] = l
      if generate_furigana and not l.name:match("%-furigana$") then
        local f = util.copy(l)
        f.name = l.name .. "-furigana"
        f.fontsize = math.floor(l.fontsize / 2 + 0.5)
        styles.n = styles.n + 1
        styles[styles.n] = f
        f.id = styles.n
        styles[f.name] = f
      end
    elseif l.class == "info" then
      local k = l.key:lower()
      meta[k] = l.value
      if k == "playresx" then meta.res_x = math.floor(tonumber(l.value) or 0)
      elseif k == "playresy" then meta.res_y = math.floor(tonumber(l.value) or 0) end
    end
  end
  if meta.res_x == 0 and meta.res_y == 0 then meta.res_x = 384; meta.res_y = 288
  elseif meta.res_x == 0 then meta.res_x = math.floor(meta.res_y / 3 * 4)
  elseif meta.res_y == 0 then meta.res_y = math.floor(meta.res_x * 3 / 4) end
  meta.video_x_correct_factor = 1
  return meta, styles
end

function karaskel.use_furigana(subs) return false end

local function strip_tags(s) return (s:gsub("%b{}", "")) end

-- Parse \k tags into line.kara (array with .n)
function karaskel.parse_syls(line, styles)
  local text = line.text
  local kara = { n = 0 }
  local cur = nil
  local pos, len = 1, #text
  local function newsyl(dur, tag)
    local syl = { duration = dur * 10, tag = tag, text = "", tags = "", style = line.styleref, line = line, i = kara.n + 1 }
    kara.n = kara.n + 1
    kara[kara.n] = syl
    return syl
  end
  cur = newsyl(0, "k")
  local started = false
  while pos <= len do
    local s, e = text:find("%b{}", pos)
    if s == pos then
      local block = text:sub(s + 1, e - 1)
      local ktag, kdur
      for tag, dur in block:gmatch("\\([kK][fo]?)(%-?[%d%.]+)") do ktag, kdur = tag, tonumber(dur) end
      if ktag then
        if started or cur.text ~= "" then cur = newsyl(kdur, ktag:lower() == "k" and ktag or ktag) else cur.duration = kdur * 10; cur.tag = ktag end
        started = true
        local rest = block:gsub("\\[kK][fo]?%-?[%d%.]+", "")
        cur.tags = rest
      else
        cur.text = cur.text .. text:sub(s, e)
      end
      pos = e + 1
    else
      local nxt = s or (len + 1)
      cur.text = cur.text .. text:sub(pos, nxt - 1)
      pos = nxt
    end
  end
  local t = 0
  for i = 1, kara.n do
    local syl = kara[i]
    syl.start_time = t
    syl.end_time = t + syl.duration
    t = syl.end_time
    syl.text_stripped = strip_tags(syl.text)
    syl.text_spacestripped = syl.text_stripped:gsub("^%s+", ""):gsub("%s+$", "")
    syl.prespace = #(syl.text_stripped:match("^%s*"))
    syl.postspace = #(syl.text_stripped:match("%s*$"))
    syl.furi = { n = 0 }
  end
  -- drop an empty leading syllable produced by text starting with a tag block
  if kara.n > 1 and kara[1].text == "" and kara[1].duration == 0 then
    table.remove(kara, 1)
    kara.n = kara.n - 1
    for i = 1, kara.n do kara[i].i = i end
  end
  return kara
end

function karaskel.preproc_line_text(meta, styles, line)
  line.styleref = styles[line.style] or styles["Default"] or styles[1]
  line.text_stripped = strip_tags(line.text)
  line.duration = line.end_time - line.start_time
  line.kara = karaskel.parse_syls(line, styles)
  line.furi = { n = 0 }
end

function karaskel.preproc_line_size(meta, styles, line)
  local style = line.styleref
  local w, h, d, e = aegisub.text_extents(style, line.text_stripped)
  line.width, line.height, line.descent, line.extlead = w, h, d, e
  for i = 1, line.kara.n do
    local syl = line.kara[i]
    syl.width, syl.height, syl.descent, syl.extlead = aegisub.text_extents(style, syl.text_stripped)
    local sw = aegisub.text_extents(style, syl.text_spacestripped)
    syl.prespacewidth = (syl.prespace > 0) and aegisub.text_extents(style, syl.text_stripped:sub(1, syl.prespace)) or 0
    syl.postspacewidth = (syl.postspace > 0) and aegisub.text_extents(style, syl.text_stripped:sub(-syl.postspace)) or 0
    syl.width_nospace = sw
  end
end

function karaskel.preproc_line_pos(meta, styles, line)
  local style = line.styleref
  line.margin_v = (line.margin_t ~= 0) and line.margin_t or style.margin_t
  line.eff_margin_l = (line.margin_l ~= 0) and line.margin_l or style.margin_l
  line.eff_margin_r = (line.margin_r ~= 0) and line.margin_r or style.margin_r
  line.eff_margin_t = line.margin_v
  line.eff_margin_b = line.margin_v
  line.eff_margin_v = line.margin_v
  local a = style.align
  line.halign = (a - 1) % 3 == 0 and "left" or ((a - 1) % 3 == 1 and "center" or "right")
  line.valign = (a <= 3) and "bottom" or (a >= 7 and "top" or "middle")
  if line.halign == "left" then
    line.left = line.eff_margin_l; line.center = line.left + line.width / 2; line.right = line.left + line.width
  elseif line.halign == "right" then
    line.right = meta.res_x - line.eff_margin_r; line.center = line.right - line.width / 2; line.left = line.right - line.width
  else
    line.left = (meta.res_x - line.eff_margin_l - line.eff_margin_r - line.width) / 2 + line.eff_margin_l
    line.center = line.left + line.width / 2; line.right = line.left + line.width
  end
  if line.valign == "top" then
    line.top = line.eff_margin_t; line.middle = line.top + line.height / 2; line.bottom = line.top + line.height
  elseif line.valign == "bottom" then
    line.bottom = meta.res_y - line.eff_margin_b; line.middle = line.bottom - line.height / 2; line.top = line.bottom - line.height
  else
    line.top = (meta.res_y - line.height) / 2; line.middle = meta.res_y / 2; line.bottom = line.top + line.height
  end
  line.x = (line.halign == "left") and line.left or ((line.halign == "right") and line.right or line.center)
  line.y = (line.valign == "top") and line.top or ((line.valign == "bottom") and line.bottom or line.middle)
  -- syllable positions
  local cx = line.left
  for i = 1, line.kara.n do
    local syl = line.kara[i]
    syl.left = cx + syl.prespacewidth
    syl.right = cx + syl.width - syl.postspacewidth
    syl.center = (syl.left + syl.right) / 2
    syl.top, syl.middle, syl.bottom = line.top, line.middle, line.bottom
    syl.x = (line.halign == "left") and syl.left or ((line.halign == "right") and syl.right or syl.center)
    syl.y = line.y
    cx = cx + syl.width
  end
end

function karaskel.preproc_line(subs, meta, styles, line)
  karaskel.preproc_line_text(meta, styles, line)
  karaskel.preproc_line_size(meta, styles, line)
  karaskel.preproc_line_pos(meta, styles, line)
end

function karaskel.fix_line(line) return line end

return karaskel
