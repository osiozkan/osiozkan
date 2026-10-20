-- karaskel.lua: line pre-processing for karaoke/typesetting scripts (Aegisub-compatible subset).
local util = include and include("utils.lua") or require("utils")
local karaskel = {}

function karaskel.collect_head(subs, generate_furigana)
  local meta = { res_x = 0, res_y = 0, wrap_style = 0, scale_border = true, video_x_correct_factor = 1.0 }
  local styles = { n = 0 }
  local first_style_line
  for i = 1, #subs do
    local l = subs[i]
    if l.class == "info" then
      local k = (l.key or ""):lower()
      if k == "playresx" then meta.res_x = math.floor(tonumber(l.value) or 0)
      elseif k == "playresy" then meta.res_y = math.floor(tonumber(l.value) or 0)
      elseif k == "wrapstyle" then meta.wrap_style = tonumber(l.value) or 0
      elseif k == "scaledborderandshadow" then meta.scale_border = (l.value or ""):lower() == "yes" end
    elseif l.class == "style" then
      first_style_line = first_style_line or i
      styles.n = styles.n + 1
      styles[styles.n] = l
      styles[l.name] = l
      l.margin_v = l.margin_t
      l.width, l.height, l.descent, l.extlead = aegisub.text_extents(l, "Mi")
    elseif l.class == "dialogue" then
      break
    end
  end
  if meta.res_x == 0 and meta.res_y == 0 then meta.res_x, meta.res_y = 384, 288
  elseif meta.res_y == 0 then meta.res_y = (meta.res_x == 1280) and 720 or math.floor(meta.res_x * 3 / 4)
  elseif meta.res_x == 0 then meta.res_x = (meta.res_y == 1024) and 1280 or math.floor(meta.res_y * 4 / 3) end
  meta.res_x, meta.res_y = meta.res_x, meta.res_y
  return meta, styles
end

local function strip_tags(s) return (s:gsub("{[^}]*}", "")) end
karaskel.strip_tags = strip_tags

-- Split into karaoke syllables on \k, \kf, \ko, \K.
function karaskel.parse_kara(line)
  local kara = { n = 0 }
  local text = line.text
  local cur = { start_time = 0, duration = 0, tag = "", text = "", i = 0 }
  kara[0] = cur
  local pos, t = 1, 0
  local pending_dur, pending_tag
  local function finish()
    cur.end_time = cur.start_time + cur.duration
    cur.text_stripped = strip_tags(cur.text)
    cur.text_spacestripped = cur.text_stripped:gsub("^%s+", ""):gsub("%s+$", "")
    cur.prespace = #cur.text_stripped:match("^%s*")
    cur.postspace = #cur.text_stripped:match("%s*$")
    if cur.text_spacestripped == "" then cur.prespace, cur.postspace = 0, 0 end
  end
  while pos <= #text do
    local s, e = text:find("{[^}]*}", pos)
    if not s then
      cur.text = cur.text .. text:sub(pos)
      break
    end
    cur.text = cur.text .. text:sub(pos, s - 1)
    local block = text:sub(s + 1, e - 1)
    -- find first karaoke tag in block
    local ktag, kdur = block:match("\\([kK][fo]?)(%-?%d+%.?%d*)")
    if ktag then
      finish()
      if cur.i == 0 and cur.text == "" and cur.duration == 0 then
        -- leading empty syllable stays at index 0
      end
      t = t + 0
      local new = { i = cur.i + 1, tag = ktag:lower() == "k" and "k" or ktag:lower(), start_time = cur.end_time, duration = math.floor(tonumber(kdur) * 10 + 0.5), text = "" }
      if ktag == "K" then new.tag = "kf" end
      -- preserve other tags in the block (minus the \k tag) as part of the syllable text
      local rest = block:gsub("\\[kK][fo]?%-?%d+%.?%d*", "", 1)
      if rest ~= "" then new.text = "{" .. rest .. "}" end
      kara.n = kara.n + 1
      kara[kara.n] = new
      cur = new
    else
      cur.text = cur.text .. text:sub(s, e)
    end
    pos = e + 1
  end
  finish()
  return kara
