-- Aegisub karaskel compatible subset (collect_head, preproc_line, kara syllable parsing)
-- NOTE: text metrics come from aegisub.text_extents, which is approximate in osisub 0.1.
local karaskel = {}

function karaskel.collect_head(subs, generate_furigana)
  local meta = { res_x = 0, res_y = 0, video_x_correct_factor = 1.0 }
  local styles = { n = 0 }
  for i = 1, #subs do
    local l = subs[i]
    if l.class == "info" then
      local k = l.key:lower()
      if k == "playresx" then meta.res_x = math.floor(tonumber(l.value) or 0)
      elseif k == "playresy" then meta.res_y = math.floor(tonumber(l.value) or 0)
      elseif k == "wrapstyle" then meta.wrap_style = tonumber(l.value)
      elseif k == "scaledborderandshadow" then meta.scaled_border_and_shadow = l.value:lower() == "yes"
      end
      meta[l.key] = l.value
    elseif l.class == "style" then
      styles.n = styles.n + 1
      styles[styles.n] = l
      styles[l.name] = l
      l.margin_v = l.margin_t
    elseif l.class == "dialogue" then
      break
    end
  end
  if meta.res_x == 0 and meta.res_y == 0 then meta.res_x, meta.res_y = 384, 288
  elseif meta.res_x == 0 then meta.res_x = math.floor(meta.res_y / 3 * 4)
  elseif meta.res_y == 0 then meta.res_y = math.floor(meta.res_x * 3 / 4) end
  return meta, styles
end

function karaskel.use_furigana(subs) return false end

-- Parse \k-style syllables: each syllable gets text, duration (ms), start_time, end_time (relative to line), tag
function karaskel.parse_syllables(subs, meta, styles, line)
  local kara = { n = 0 }
  local text = line.text
  local pos, t = 1, 0
  local cur = nil
  local function flush()
    if cur then
      cur.text_stripped = cur.text:gsub("{[^}]*}", "")
      cur.text_spacestripped = cur.text_stripped:gsub("^%s+", ""):gsub("%s+$", "")
      kara.n = kara.n + 1
      cur.i = kara.n
      kara[kara.n] = cur
      cur = nil
    end
  end
  -- text before the first \k block belongs to a zero-length syllable
  while pos <= #text do
    local s, e = text:find("{[^}]*}", pos)
    local chunk
    if s == pos then
      local blk = text:sub(s + 1, e - 1)
      local tag, dur = blk:match("\\([kK][fo]?)(%-?%d+)")
      if tag then
        flush()
        dur = tonumber(dur) * 10
        cur = { duration = dur, start_time = t, end_time = t + dur, tag = tag:lower(), text = "", tag_blocks = blk }
        t = t + dur
      elseif cur then
        cur.text = cur.text .. text:sub(s, e)
      else
        cur = { duration = 0, start_time = 0, end_time = 0, tag = "k", text = text:sub(s, e) }
      end
      pos = e + 1
    else
      local stop = (s or #text + 1) - 1
      chunk = text:sub(pos, stop)
      if not cur then cur = { duration = 0, start_time = 0, end_time = 0, tag = "k", text = "" } end
      cur.text = cur.text .. chunk
      pos = stop + 1
    end
  end
  flush()
  line.kara = kara
  return kara
end

function karaskel.preproc_line_text(meta, styles, line)
  line.text_stripped = line.text:gsub("{[^}]*}", "")
  line.duration = line.end_time - line.start_time
  line.styleref = styles[line.style] or styles["Default"] or styles[1]
end

function karaskel.preproc_line_size(meta, styles, line)
  local st = line.styleref
  line.width, line.height, line.descent, line.extlead = aegisub.text_extents(st, line.text_stripped)
  line.width = line.width * meta.video_x_correct_factor
end

function karaskel.preproc_line_pos(meta, styles, line)
  local st = line.styleref
  line.margin_v = line.margin_t
  line.eff_margin_l = (line.margin_l > 0) and line.margin_l or st.margin_l
  line.eff_margin_r = (line.margin_r > 0) and line.margin_r or st.margin_r
  line.eff_margin_t = (line.margin_t > 0) and line.margin_t or st.margin_t
  line.eff_margin_b = (line.margin_b > 0) and line.margin_b or st.margin_b
  line.eff_margin_v = line.eff_margin_t
  local al = st.align
  line.valign = al >= 7 and "top" or (al >= 4 and "middle" or "bottom")
  local h = al % 3
  line.halign = h == 1 and "left" or (h == 2 and "center" or "right")
  line.left = line.eff_margin_l
  line.right = meta.res_x - line.eff_margin_r
  line.center = math.floor((line.left + line.right) / 2 + 0.5)
  line.top = line.eff_margin_t
  line.bottom = meta.res_y - line.eff_margin_b
  line.middle = math.floor(meta.res_y / 2 + 0.5)
  if line.halign == "left" then line.x = line.left
  elseif line.halign == "center" then line.x = line.center
  else line.x = line.right end
  if line.valign == "top" then line.y = line.top
  elseif line.valign == "middle" then line.y = line.middle
  else line.y = line.bottom end
end

function karaskel.preproc_line(subs, meta, styles, line)
  karaskel.preproc_line_text(meta, styles, line)
  karaskel.preproc_line_size(meta, styles, line)
  karaskel.preproc_line_pos(meta, styles, line)
  if line.text:find("\\[kK][fo]?%-?%d") then karaskel.parse_syllables(subs, meta, styles, line) end
end

return karaskel
