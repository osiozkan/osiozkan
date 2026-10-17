-- karaskel.lua: Aegisub karaoke skeleton (compatible re-implementation)
require "utils"
local unicode = require "unicode"
local karaskel = {}

function karaskel.collect_head(subs, generate_furigana)
  local meta = { res_x = 0, res_y = 0 }
  local styles = { n = 0 }
  local first_style_line = nil
  for i = 1, #subs do
    local l = subs[i]
    if l.class == "style" then
      if not first_style_line then first_style_line = i end
      styles.n = styles.n + 1
      styles[styles.n] = l
      styles[l.name] = l
      l.margin_v = l.margin_t
    elseif l.class == "info" then
      local k = l.key:lower()
      meta[k] = l.value
      if k == "playresx" then meta.res_x = math.floor(tonumber(l.value) or 0)
      elseif k == "playresy" then meta.res_y = math.floor(tonumber(l.value) or 0) end
    end
  end
  if meta.res_x == 0 and meta.res_y == 0 then
    meta.res_x = 384; meta.res_y = 288
  elseif meta.res_x == 0 then
    meta.res_x = math.floor(meta.res_y / 3 * 4)
  elseif meta.res_y == 0 then
    meta.res_y = math.floor(meta.res_x * 3 / 4)
  end
  meta.video_x_correct_factor = 1

  if generate_furigana then
    local n = styles.n
    for i = 1, n do
      local s = aegisub.util.copy(styles[i])
      s.name = s.name .. "-furigana"
      s.fontsize = s.fontsize / 2
      styles.n = styles.n + 1
      styles[styles.n] = s
      styles[s.name] = s
      styles[i].furistyle = s
    end
  end
  return meta, styles
end


-- Split the text of a line into syllables according to \k tags.
local function parse_syllables(line)
  local text = line.text
  local kara = { n = 0 }
  local pos, cur = 1, nil
  local pre = ""
  local function newsyl(tag, dur, extra)
    kara.n = kara.n + 1
    cur = { i = kara.n, tag = tag, duration = dur * 10, text = extra or "", inline_fx = "" }
    kara[kara.n] = cur
  end
  while pos <= #text do
    local s, e = text:find("%b{}", pos)
    if not s then
      local rest = text:sub(pos)
      if cur then cur.text = cur.text .. rest else pre = pre .. rest end
      break
    end
    local plain = text:sub(pos, s - 1)
    if #plain > 0 then
      if cur then cur.text = cur.text .. plain else pre = pre .. plain end
    end
    local block = text:sub(s + 1, e - 1)
    local ktag, kdur
    -- find first \k-family tag
    local ks, ke, kt, kd = block:find("\\([kK][fo]?)(%-?%d+%.?%d*)")
    if ks then
      ktag = kt:lower()
      if kt == "K" then ktag = "kf" end
      kdur = tonumber(kd)
      local rest = block:sub(1, ks - 1) .. block:sub(ke + 1)
      local fx = rest:match("\\%-([%w_]+)")
      rest = rest:gsub("\\%-[%w_]+", "")
      if cur == nil and #pre > 0 then
        -- text before first \k becomes a zero-duration syllable
        kara.n = kara.n + 1
        kara[kara.n] = { i = kara.n, tag = "k", duration = 0, text = pre, inline_fx = "" }
        pre = ""
      end
      newsyl(ktag, kdur, rest ~= "" and ("{" .. rest .. "}") or "")
      if fx then cur.inline_fx = fx end
    else
      if cur then cur.text = cur.text .. "{" .. block .. "}" else pre = pre .. "{" .. block .. "}" end
    end
    pos = e + 1
  end
  if kara.n == 0 and #pre > 0 then
    kara.n = 1
    kara[1] = { i = 1, tag = "k", duration = 0, text = pre, inline_fx = "" }
  elseif kara.n > 0 and #pre > 0 and not kara[1].text:find(pre, 1, true) then
    table.insert(kara, 1, { tag = "k", duration = 0, text = pre, inline_fx = "" })
    kara.n = kara.n + 1
    for i = 1, kara.n do kara[i].i = i end
  end
  local t = 0
  for i = 1, kara.n do
    local s = kara[i]
    s.start_time = t
    t = t + s.duration
    s.end_time = t
    s.text_stripped = s.text:gsub("%b{}", "")
    s.text_spacestripped = s.text_stripped:gsub("^%s+", ""):gsub("%s+$", "")
    s.prespace = #s.text_stripped:match("^%s*")
    s.postspace = #s.text_stripped:match("%s*$")
    if s.text_spacestripped == "" then s.prespace = #s.text_stripped; s.postspace = 0 end
    s.kdur = s.duration / 10
  end
  return kara