end

function karaskel.preproc_line_text(meta, styles, line)
  line.styleref = styles[line.style] or styles["Default"] or styles[1]
  line.text_stripped = strip_tags(line.text)
  line.duration = line.end_time - line.start_time
  line.kara = karaskel.parse_kara(line)
  -- sum of durations is only informational; keep syllable times relative to line start
end

function karaskel.preproc_line_size(meta, styles, line)
  local st = line.styleref or styles[line.style] or styles[1]
  line.width, line.height, line.descent, line.extlead = aegisub.text_extents(st, line.text_stripped)
  for i = 0, line.kara.n do
    local syl = line.kara[i]
    syl.width, syl.height, syl.descent, syl.extlead = aegisub.text_extents(st, syl.text_stripped)
    syl.prespace_width = aegisub.text_extents(st, string.rep(" ", syl.prespace))
    syl.postspace_width = aegisub.text_extents(st, string.rep(" ", syl.postspace))
  end
  local acc = 0
  for i = 0, line.kara.n do
    local syl = line.kara[i]
    syl.left = acc + (syl.prespace_width or 0)
    syl.center = syl.left + (syl.width - (syl.prespace_width or 0) - (syl.postspace_width or 0)) / 2
    syl.right = acc + syl.width - (syl.postspace_width or 0)
    acc = acc + syl.width
  end
end

function karaskel.preproc_line_pos(meta, styles, line)
  local st = line.styleref or styles[line.style] or styles[1]
  line.margin_l = (line.margin_l and line.margin_l > 0) and line.margin_l or st.margin_l
  line.margin_r = (line.margin_r and line.margin_r > 0) and line.margin_r or st.margin_r
  line.margin_v = (line.margin_t and line.margin_t > 0) and line.margin_t or st.margin_t
  local align = tonumber(line.text:match("\\an(%d)")) or st.align
  line.halign = (align - 1) % 3          -- 0 left, 1 center, 2 right
  line.valign = math.floor((align - 1) / 3) -- 0 bottom, 1 middle, 2 top
  local w, h = line.width, line.height
  if line.halign == 0 then
    line.left = line.margin_l; line.center = line.left + w / 2; line.right = line.left + w
  elseif line.halign == 1 then
    line.left = (meta.res_x - line.margin_l - line.margin_r - w) / 2 + line.margin_l
    line.center = line.left + w / 2; line.right = line.left + w
  else
    line.right = meta.res_x - line.margin_r; line.center = line.right - w / 2; line.left = line.right - w
  end
  if line.valign == 0 then
    line.bottom = meta.res_y - line.margin_v; line.middle = line.bottom - h / 2; line.top = line.bottom - h
  elseif line.valign == 2 then
    line.top = line.margin_v; line.middle = line.top + h / 2; line.bottom = line.top + h
  else
    line.middle = meta.res_y / 2; line.top = line.middle - h / 2; line.bottom = line.middle + h / 2
  end
  line.x = (line.halign == 0) and line.left or (line.halign == 1) and line.center or line.right
  line.y = (line.valign == 0) and line.bottom or (line.valign == 1) and line.middle or line.top
  -- vertical-text variants (width/height swapped)
  line.vleft, line.vcenter, line.vright = line.left, line.center, line.right
  line.vtop, line.vmiddle, line.vbottom = line.top, line.middle, line.bottom
  for i = 0, line.kara.n do
    local syl = line.kara[i]
    syl.left = syl.left + line.left
    syl.center = syl.center + line.left
    syl.right = syl.right + line.left
  end
end

function karaskel.preproc_line(subs, meta, styles, line)
  karaskel.preproc_line_text(meta, styles, line)
  karaskel.preproc_line_size(meta, styles, line)
  karaskel.preproc_line_pos(meta, styles, line)
end

function karaskel.fix_line_overlaps() end

_G.karaskel = karaskel
return karaskel
