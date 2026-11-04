-- Yozora karaskel.lua: Aegisub `karaskel.lua` ile uyumlu karaoke iskeleti.
include("utils.lua")

karaskel = karaskel or {}

local function num(v) return tonumber(v) or 0 end

-- Başlık (Script Info + stiller) toplar.
function karaskel.collect_head(subs, generate_furigana)
  local meta = { res_x = 0, res_y = 0, wrap_style = 0, scaled_border_and_shadow = false, raw = {} }
  local styles = { n = 0 }
  for i = 1, #subs do
    local l = subs[i]
    if l.class == "info" then
      local k = l.key:lower()
      meta.raw[k] = l.value
      if k == "playresx" then meta.res_x = num(l.value)
      elseif k == "playresy" then meta.res_y = num(l.value)
      elseif k == "wrapstyle" then meta.wrap_style = num(l.value)
      elseif k == "scaledborderandshadow" then meta.scaled_border_and_shadow = (l.value:lower() == "yes")
      end
    elseif l.class == "style" then
      styles.n = styles.n + 1
      l.margin_v = l.margin_t
      styles[l.name] = l
      styles[styles.n] = l
    elseif l.class == "dialogue" then
      break
    end
  end
  if meta.res_x == 0 and meta.res_y == 0 then
    meta.res_x, meta.res_y = 384, 288
  elseif meta.res_x == 0 then
    meta.res_x = meta.res_y / 3 * 4
  elseif meta.res_y == 0 then
    meta.res_y = meta.res_x / 4 * 3
  end
  meta.video_x_correct_factor = 1
  for i = 1, styles.n do
    local s = styles[i]
    s.width, s.height, s.descent, s.extlead = aegisub.text_extents(s, "A")
    s.margin_v = s.margin_t
  end
  return meta, styles
end

-- Karaoke hecelerine ayır. Süreler centisaniye cinsindendir.
local function parse_syllables(line)
  local text = line.text
  local kara = { n = 0 }
  local cur = nil
  local pos = 1
  local function new_syl(dur, tag)
    cur = { duration = dur, tag = tag, text = "", text_stripped = "", style = line.styleref }
    kara.n = kara.n + 1
    cur.i = kara.n
    kara[kara.n] = cur
  end
  local time = 0
  local style_override = nil
  local function finish_time()
    if cur then
      cur.start_time = time
      cur.end_time = time + cur.duration * 10
      time = cur.end_time
    end
  end
  while pos <= #text do
    local s, e = text:find("%b{}", pos)
    if s == pos then
      local block = text:sub(s, e)
      local k_tag, k_dur = block:match("\\([kK][fo]?)(%d+)")
      if k_tag then
        finish_time()
        new_syl(tonumber(k_dur), k_tag == "K" and "kf" or k_tag)
        cur.text = block
      else
        if not cur then new_syl(0, "k") end
        cur.text = cur.text .. block
      end
      pos = e + 1
    else
      local stop = s and (s - 1) or #text
      local plain = text:sub(pos, stop)
      if not cur then new_syl(0, "k") end
      cur.text = cur.text .. plain
      cur.text_stripped = cur.text_stripped .. plain
      pos = stop + 1
    end
  end
  finish_time()
  -- Baştaki boş (0 süreli, metinsiz) hece olursa at.
  if kara.n > 0 and kara[1].duration == 0 and kara[1].text_stripped == "" and kara.n > 1 then
    table.remove(kara, 1)
    kara.n = kara.n - 1
    local t = 0
    for i, sy in ipairs(kara) do
      sy.i = i
      sy.start_time = t
      sy.end_time = t + sy.duration * 10
      t = sy.end_time
    end
  end
  return kara
end

function karaskel.preproc_line_text(meta, styles, line)
  line.text_stripped = line.text:gsub("%b{}", "")
  line.duration = line.end_time - line.start_time
  line.styleref = styles[line.style]
  if not line.styleref then
    aegisub.debug.out(2, "Stil bulunamadı: %s (Default kullanılıyor)\n", tostring(line.style))
    line.styleref = styles["Default"] or styles[1]
  end
  line.kara = parse_syllables(line)
  for i = 1, line.kara.n do
    local s = line.kara[i]
    s.style = line.styleref
  end
end

function karaskel.preproc_line_size(meta, styles, line)
  line.width, line.height, line.descent, line.extlead = aegisub.text_extents(line.styleref, line.text_stripped)
  for i = 1, line.kara.n do
    local syl = line.kara[i]
    syl.text_spacestripped = syl.text_stripped:gsub("^[ \t]+", ""):gsub("[ \t]+$", "")
    local lead = syl.text_stripped:match("^[ \t]*")
    local trail = syl.text_stripped:match("[ \t]*$")
    if #syl.text_spacestripped == 0 then trail = "" end
    syl.width, syl.height, syl.descent, syl.extlead = aegisub.text_extents(syl.style, syl.text_spacestripped)
    syl.prespace = (#lead > 0) and (aegisub.text_extents(syl.style, lead)) or 0
    syl.postspace = (#trail > 0) and (aegisub.text_extents(syl.style, trail)) or 0
  end
end

function karaskel.preproc_line_pos(meta, styles, line)
  local st = line.styleref
  line.margin_l = line.margin_l or 0
  line.margin_r = line.margin_r or 0
  line.margin_t = line.margin_t or 0
  line.margin_b = line.margin_b or line.margin_t
  line.eff_margin_l = line.margin_l > 0 and line.margin_l or st.margin_l
  line.eff_margin_r = line.margin_r > 0 and line.margin_r or st.margin_r
  line.eff_margin_t = line.margin_t > 0 and line.margin_t or st.margin_t
  line.eff_margin_b = line.margin_t > 0 and line.margin_t or st.margin_b
  line.eff_margin_v = line.eff_margin_t
  line.margin_v = line.margin_t

  local al = st.align
  if al % 3 == 1 then line.halign = "left" elseif al % 3 == 2 then line.halign = "center" else line.halign = "right" end
  if al > 6 then line.valign = "top" elseif al > 3 then line.valign = "middle" else line.valign = "bottom" end

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
    line.right = meta.res_x - line.eff_margin_r
    line.left = line.right - line.width
    line.center = line.left + line.width / 2
    line.x = line.right
  end

  if line.valign == "top" then
    line.top = line.eff_margin_t
    line.middle = line.top + line.height / 2
    line.bottom = line.top + line.height
    line.y = line.top
  elseif line.valign == "middle" then
    line.middle = meta.res_y / 2
    line.top = line.middle - line.height / 2
    line.bottom = line.top + line.height
    line.y = line.middle
  else
    line.bottom = meta.res_y - line.eff_margin_b
    line.middle = line.bottom - line.height / 2
    line.top = line.bottom - line.height
    line.y = line.bottom
  end

  -- Hecelerin yatay konumları
  local x = line.left
  for i = 1, line.kara.n do
    local syl = line.kara[i]
    syl.left = x + (syl.prespace or 0)
    syl.center = syl.left + syl.width / 2
    syl.right = syl.left + syl.width
    x = x + (syl.prespace or 0) + syl.width + (syl.postspace or 0)
  end
end

function karaskel.preproc_line(subs, meta, styles, line)
  karaskel.preproc_line_text(meta, styles, line)
  karaskel.preproc_line_size(meta, styles, line)
  karaskel.preproc_line_pos(meta, styles, line)
end

-- Eski ad
karaskel.preproc_lines = nil

return karaskel
