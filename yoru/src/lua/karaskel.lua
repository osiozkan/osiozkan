-- karaskel.lua uyumlu: karaoke şablonları için satır/stil ön işleme
karaskel = {}

local function default_res(meta, x, y)
  -- Aegisub: yalnızca biri tanımlıysa 4:3 varsayımı, hiçbiri yoksa 384x288
  if x and y then return x, y end
  if x then return x, math.floor(x * 3 / 4) end
  if y then return math.floor(y * 4 / 3), y end
  return 384, 288
end

function karaskel.collect_head(subs, generate_furigana)
  local meta = { res_x = 0, res_y = 0 }
  local styles = { n = 0 }
  local rx, ry
  local infos = {}
  for i = 1, #subs do
    local l = subs[i]
    if l.class == "info" then
      infos[l.key] = l.value
      local k = tostring(l.key):lower()
      if k == "playresx" then rx = tonumber(l.value) elseif k == "playresy" then ry = tonumber(l.value) end
      if k == "wrapstyle" then meta.wrapstyle = tonumber(l.value) end
      if k == "scaledborderandshadow" then meta.scaledborder = tostring(l.value):lower() == "yes" end
    elseif l.class == "style" then
      styles.n = styles.n + 1
      styles[styles.n] = l
      styles[l.name] = l
    elseif l.class == "dialogue" then
      break
    end
  end
  meta.res_x, meta.res_y = default_res(meta, rx, ry)
  meta.infos = infos
  karaskel.fix_styles(styles, meta)
  if not styles["Default"] and styles.n > 0 then styles["Default"] = styles[1] end
  return meta, styles
end

function karaskel.fix_styles(styles, meta)
  for i = 1, styles.n do
    local s = styles[i]
    s.margin_v = s.margin_t
    s.margin_t = s.margin_t or s.margin_v
    s.margin_b = s.margin_t
    -- hizalama türetilmiş alanları
    s.halign = (s.align - 1) % 3          -- 0 sol, 1 orta, 2 sağ
    s.valign = (s.align - 1) // 3         -- 0 alt, 1 orta, 2 üst
    s.width, s.height, s.descent, s.extlead = aegisub.text_extents(s, "A")
  end
end

local function strip_tags(t) return (t:gsub("{[^}]*}", "")) end

