-- karaskel.lua (Aegisub uyumlu alt küme; stil/ölçü/konum ön-işleme)
local karaskel = {}
local function parse_colors(s) return s end

function karaskel.collect_head(subs, generate_furigana)
  local meta = { res_x = 0, res_y = 0 }
  local styles = { n = 0 }
  local first_style_line = nil
  for i = 1, #subs do
    local l = subs[i]
    if l.class == "info" then
      local k = l.key:lower()
      if k == "playresx" then meta.res_x = tonumber(l.value) or 0
      elseif k == "playresy" then meta.res_y = tonumber(l.value) or 0
      elseif k == "wrapstyle" then meta.wrapstyle = tonumber(l.value) end
      meta[l.key:lower()] = l.value
    elseif l.class == "style" then
      first_style_line = first_style_line or i
      styles.n = styles.n + 1
      styles[styles.n] = l
      styles[l.name] = l
      l.margin_v = l.margin_t
    end
  end
  if meta.res_x == 0 and meta.res_y == 0 then meta.res_x, meta.res_y = 384, 288
  elseif meta.res_x == 0 then meta.res_x = meta.res_y / 3 * 4
  elseif meta.res_y == 0 then meta.res_y = meta.res_x * 3 / 4 end
  -- kullanılan stilleri ölç
  for i = 1, styles.n do
    local s = styles[i]
    local w, h, d, e = aegisub.text_extents(s, "A")
    s.height, s.descent, s.extlead = h, d, e
    s.space_width = aegisub.text_extents(s, " ")
  end
  return meta, styles
end

function karaskel.preproc_line_text(meta, styles, line)
  line.text_stripped = line.text:gsub("%b{}", "")
  line.duration = line.end_time - line.start_time
  line.kara = { n = 0 }
  local pos, cur = 1, nil
  local text = line.text
  local k_dur, k_tag = 0, "k"
  local syl_start = 0
  local acc = ""
  local function push(txt, dur, tag, inlinefx)
    line.kara.n = line.kara.n + 1
    local s = {
      text = txt, duration = dur * 10, kdur = dur, tag = tag, inline_fx = inlinefx or "",
      start_time = syl_start, i = line.kara.n,
    }
    s.end_time = s.start_time + s.duration
    s.text_stripped = txt:gsub("%b{}", "")
    s.text_spacestripped = s.text_stripped:gsub("^%s+", ""):gsub("%s+$", "")
    s.prespace = #s.text_stripped:match("^%s*")
    s.postspace = #s.text_stripped:match("%s*$")
    line.kara[line.kara.n] = s
    syl_start = syl_start + s.duration
  end
  local has_k = false
  -- {\k..}hece bölümlerini topla
  local rest = text
  local lead = rest:match("^([^{]*)")
  local first = true
  for tags, body in rest:gmatch("{([^}]*)}([^{]*)") do
    local tg, d = tags:match("\\([kK][fo]?)(%d+)")
    if tg then
      has_k = true
      push(body, tonumber(d), tg:lower(), tags:gsub("\\[kK][fo]?%d+", ""))
    elseif line.kara.n > 0 then
      line.kara[line.kara.n].text = line.kara[line.kara.n].text .. "{" .. tags .. "}" .. body
      line.kara[line.kara.n].text_stripped = line.kara[line.kara.n].text_stripped .. body
    end
  end
  if not has_k then
    push(line.text_stripped, math.floor(line.duration / 10), "k")
  end
end

function karaskel.preproc_line_size(meta, styles, line)
  local st = line.styleref
  local w, h, d, e = aegisub.text_extents(st, line.text_stripped)
  line.width, line.height, line.descent, line.extlead = w, h, d, e
  for i = 1, line.kara.n do
    local s = line.kara[i]
    s.width = aegisub.text_extents(st, s.text_stripped)
    s.prespacewidth = aegisub.text_extents(st, string.rep(" ", s.prespace))
    s.postspacewidth = aegisub.text_extents(st, string.rep(" ", s.postspace))
    local sp = s.text_spacestripped
    s.wordwidth = aegisub.text_extents(st, sp)
  end
  line.syllables = line.kara
end

function karaskel.preproc_line_pos(meta, styles, line)
  local st = line.styleref
  line.margin_v = (line.margin_t ~= 0) and line.margin_t or st.margin_t
  line.margin_l = (line.margin_l ~= 0) and line.margin_l or st.margin_l
  line.margin_r = (line.margin_r ~= 0) and line.margin_r or st.margin_r
  local al = st.align
  local col = (al - 1) % 3          -- 0 sol, 1 orta, 2 sağ
  local row = math.floor((al - 1) / 3) -- 0 alt, 1 orta, 2 üst
  line.halign = ({ "left", "center", "right" })[col + 1]
  line.valign = ({ "bottom", "middle", "top" })[row + 1]
  if col == 0 then line.left = line.margin_l; line.center = line.left + line.width / 2; line.right = line.left + line.width
  elseif col == 1 then line.left = (meta.res_x - line.margin_l - line.margin_r - line.width) / 2 + line.margin_l
    line.center = line.left + line.width / 2; line.right = line.left + line.width
  else line.right = meta.res_x - line.margin_r; line.left = line.right - line.width; line.center = line.left + line.width / 2 end
  if row == 0 then line.bottom = meta.res_y - line.margin_v; line.top = line.bottom - line.height; line.middle = line.top + line.height / 2
  elseif row == 2 then line.top = line.margin_v; line.bottom = line.top + line.height; line.middle = line.top + line.height / 2
  else line.middle = meta.res_y / 2; line.top = line.middle - line.height / 2; line.bottom = line.middle + line.height / 2 end
  line.x = ({ line.left, line.center, line.right })[col + 1]
  line.y = ({ line.bottom, line.middle, line.top })[row + 1]
  local x = line.left
  for i = 1, line.kara.n do
    local s = line.kara[i]
    s.left = x + s.prespacewidth
    s.center = s.left + s.wordwidth / 2
    s.right = s.left + s.wordwidth
    x = x + s.width
  end
end

function karaskel.preproc_line(subs, meta, styles, line)
  line.styleref = styles[line.style] or styles["Default"] or styles[1]
  karaskel.preproc_line_text(meta, styles, line)
  karaskel.preproc_line_size(meta, styles, line)
  karaskel.preproc_line_pos(meta, styles, line)
end

function karaskel.fix_zero_length_syls(subs, meta, styles) end
function karaskel.use_furigana() end
_G.karaskel = karaskel
return karaskel
