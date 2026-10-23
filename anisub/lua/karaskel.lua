-- karaskel.lua (Aegisub-compatible subset): style/line preprocessing and karaoke parsing.
karaskel = karaskel or {}

function karaskel.collect_head(subs, generate_furigana)
  local meta = {res_x = 0, res_y = 0}
  local styles = {n = 0}
  local first_style = nil
  for i = 1, #subs do
    local l = subs[i]
    if l.class == "info" then
      local k = l.key:lower()
      if k == "playresx" then meta.res_x = tonumber(l.value) or 0
      elseif k == "playresy" then meta.res_y = tonumber(l.value) or 0
      elseif k == "wrapstyle" then meta.wrap_style = tonumber(l.value) end
      meta[l.key] = l.value
    elseif l.class == "style" then
      styles.n = styles.n + 1
      styles[styles.n] = l
      styles[l.name] = l
      first_style = first_style or l
    end
  end
  if meta.res_x == 0 and meta.res_y == 0 then meta.res_x, meta.res_y = 384, 288
  elseif meta.res_x == 0 then meta.res_x = meta.res_y / 3 * 4
  elseif meta.res_y == 0 then meta.res_y = meta.res_x * 3 / 4 end
  local vx, vy = aegisub.video_size()
  meta.video_x_correct_factor = 1
  if vx and vy and vx > 0 and vy > 0 then
    meta.video_x_correct_factor = (vx / vy) / (meta.res_x / meta.res_y)
  end
  for i = 1, styles.n do
    local s = styles[i]
    s.margin_v = s.margin_t
    local w, h, d, e = aegisub.text_extents(s, "Mj")
    s.height, s.descent, s.extlead = h, d, e
  end
  return meta, styles
end

function karaskel.parse_karaoke_data(text)
  local syls = {}
  local time = 0
  local cur = {text = "", tags = "", duration = 0, tag = "k", start_time = 0}
  local pending = false
  local pos = 1
  local function flush()
    if pending then
      cur.end_time = cur.start_time + cur.duration
      cur.text_stripped = cur.text:gsub("{[^}]*}", "")
      cur.text_spaceless = cur.text_stripped:gsub("%s", "")
      cur.i = #syls + 1
      table.insert(syls, cur)
    end
  end
  while pos <= #text do
    local a, b, blk = text:find("^{([^}]*)}", pos)
    if a then
      local tag, dur = blk:match("\\([kK][fo]?)(%d+)")
      if tag then
        flush()
        cur = {text = "", tags = blk, duration = tonumber(dur) * 10, tag = tag:lower(), start_time = time, prefix = ""}
        time = time + cur.duration
        pending = true
      else
        cur.text = cur.text .. text:sub(a, b)
        if not pending then cur.pre = (cur.pre or "") .. text:sub(a, b) end
      end
      pos = b + 1
    else
      local nxt = text:find("{", pos, true) or (#text + 1)
      cur.text = cur.text .. text:sub(pos, nxt - 1)
      pos = nxt
    end
  end
  flush()
  syls.n = #syls
  return syls
end

function karaskel.preproc_line_text(meta, styles, line)
  line.styleref = styles[line.style] or styles[1]
  line.text_stripped = line.text:gsub("{[^}]*}", ""):gsub("\\[Nn]", " ")
  line.duration = line.end_time - line.start_time
  local w, h, d, e = aegisub.text_extents(line.styleref, line.text_stripped)
  line.width, line.height, line.descent, line.extlead = w, h, d, e
  if line.text:find("\\k[fo]?%d") then line.kara = karaskel.parse_karaoke_data(line.text) end
end

function karaskel.preproc_line_pos(meta, styles, line)
  local st = line.styleref
  local function eff(v, d) return v ~= 0 and v or d end
  line.eff_margin_l = eff(line.margin_l, st.margin_l)
  line.eff_margin_r = eff(line.margin_r, st.margin_r)
  line.eff_margin_t = eff(line.margin_t, st.margin_t)
  line.eff_margin_b = eff(line.margin_b, st.margin_b)
  line.margin_v = line.eff_margin_t
  local al = st.align
  line.halign = ({"left", "center", "right"})[(al - 1) % 3 + 1]
  line.valign = al > 6 and "top" or (al > 3 and "middle" or "bottom")
  line.left = line.eff_margin_l
  line.right = meta.res_x - line.eff_margin_r
  line.top = line.eff_margin_t
  line.bottom = meta.res_y - line.eff_margin_b
  line.center = (line.left + line.right) / 2
  line.middle = meta.res_y / 2
  line.x = line.halign == "left" and line.left or (line.halign == "right" and line.right or line.center)
  line.y = line.valign == "top" and line.top or (line.valign == "bottom" and line.bottom or line.middle)
  line.hcenter = line.center
  line.vcenter = line.middle
end

function karaskel.preproc_line(subs, meta, styles, line)
  karaskel.preproc_line_text(meta, styles, line)
  karaskel.preproc_line_pos(meta, styles, line)
  if line.kara then
    local x = line.left
    for _, s in ipairs(line.kara) do
      s.width = aegisub.text_extents(line.styleref, s.text_stripped)
      s.left = x; s.center = x + s.width / 2; s.right = x + s.width
      x = x + s.width
      s.height, s.top, s.middle, s.bottom = line.height, line.top, line.middle, line.bottom
      s.style, s.line = line.styleref, line
    end
  end
end

function karaskel.fix_line_start_time() end