function karaskel.preproc_line_text(meta, styles, line)
  line.styleref = styles[line.style] or styles["Default"] or styles[1]
  line.text_stripped = strip_tags(line.text)
  line.duration = line.end_time - line.start_time
  line.kara = { n = 0 }
  -- karaoke hecelerine ayır
  local text = line.text
  local pos, cur_start = 1, 0
  local syl = { tag = "k", duration = 0, text = "", inline_fx = "", i = 0, prespace = "", postspace = "" }
  local started = false
  local function pushsyl()
    if started or syl.text ~= "" then
      line.kara.n = line.kara.n + 1
      syl.i = line.kara.n
      syl.start_time = cur_start
      syl.end_time = cur_start + syl.duration * 10
      syl.duration = syl.duration * 10
      cur_start = syl.end_time
      syl.text_stripped = strip_tags(syl.text)
      local pre, body, post = syl.text_stripped:match("^(%s*)(.-)(%s*)$")
      syl.prespace, syl.text_spacestripped, syl.postspace = pre, body, post
      syl.line = line
      line.kara[line.kara.n] = syl
    end
    syl = { tag = "k", duration = 0, text = "", inline_fx = "", i = 0, prespace = "", postspace = "" }
    started = false
  end
  local first = true
  while pos <= #text do
    local a, b, blk = text:find("^{([^}]*)}", pos)
    if a then
      local kdone = false
      for ktag, kd in blk:gmatch("\\([kK][fo]?)(%d+)") do
        if started or syl.text ~= "" then pushsyl() end
        syl.tag = ktag:lower() == "k" and "k" or ktag:lower()
        if ktag == "K" then syl.tag = "kf" end
        syl.duration = tonumber(kd); started = true; kdone = true
        local rest = blk:gsub("\\[kK][fo]?%d+", "")
        syl.text = syl.text .. (rest ~= "" and "{" .. rest .. "}" or "")
        local fx = blk:match("\\%-([%w_]+)")
        if fx then syl.inline_fx = fx end
        break
      end
      if not kdone then syl.text = syl.text .. "{" .. blk .. "}" end
      pos = b + 1
    else
      local nxt = text:find("{", pos, true) or (#text + 1)
      syl.text = syl.text .. text:sub(pos, nxt - 1)
      pos = nxt
    end
    first = false
  end
  pushsyl()
  line.kara.n = #line.kara
  -- k etiketi yoksa tek heceyi tüm satır olarak bırak
  if line.kara.n == 0 then line.kara.n = 0 end
end

function karaskel.preproc_line_size(meta, styles, line)
  local s = line.styleref
  line.width, line.height, line.descent, line.extlead = aegisub.text_extents(s, line.text_stripped)
  for i = 1, line.kara.n do
    local syl = line.kara[i]
    syl.width, syl.height, syl.descent, syl.extlead = aegisub.text_extents(s, syl.text_stripped)
    syl.prespace_width = aegisub.text_extents(s, syl.prespace)
    syl.postspace_width = aegisub.text_extents(s, syl.postspace)
  end
end

function karaskel.preproc_line_pos(meta, styles, line)
  local s = line.styleref
  line.margin_l = (line.margin_l and line.margin_l ~= 0) and line.margin_l or s.margin_l
  line.margin_r = (line.margin_r and line.margin_r ~= 0) and line.margin_r or s.margin_r
  line.margin_v = (line.margin_t and line.margin_t ~= 0) and line.margin_t or s.margin_t
  line.eff_margin_l, line.eff_margin_r, line.eff_margin_t, line.eff_margin_b = line.margin_l, line.margin_r, line.margin_v, line.margin_v
  line.halign = (s.align - 1) % 3 == 0 and "left" or ((s.align - 1) % 3 == 1 and "center" or "right")
  line.valign = (s.align - 1) // 3 == 0 and "bottom" or ((s.align - 1) // 3 == 1 and "middle" or "top")
  if line.halign == "left" then line.left = line.margin_l; line.center = line.left + line.width / 2; line.right = line.left + line.width
  elseif line.halign == "center" then line.left = (meta.res_x - line.margin_l - line.margin_r - line.width) / 2 + line.margin_l
    line.center = line.left + line.width / 2; line.right = line.left + line.width
  else line.right = meta.res_x - line.margin_r; line.center = line.right - line.width / 2; line.left = line.right - line.width end
  if line.valign == "top" then line.top = line.margin_v; line.middle = line.top + line.height / 2; line.bottom = line.top + line.height
  elseif line.valign == "middle" then line.top = (meta.res_y - line.height) / 2; line.middle = line.top + line.height / 2; line.bottom = line.top + line.height
  else line.bottom = meta.res_y - line.margin_v; line.middle = line.bottom - line.height / 2; line.top = line.bottom - line.height end
  line.x = line.halign == "left" and line.left or (line.halign == "center" and line.center or line.right)
  line.y = line.valign == "top" and line.top or (line.valign == "middle" and line.middle or line.bottom)
  local x = line.left
  for i = 1, line.kara.n do
    local syl = line.kara[i]
    syl.left = x + syl.prespace_width
    syl.center = syl.left + (syl.width - syl.prespace_width - syl.postspace_width) / 2
    syl.right = syl.left + (syl.width - syl.prespace_width - syl.postspace_width)
    syl.top, syl.middle, syl.bottom = line.top, line.middle, line.bottom
    syl.x = syl.center; syl.y = line.y
    x = x + syl.width
  end
end

function karaskel.preproc_line(subs, meta, styles, line)
  karaskel.preproc_line_text(meta, styles, line)
  karaskel.preproc_line_size(meta, styles, line)
  karaskel.preproc_line_pos(meta, styles, line)
end

return karaskel
