-- karaskel.lua: karaoke skeleton helpers (Osisub implementation, API compatible with Aegisub's karaskel).
karaskel = karaskel or {}
local karaskel = karaskel

function karaskel.collect_head(subs, generate_furigana)
  local meta = { res_x = 0, res_y = 0, video_x_correct_factor = 1.0 }
  local styles = { n = 0 }
  local toinsert = {}
  local first_style_line
  if not karaskel.furigana_scale then karaskel.furigana_scale = 0.5 end

  for i = 1, #subs do
    if aegisub.progress.is_cancelled() then error("User cancelled") end
    local l = subs[i]
    if l.class == "style" then
      first_style_line = first_style_line or i
      styles.n = styles.n + 1
      styles[styles.n] = l
      styles[l.name] = l
      l.margin_v = l.margin_t
      if generate_furigana and not l.name:match("furigana") then
        local fs = table.copy(l)
        fs.fontsize = l.fontsize * karaskel.furigana_scale
        fs.outline = l.outline * karaskel.furigana_scale
        fs.shadow = l.shadow * karaskel.furigana_scale
        fs.name = l.name .. "-furigana"
        toinsert[#toinsert + 1] = fs
      end
    elseif l.class == "info" then
      meta[l.key:lower()] = l.value
    else
      break
    end
  end
  if not first_style_line then error("Karaskel error: No styles were found in the file, bug?!") end

  for i = 1, #toinsert do
    if not styles[toinsert[i].name] then
      styles.n = styles.n + 1
      styles[styles.n] = toinsert[i]
      styles[toinsert[i].name] = toinsert[i]
      subs[-first_style_line] = toinsert[i]
    end
  end

  meta.res_x, meta.res_y = subs.script_resolution()
  local vx, vy = aegisub.video_size()
  if vy then
    meta.video_x_correct_factor = (vy / vx) / (meta.res_y / meta.res_x)
  end
  return meta, styles
end

function karaskel.preproc_line_text(meta, styles, line)
  local kara = aegisub.parse_karaoke_data(line)
  line.kara = { n = 0 }
  line.furi = { n = 0 }
  line.text_stripped = ""
  line.duration = line.end_time - line.start_time

  local worksyl = { highlights = { n = 0 }, furi = { n = 0 } }
  local cur_inline_fx = ""
  for i = 0, kara.n do
    local syl = kara[i]
    local fx = syl.text:match("%{.*\\%-([^}\\]+)")
    if fx then cur_inline_fx = fx end
    local prespace, syltext, postspace = syl.text_stripped:match("^([ \t]*)(.-)([ \t]*)$")
    local prefix = syltext:sub(1, unicode.charwidth(syltext, 1))
    local is_hl = (prefix == "#" or prefix == "＃")
    if not is_hl and i > 0 then
      line.kara[line.kara.n] = worksyl
      line.kara.n = line.kara.n + 1
      worksyl = { highlights = { n = 0 }, furi = { n = 0 } }
    end
    local hl = { start_time = syl.start_time, end_time = syl.end_time, duration = syl.duration }
    worksyl.highlights.n = worksyl.highlights.n + 1
    worksyl.highlights[worksyl.highlights.n] = hl

    if syltext:find("|", 1, true) or syltext:find("｜", 1, true) then
      syltext = syltext:gsub("｜", "|")
      local maintext, furitext = syltext:match("^(.-)|(.-)$")
      syltext = maintext
      local furi = { syl = worksyl }
      local fp = furitext:sub(1, unicode.charwidth(furitext, 1))
      if fp == "!" or fp == "！" then furi.isbreak, furi.spillback = true, false
      elseif fp == "<" or fp == "＜" then furi.isbreak, furi.spillback = true, true
      else furi.isbreak, furi.spillback = false, false end
      if furi.isbreak then furitext = furitext:sub(unicode.charwidth(furitext, 1) + 1) end
      furi.start_time, furi.end_time, furi.duration = syl.start_time, syl.end_time, syl.duration
      furi.kdur = syl.duration / 10
      furi.text, furi.text_stripped, furi.text_spacestripped = furitext, furitext, furitext
      furi.line, furi.tag, furi.inline_fx = line, syl.tag, cur_inline_fx
      furi.i = line.kara.n
      furi.prespace, furi.postspace = "", ""
      furi.highlights = { n = 1, [1] = hl }
      furi.isfuri = true
      line.furi.n = line.furi.n + 1
      line.furi[line.furi.n] = furi
      worksyl.furi.n = worksyl.furi.n + 1
      worksyl.furi[worksyl.furi.n] = furi
    end

    if not worksyl.text or not is_hl then
      line.text_stripped = line.text_stripped .. prespace .. syltext .. postspace
      worksyl.text = syl.text
      worksyl.duration = syl.duration
      worksyl.kdur = syl.duration / 10
      worksyl.start_time, worksyl.end_time = syl.start_time, syl.end_time
      worksyl.tag = syl.tag
      worksyl.line = line
      worksyl.i = line.kara.n
      worksyl.text_stripped = prespace .. syltext .. postspace
      worksyl.inline_fx = cur_inline_fx
      worksyl.text_spacestripped = syltext
      worksyl.prespace, worksyl.postspace = prespace, postspace
    else
      worksyl.duration = worksyl.duration + syl.duration
      worksyl.kdur = worksyl.kdur + syl.duration / 10
      worksyl.end_time = syl.end_time
    end
  end
  line.kara[line.kara.n] = worksyl
end

function karaskel.preproc_line_size(meta, styles, line)
  if not line.kara then karaskel.preproc_line_text(meta, styles, line) end
  if styles[line.style] then
    line.styleref = styles[line.style]
  else
    aegisub.debug.out(2, "WARNING: Style not found: " .. line.style .. "\n")
    line.styleref = styles[1]
  end
  line.width, line.height, line.descent, line.extlead = aegisub.text_extents(line.styleref, line.text_stripped)
  line.width = line.width * meta.video_x_correct_factor
  for s = 0, line.kara.n do
    local syl = line.kara[s]
    syl.style = line.styleref
    syl.width, syl.height = aegisub.text_extents(syl.style, syl.text_spacestripped)
    syl.width = syl.width * meta.video_x_correct_factor
    syl.prespacewidth = aegisub.text_extents(syl.style, syl.prespace) * meta.video_x_correct_factor
    syl.postspacewidth = aegisub.text_extents(syl.style, syl.postspace) * meta.video_x_correct_factor
  end
  line.furistyle = styles[line.style .. "-furigana"] or false
  if line.furistyle then
    for f = 1, line.furi.n do
      local furi = line.furi[f]
      furi.style = line.furistyle
      furi.width, furi.height = aegisub.text_extents(furi.style, furi.text)
      furi.width = furi.width * meta.video_x_correct_factor
      furi.prespacewidth, furi.postspacewidth = 0, 0
    end
  end
end

function karaskel.do_basic_layout(meta, styles, line)
  local curx = 0
  for i = 0, line.kara.n do
    local syl = line.kara[i]
    curx = curx + syl.prespacewidth
    syl.left = curx
    syl.center = curx + syl.width / 2
    syl.right = curx + syl.width
    curx = curx + syl.width + syl.postspacewidth
  end
  line.width = curx
end

-- Furigana layout: syllable positions are computed like the basic layout, and each furigana is
-- centred above its base syllable (spilling to the right is allowed).
function karaskel.do_furigana_layout(meta, styles, line)
  karaskel.do_basic_layout(meta, styles, line)
  for f = 1, line.furi.n do
    local furi = line.furi[f]
    local base = furi.syl
    furi.left = base.center - furi.width / 2
    furi.center = base.center
    furi.right = furi.left + furi.width
    if furi.left < base.left and not furi.spillback then furi.left = base.left; furi.right = furi.left + furi.width; furi.center = furi.left + furi.width / 2 end
  end
end

function karaskel.preproc_line_pos(meta, styles, line)
  if not line.styleref then karaskel.preproc_line_size(meta, styles, line) end
  if line.furistyle then karaskel.do_furigana_layout(meta, styles, line)
  else karaskel.do_basic_layout(meta, styles, line) end

  local sr = line.styleref
  line.margin_v = line.margin_t
  line.eff_margin_l = (line.margin_l > 0) and line.margin_l or sr.margin_l
  line.eff_margin_r = (line.margin_r > 0) and line.margin_r or sr.margin_r
  line.eff_margin_t = (line.margin_t > 0) and line.margin_t or sr.margin_t
  line.eff_margin_b = (line.margin_b > 0) and line.margin_b or sr.margin_b
  line.eff_margin_v = (line.margin_v > 0) and line.margin_v or sr.margin_v
  local a = sr.align
  if a == 1 or a == 4 or a == 7 then
    line.left = line.eff_margin_l
    line.center = line.left + line.width / 2
    line.right = line.left + line.width
    line.x = line.left
    line.halign = "left"
  elseif a == 2 or a == 5 or a == 8 then
    line.left = (meta.res_x - line.eff_margin_l - line.eff_margin_r - line.width) / 2 + line.eff_margin_l
    line.center = line.left + line.width / 2
    line.right = line.left + line.width
    line.x = line.center
    line.halign = "center"
  else
    line.left = meta.res_x - line.eff_margin_r - line.width
    line.center = line.left + line.width / 2
    line.right = line.left + line.width
    line.x = line.right
    line.halign = "right"
  end
  line.hcenter = line.center
  if a >= 1 and a <= 3 then
    line.bottom = meta.res_y - line.eff_margin_b
    line.middle = line.bottom - line.height / 2
    line.top = line.bottom - line.height
    line.y = line.bottom
    line.valign = "bottom"
  elseif a >= 4 and a <= 6 then
    line.top = (meta.res_y - line.eff_margin_t - line.eff_margin_b - line.height) / 2 + line.eff_margin_t
    line.middle = line.top + line.height / 2
    line.bottom = line.top + line.height
    line.y = line.middle
    line.valign = "middle"
  else
    line.top = line.eff_margin_t
    line.middle = line.top + line.height / 2
    line.bottom = line.top + line.height
    line.y = line.top
    line.valign = "top"
  end
  line.vcenter = line.middle
end

function karaskel.preproc_line(subs, meta, styles, line)
  karaskel.preproc_line_text(meta, styles, line)
  karaskel.preproc_line_size(meta, styles, line)
  karaskel.preproc_line_pos(meta, styles, line)
end

function karaskel.use_fx_library_furi(use_furigana, macrotoo)
  local function fx_library_main(subs)
    aegisub.progress.task("Collecting header data")
    local meta, styles = karaskel.collect_head(subs, use_furigana)
    local i, maxi = 1, #subs
    while i <= maxi do
      aegisub.progress.set(i / maxi * 100)
      local l = subs[i]
      if l.class == "dialogue" and ((l.effect == "fx") or (l.effect == "karaoke" and false)) and not l.comment then
        karaskel.preproc_line(subs, meta, styles, l)
        local keep = false
        if type(_G.do_fx) == "function" then keep = _G.do_fx(subs, meta, styles, l, {}) end
        if not keep then l.comment = true; subs[i] = l end
      end
      i = i + 1
    end
    aegisub.set_undo_point(script_name or "fx")
  end
  aegisub.register_filter(script_name or "fx", script_description or "", 2000, fx_library_main)
  if macrotoo then aegisub.register_macro(script_name or "fx", script_description or "", fx_library_main) end
end

function karaskel.use_fx_library(macrotoo) return karaskel.use_fx_library_furi(false, macrotoo) end

function karaskel.use_classic_adv(use_furigana, macrotoo)
  local function main(subs)
    local meta, styles = karaskel.collect_head(subs, use_furigana)
    local sylfunc = type(_G.do_syllable) == "function" and _G.do_syllable or function() end
    local linefunc = type(_G.do_line) == "function" and _G.do_line
    local i = 1
    while i <= #subs do
      local l = subs[i]
      if l.class == "dialogue" and not l.comment and l.effect:match("^karaoke") then
        karaskel.preproc_line(subs, meta, styles, l)
        if linefunc then linefunc(subs, meta, styles, nil, l)
        else for s = 0, l.kara.n do sylfunc(subs, meta, styles, nil, l, l.kara[s]) end end
      end
      i = i + 1
    end
    aegisub.set_undo_point(script_name or "karaoke")
  end
  aegisub.register_filter(script_name or "karaoke", script_description or "", 2000, main)
  if macrotoo then aegisub.register_macro(script_name or "karaoke", script_description or "", main) end
end

return karaskel
