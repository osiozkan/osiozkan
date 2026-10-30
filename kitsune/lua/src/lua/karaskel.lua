-- Aegisub "karaskel.lua" uyumlu iskelet (Kitsune). Ölçüler aegisub.text_extents'e dayanır.
local karaskel = {}

function karaskel.collect_head(subs, generate_furigana)
  local meta = { res_x = 0, res_y = 0 }
  local styles = { n = 0 }
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
    elseif l.class == "dialogue" then
      break
    end
  end
  if meta.res_x == 0 and meta.res_y == 0 then meta.res_x, meta.res_y = 384, 288
  elseif meta.res_y == 0 then meta.res_y = meta.res_x * 3 / 4
  elseif meta.res_x == 0 then meta.res_x = meta.res_y * 4 / 3 end
  for i = 1, styles.n do karaskel.parse_style_data(meta, styles, styles[i]) end
  return meta, styles
end

function karaskel.parse_style_data(meta, styles, st)
  st.margin_v = st.margin_t
  return st
end

function karaskel.preproc_line_text(meta, styles, line)
  line.text_stripped = line.text:gsub("{.-}", ""):gsub("\\[Nn]", " "):gsub("\\h", " ")
  line.duration = line.end_time - line.start_time
  line.kara = { n = 0 }
end

function karaskel.preproc_line_size(meta, styles, line)
  local st = line.styleref
  local w, h, d, e = aegisub.text_extents(st, line.text_stripped)
  line.width, line.height, line.descent, line.extlead = w, h, d, e
end

function karaskel.preproc_line_pos(meta, styles, line)
  local st = line.styleref
  line.margin_l = line.margin_l > 0 and line.margin_l or st.margin_l
  line.margin_r = line.margin_r > 0 and line.margin_r or st.margin_r
  line.margin_v = line.margin_t > 0 and line.margin_t or st.margin_t
  line.eff_margin_l, line.eff_margin_r, line.eff_margin_t, line.eff_margin_b =
    line.margin_l, line.margin_r, line.margin_v, line.margin_v
  local al = st.align
  local col = (al - 1) % 3
  if col == 0 then
    line.left = line.margin_l; line.center = line.left + line.width / 2; line.right = line.left + line.width
    line.halign = "left"
  elseif col == 1 then
    line.left = (meta.res_x - line.margin_l - line.margin_r - line.width) / 2 + line.margin_l
    line.center = line.left + line.width / 2; line.right = line.left + line.width; line.halign = "center"
  else
    line.right = meta.res_x - line.margin_r; line.left = line.right - line.width
    line.center = line.left + line.width / 2; line.halign = "right"
  end
  if al >= 7 then
    line.top = line.margin_v; line.middle = line.top + line.height / 2; line.bottom = line.top + line.height
    line.valign = "top"
  elseif al >= 4 then
    line.top = (meta.res_y - line.height) / 2; line.middle = meta.res_y / 2; line.bottom = line.top + line.height
    line.valign = "middle"
  else
    line.bottom = meta.res_y - line.margin_v; line.top = line.bottom - line.height
    line.middle = line.top + line.height / 2; line.valign = "bottom"
  end
  line.x, line.y = line.center, line.bottom
  line.vcenter = line.middle
end

function karaskel.preproc_line(subs, meta, styles, line)
  line.styleref = styles[line.style] or styles["Default"] or styles[1]
  karaskel.preproc_line_text(meta, styles, line)
  karaskel.preproc_line_size(meta, styles, line)
  karaskel.preproc_line_pos(meta, styles, line)
end

return karaskel
