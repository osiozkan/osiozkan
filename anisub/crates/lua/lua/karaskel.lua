-- karaskel.lua: karaoke skeleton helpers (API-compatible subset of Aegisub's karaskel).
include("utils.lua")
include("unicode.lua")

karaskel = karaskel or {}

local function parse_k_tags(text)
  -- returns list of {duration(ms), tag, text}
  local out, pos = {}, 1
  local cur = { duration = 0, tag = "k", text = "", tags = "" }
  local have = false
  while pos <= #text do
    local s, e, block = text:find("^{(.-)}", pos)
    if s then
      local kt, kd
      for t, d in block:gmatch("\\([kK][fo]?)(%-?[%d%.]+)") do kt, kd = t, d end
      if kt then
        if have then table.insert(out, cur) end
        cur = { duration = math.floor(tonumber(kd) * 10 + 0.5), tag = kt:lower() == "k" and kt or kt, text = "", tags = block }
        if kt == "K" then cur.tag = "kf" end
        have = true
      else
        cur.tags = cur.tags .. block
      end
      pos = e + 1
    else
      local nxt = text:find("{", pos) or (#text + 1)
      cur.text = cur.text .. text:sub(pos, nxt - 1)
      have = true
      pos = nxt
    end
  end
  if have then table.insert(out, cur) end
  return out
end

function karaskel.collect_head(subs, generate_furigana)
  local meta = { res_x = 0, res_y = 0, video_x_correct_factor = 1 }
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
      if k == "playresx" then meta.res_x = tonumber(l.value) or 0
      elseif k == "playresy" then meta.res_y = tonumber(l.value) or 0
      elseif k == "wrapstyle" then meta.wrap_style = tonumber(l.value) or 0
      elseif k == "scaledborderandshadow" then meta.scaled_border_and_shadow = l.value:lower() == "yes"
      end
      meta[k] = l.value
    end
  end
  if meta.res_x == 0 and meta.res_y == 0 then meta.res_x, meta.res_y = 384, 288
  elseif meta.res_y == 0 then meta.res_y = meta.res_x == 1280 and 1024 or meta.res_x * 3 / 4
  elseif meta.res_x == 0 then meta.res_x = meta.res_y == 1024 and 1280 or meta.res_y * 4 / 3 end
  for i = 1, styles.n do karaskel.preproc_style(meta, styles[i]) end
  return meta, styles
end

function karaskel.preproc_style(meta, style)
  style.margin_v = style.margin_t
  local a = style.align
  style.halign = (a - 1) % 3 == 0 and "left" or ((a - 1) % 3 == 1 and "center" or "right")
  style.valign = a >= 7 and "top" or (a >= 4 and "middle" or "bottom")
  local w, h, d, e = aegisub.text_extents(style, "Mj")
  style.height, style.descent, style.extlead = h, d, e
  style.width = w
  return style
end

function karaskel.collect_styles(subs)
  local _, styles = karaskel.collect_head(subs)
  return styles
end

function karaskel.preproc_line_text(meta, styles, line)
  line.styleref = styles[line.style] or styles["Default"] or styles[1]
  line.text_stripped = line.text:gsub("{.-}", "")
  line.duration = line.end_time - line.start_time
  line.kara = parse_k_tags(line.text)
  local t = 0
  for i, s in ipairs(line.kara) do
    s.i = i
    s.start_time = t
    s.end_time = t + s.duration
    s.duration = s.duration
    s.text_stripped = s.text:gsub("{.-}", "")
    s.text_spacestripped = s.text_stripped:gsub("^[ \t]+", ""):gsub("[ \t]+$", "")
    s.furi = {}
    t = s.end_time
  end
  line.kara.n = #line.kara
end

function karaskel.preproc_line_size(meta, styles, line)
  local st = line.styleref
  line.width, line.height, line.descent, line.extlead = aegisub.text_extents(st, line.text_stripped)
  local pos = 0
  for _, s in ipairs(line.kara) do
    s.width, s.height, s.descent, s.extlead = aegisub.text_extents(st, s.text_stripped)
    s.prespacewidth = aegisub.text_extents(st, s.text_stripped:match("^[ \t]*") or "")
    s.postspacewidth = aegisub.text_extents(st, s.text_stripped:match("[ \t]*$") or "")
    s.left = pos
    s.center = pos + s.width / 2
    s.right = pos + s.width
    pos = pos + s.width
  end
end

function karaskel.preproc_line_pos(meta, styles, line)
  local st = line.styleref
  line.margin_l = line.margin_l ~= 0 and line.margin_l or st.margin_l
  line.margin_r = line.margin_r ~= 0 and line.margin_r or st.margin_r
  line.margin_v = line.margin_t ~= 0 and line.margin_t or st.margin_t
  line.eff_margin_l, line.eff_margin_r, line.eff_margin_v = line.margin_l, line.margin_r, line.margin_v
  line.halign, line.valign = st.halign, st.valign
  if line.halign == "left" then
    line.left = line.margin_l; line.center = line.left + line.width / 2; line.right = line.left + line.width
    line.x = line.left
  elseif line.halign == "center" then
    line.left = (meta.res_x - line.margin_l - line.margin_r - line.width) / 2 + line.margin_l
    line.center = line.left + line.width / 2; line.right = line.left + line.width
    line.x = line.center
  else
    line.right = meta.res_x - line.margin_r; line.center = line.right - line.width / 2
    line.left = line.right - line.width; line.x = line.right
  end
  if line.valign == "top" then
    line.top = line.margin_v; line.middle = line.top + line.height / 2
    line.bottom = line.top + line.height; line.y = line.top
  elseif line.valign == "middle" then
    line.top = (meta.res_y - line.height) / 2; line.middle = line.top + line.height / 2
    line.bottom = line.top + line.height; line.y = line.middle
  else
    line.bottom = meta.res_y - line.margin_v; line.middle = line.bottom - line.height / 2
    line.top = line.bottom - line.height; line.y = line.bottom
  end
  for _, s in ipairs(line.kara) do
    s.left = s.left + line.left
    s.center = s.center + line.left
    s.right = s.right + line.left
    s.top, s.middle, s.bottom = line.top, line.middle, line.bottom
    s.x = line.halign == "left" and s.left or (line.halign == "center" and s.center or s.right)
    s.y = line.y
  end
end

function karaskel.preproc_line(subs, meta, styles, line)
  karaskel.preproc_line_text(meta, styles, line)
  karaskel.preproc_line_size(meta, styles, line)
  karaskel.preproc_line_pos(meta, styles, line)
end

function karaskel.do_basic_text_stuff(subs, meta, styles, line)
  karaskel.preproc_line_text(meta, styles, line)
end