end
karaskel.parse_syllables = parse_syllables

function karaskel.preproc_line_text(meta, styles, line)
  line.styleref = styles[line.style]
  if not line.styleref then
    aegisub.debug.out(1, "WARNING: style not found: " .. tostring(line.style) .. "\n")
    line.styleref = styles["Default"] or styles[1]
  end
  line.text_stripped = line.text:gsub("%b{}", "")
  line.duration = line.end_time - line.start_time
  line.kara = parse_syllables(line)
  -- strip karaoke-related whitespace into per-syllable info is done in parse_syllables
end

local function ext(style, text)
  return aegisub.text_extents(style, text)
end

function karaskel.preproc_line_size(meta, styles, line)
  local style = line.styleref
  line.width, line.height, line.descent, line.extlead = ext(style, line.text_stripped)
  for i = 1, line.kara.n do
    local s = line.kara[i]
    s.width, s.height, s.descent, s.extlead = ext(style, s.text_spacestripped)
    local pre = s.text_stripped:match("^%s*")
    local post = s.text_spacestripped == "" and "" or s.text_stripped:match("%s*$")
    s.prespace = (#pre > 0) and (ext(style, pre)) or 0
    s.postspace = (#post > 0) and (ext(style, post)) or 0
    if s.text_spacestripped == "" then s.width = 0; s.prespace = (ext(style, s.text_stripped)) end
  end
  -- recompute line width as sum of syllables when it matters for centring
end

function karaskel.preproc_line_pos(meta, styles, line)
  local style = line.styleref
  line.margin_l = (line.margin_l and line.margin_l > 0) and line.margin_l or style.margin_l
  line.margin_r = (line.margin_r and line.margin_r > 0) and line.margin_r or style.margin_r
  line.margin_t = (line.margin_t and line.margin_t > 0) and line.margin_t or style.margin_t
  line.margin_b = (line.margin_b and line.margin_b > 0) and line.margin_b or style.margin_b
  line.margin_v = line.margin_t

  local a = style.align
  local h = (a - 1) % 3
  line.halign = h == 0 and "left" or (h == 1 and "center" or "right")
  line.valign = a > 6 and "top" or (a > 3 and "middle" or "bottom")

  if line.halign == "left" then
    line.left = line.margin_l
    line.center = line.left + line.width / 2
    line.right = line.left + line.width
    line.x = line.left
  elseif line.halign == "center" then
    line.left = (meta.res_x - line.margin_l - line.margin_r - line.width) / 2 + line.margin_l
    line.center = line.left + line.width / 2
    line.right = line.left + line.width
    line.x = line.center
  else
    line.left = meta.res_x - line.margin_r - line.width
    line.center = line.left + line.width / 2
    line.right = line.left + line.width
    line.x = line.right
  end

  if line.valign == "top" then
    line.top = line.margin_t
    line.middle = line.top + line.height / 2
    line.bottom = line.top + line.height
    line.y = line.top
  elseif line.valign == "middle" then
    line.top = (meta.res_y - line.height) / 2
    line.middle = line.top + line.height / 2
    line.bottom = line.top + line.height
    line.y = line.middle
  else
    line.bottom = meta.res_y - line.margin_b
    line.middle = line.bottom - line.height / 2
    line.top = line.bottom - line.height
    line.y = line.bottom
  end

  local x = line.left
  for i = 1, line.kara.n do
    local s = line.kara[i]
    s.left = x + s.prespace
    s.center = s.left + s.width / 2
    s.right = s.left + s.width
    s.top, s.middle, s.bottom = line.top, line.middle, line.bottom
    x = s.right + s.postspace
  end
end

function karaskel.preproc_line(subs, meta, styles, line)
  karaskel.preproc_line_text(meta, styles, line)
  karaskel.preproc_line_size(meta, styles, line)
  karaskel.preproc_line_pos(meta, styles, line)
end

-- Convenience: iterate dialogue lines with full preprocessing
function karaskel.process(subs)
  local meta, styles = karaskel.collect_head(subs, false)
  for i = 1, #subs do
    local l = subs[i]
    if l.class == "dialogue" then
      karaskel.preproc_line(subs, meta, styles, l)
      subs[i] = l
    end
  end
end

return karaskel
