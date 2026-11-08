-- Aegisub-compatible `karaskel` module (Kitsune reimplementation)
local karaskel = {}

local function strip_tags(t) return (t:gsub("{[^}]*}", "")) end

function karaskel.collect_head(subs, generate_furigana)
  local meta = { res_x = 0, res_y = 0 }
  local styles = { n = 0 }
  local wrap, scaled
  for i = 1, #subs do
    local l = subs[i]
    if l.class == "info" then
      local k = l.key:lower()
      if k == "playresx" then meta.res_x = math.floor(tonumber(l.value) or 0)
      elseif k == "playresy" then meta.res_y = math.floor(tonumber(l.value) or 0)
      elseif k == "wrapstyle" then meta.wrap_style = tonumber(l.value)
      elseif k == "scaledborderandshadow" then meta.scaled_border_and_shadow = l.value:lower() == "yes" end
    elseif l.class == "style" then
      styles.n = styles.n + 1
      styles[styles.n] = l
      styles[l.name] = l
      l.margin_v = l.margin_t
    end
  end
  if meta.res_x == 0 and meta.res_y == 0 then meta.res_x = 384; meta.res_y = 288
  elseif meta.res_x == 0 then meta.res_x = math.floor(meta.res_y / 3 * 4)
  elseif meta.res_y == 0 then meta.res_y = math.floor(meta.res_x * 3 / 4) end
  meta.video_x_correct_factor = 1
  return meta, styles
end

function karaskel.use_ass_style_defaults() end

function karaskel.parse_karaoke_data(line, meta, styles)
  local kara = { n = 0 }
  local text = line.text
  local pos, cur = 1, nil
  local start = 0
  local function new_syl(dur, tag)
    kara.n = kara.n + 1
    local s = { duration = dur * 10, start_time = start, end_time = start + dur * 10, tag = tag,
                text = "", text_stripped = "", i = kara.n, line = line }
    start = start + dur * 10
    kara[kara.n] = s
    return s
  end
  -- leading text before first \k belongs to syllable 0 (treated as zero-length syllable)
  cur = nil
  while pos <= #text do
    local bs, be = text:find("{[^}]*}", pos)
    if bs == pos then
      local block = text:sub(bs + 1, be - 1)
      local kt, kd = block:match("\\([kK][fo]?)(%d+)")
      if kt then
        cur = new_syl(tonumber(kd), kt)
        cur.text = text:sub(bs, be)
      else
        if cur then cur.text = cur.text .. text:sub(bs, be) end
      end
      pos = be + 1
    else
      local nxt = bs and bs - 1 or #text
      local chunk = text:sub(pos, nxt)
      if not cur then cur = new_syl(0, "k") end
      cur.text = cur.text .. chunk
      cur.text_stripped = cur.text_stripped .. chunk
      pos = nxt + 1
    end
  end
  for _, s in ipairs(kara) do
    s.text_spacestripped = s.text_stripped:gsub("^%s+", ""):gsub("%s+$", "")
    s.duration = s.duration
  end
  line.kara = kara
  line.kara.n = kara.n
end

function karaskel.preproc_line_text(meta, styles, line)
  line.text_stripped = strip_tags(line.text)
  local style = line.styleref
  local w, h, d, e = aegisub.text_extents(style, line.text_stripped)
  line.width, line.height, line.descent, line.extlead = w, h, d, e
end

function karaskel.preproc_line_pos(meta, styles, line)
  local style = line.styleref
  line.margin_v = line.margin_t
  line.eff_margin_l = line.margin_l > 0 and line.margin_l or style.margin_l
  line.eff_margin_r = line.margin_r > 0 and line.margin_r or style.margin_r
  line.eff_margin_t = line.margin_t > 0 and line.margin_t or style.margin_t
  line.eff_margin_b = line.eff_margin_t
  line.eff_margin_v = line.eff_margin_t
  local al = style.align
  local ov = line.text:match("{[^}]*\\an(%d)")
  if ov then al = tonumber(ov) end
  line.halign = ({ "left", "center", "right" })[(al - 1) % 3 + 1]
  line.valign = al <= 3 and "bottom" or (al <= 6 and "middle" or "top")
  if line.halign == "left" then line.left = line.eff_margin_l; line.center = line.left + line.width / 2; line.right = line.left + line.width
  elseif line.halign == "center" then line.left = (meta.res_x - line.eff_margin_l - line.eff_margin_r - line.width) / 2 + line.eff_margin_l
    line.center = line.left + line.width / 2; line.right = line.left + line.width
  else line.right = meta.res_x - line.eff_margin_r; line.center = line.right - line.width / 2; line.left = line.right - line.width end
  if line.valign == "bottom" then line.bottom = meta.res_y - line.eff_margin_b; line.middle = line.bottom - line.height / 2; line.top = line.bottom - line.height
  elseif line.valign == "middle" then line.middle = meta.res_y / 2; line.top = line.middle - line.height / 2; line.bottom = line.top + line.height
  else line.top = line.eff_margin_t; line.middle = line.top + line.height / 2; line.bottom = line.top + line.height end
  line.x = ({ left = line.left, center = line.center, right = line.right })[line.halign]
  line.y = ({ top = line.top, middle = line.middle, bottom = line.bottom })[line.valign]
end

function karaskel.preproc_line(subs, meta, styles, line)
  line.styleref = styles[line.style] or styles["Default"] or styles[1]
  line.duration = line.end_time - line.start_time
  karaskel.preproc_line_text(meta, styles, line)
  karaskel.preproc_line_pos(meta, styles, line)
  karaskel.parse_karaoke_data(line, meta, styles)
  for _, s in ipairs(line.kara) do
    local w, h, d = aegisub.text_extents(line.styleref, s.text_stripped)
    s.width, s.height, s.descent = w, h, d
    s.left, s.center, s.right = 0, 0, 0
  end
  -- syllable positions
  local x = 0
  for _, s in ipairs(line.kara) do
    s.left = x; s.center = x + s.width / 2; s.right = x + s.width; x = x + s.width
  end
end

function karaskel.fix_line_pos() end

return karaskel
