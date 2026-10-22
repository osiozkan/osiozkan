-- AniSub karaskel.lua : API-compatible re-implementation of Aegisub's karaskel library.
include("utils.lua")
karaskel = {}

function karaskel.collect_head(subs, generate_furigana)
  local meta = { res_x = 0, res_y = 0 }
  local styles = { n = 0 }
  local have_x, have_y = false, false
  for i = 1, #subs do
    local l = subs[i]
    if l.class == "info" then
      local k = l.key:lower()
      if k == "playresx" then meta.res_x = tonumber(l.value) or 0; have_x = true
      elseif k == "playresy" then meta.res_y = tonumber(l.value) or 0; have_y = true
      elseif k == "wrapstyle" then meta.wrap_style = tonumber(l.value)
      elseif k == "scaledborderandshadow" then meta.scaled_border_and_shadow = (l.value:lower() == "yes")
      elseif k == "scripttype" then meta.scripttype = l.value
      end
    elseif l.class == "style" then
      l.margin_v = l.margin_t
      styles.n = styles.n + 1
      styles[styles.n] = l
      styles[l.name] = l
    end
  end
  if meta.res_x == 0 and meta.res_y == 0 then
    meta.res_x, meta.res_y = 384, 288
  elseif meta.res_x == 0 then
    meta.res_x = meta.res_y == 1024 and 1280 or meta.res_y / 3 * 4
  elseif meta.res_y == 0 then
    meta.res_y = meta.res_x == 1280 and 1024 or meta.res_x / 4 * 3
  end
  return meta, styles
end

local function strip_tags(text) return (text:gsub("{[^}]*}", "")) end

function karaskel.parse_syllables(line)
  local text = line.text
  local kara = { n = 0 }
  local cur
  local t = 0
  local function newsyl(dur, tag)
    cur = { text = "", duration = dur * 10, tag = tag, start_time = t, i = kara.n + 1 }
    t = t + cur.duration
    cur.end_time = t
    kara.n = kara.n + 1
    kara[kara.n] = cur
  end
  local pos = 1
  while pos <= #text do
    local bs, be = text:find("{[^}]*}", pos)
    if not bs then
      if cur == nil then newsyl(0, "k") end
      cur.text = cur.text .. text:sub(pos)
      break
    end
    if bs > pos then
      if cur == nil then newsyl(0, "k") end
      cur.text = cur.text .. text:sub(pos, bs - 1)
    end
    local block = text:sub(bs + 1, be - 1)
    if block:find("\\[kK][fot]?%-?%d") then
      -- walk the block: every k tag starts a new syllable; other tags attach to the syllable
      local rest, p = "", 1
      local first = true
      while true do
        local ks, ke, ktag, kval = block:find("\\([kK][fot]?)(%-?%d+%.?%d*)", p)
        if not ks then
          rest = rest .. block:sub(p)
          break
        end
        rest = rest .. block:sub(p, ks - 1)
        if not first or cur ~= nil or true then
          if not first then
            if rest ~= "" then cur.text = cur.text .. "{" .. rest .. "}" end
            rest = ""
          end
        end
        newsyl(tonumber(kval), ktag)
        first = false
        p = ke + 1
      end
      if rest ~= "" then cur.text = cur.text .. "{" .. rest .. "}" end
    else
      if cur == nil then newsyl(0, "k") end
      cur.text = cur.text .. text:sub(bs, be)
    end
    pos = be + 1
  end
  return kara
end

function karaskel.preproc_line_text(meta, styles, line)
  line.text_stripped = strip_tags(line.text)
  line.duration = line.end_time - line.start_time
  line.kara = karaskel.parse_syllables(line)
  for _, s in ipairs(line.kara) do
    s.text_stripped = strip_tags(s.text)
    s.prespace = s.text_stripped:match("^%s*")
    s.postspace = s.text_stripped:match("%s*$")
    if #s.prespace == #s.text_stripped then s.postspace = "" end
    s.text_spacestripped = s.text_stripped:sub(#s.prespace + 1, #s.text_stripped - #s.postspace)
    s.style = line.styleref
    s.line = line
  end
end

function karaskel.preproc_line_size(meta, styles, line)
  line.width, line.height, line.descent, line.extlead = aegisub.text_extents(line.styleref, line.text_stripped)
  local left = 0
  for _, s in ipairs(line.kara) do
    s.width, s.height, s.descent, s.extlead = aegisub.text_extents(line.styleref, s.text_spacestripped)
    s.prespace_width = (aegisub.text_extents(line.styleref, s.prespace))
    s.postspace_width = (aegisub.text_extents(line.styleref, s.postspace))
    s.rel_left = left + s.prespace_width
    left = left + s.prespace_width + s.width + s.postspace_width
  end
end

function karaskel.preproc_line_pos(meta, styles, line)
  local st = line.styleref
  line.margin_v = (line.margin_t ~= 0) and line.margin_t or st.margin_t
  line.margin_l = (line.margin_l ~= 0) and line.margin_l or st.margin_l
  line.margin_r = (line.margin_r ~= 0) and line.margin_r or st.margin_r
  line.margin_t = line.margin_v
  line.margin_b = (line.margin_b ~= 0) and line.margin_b or st.margin_b
  local al = st.align
  line.halign = (al % 3 == 1) and "left" or (al % 3 == 2) and "center" or "right"
  line.valign = (al > 6) and "top" or (al > 3) and "middle" or "bottom"
  if line.halign == "left" then
    line.left = line.margin_l
    line.x = line.left
  elseif line.halign == "center" then
    line.left = (meta.res_x - line.margin_l - line.margin_r - line.width) / 2 + line.margin_l
    line.x = line.left + line.width / 2
  else
    line.left = meta.res_x - line.margin_r - line.width
    line.x = line.left + line.width
  end
  line.center = line.left + line.width / 2
  line.right = line.left + line.width
  if line.valign == "top" then
    line.top = line.margin_v
    line.middle = line.top + line.height / 2
    line.bottom = line.top + line.height
    line.y = line.top
  elseif line.valign == "middle" then
    line.top = (meta.res_y - line.height) / 2
    line.middle = line.top + line.height / 2
    line.bottom = line.top + line.height
    line.y = line.middle
  else
    line.bottom = meta.res_y - line.margin_v
    line.middle = line.bottom - line.height / 2
    line.top = line.bottom - line.height
    line.y = line.bottom
  end
  for _, s in ipairs(line.kara) do
    s.left = line.left + s.rel_left
    s.center = s.left + s.width / 2
    s.right = s.left + s.width
    s.top, s.middle, s.bottom = line.top, line.middle, line.bottom
    s.x = (line.halign == "left") and s.left or (line.halign == "center") and s.center or s.right
    s.y = line.y
  end
end

function karaskel.preproc_line(subs, meta, styles, line)
  line.styleref = styles[line.style]
  if not line.styleref then
    aegisub.log(1, "WARNING: Style not found: " .. tostring(line.style) .. "\n")
    line.styleref = styles["Default"] or styles[1]
  end
  karaskel.preproc_line_text(meta, styles, line)
  karaskel.preproc_line_size(meta, styles, line)
  karaskel.preproc_line_pos(meta, styles, line)
end

function karaskel.preproc_lines(subs, meta, styles, lines)
  for _, l in ipairs(lines) do karaskel.preproc_line(subs, meta, styles, l) end
end
