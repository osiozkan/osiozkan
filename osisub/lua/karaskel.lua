-- karaskel.lua: karaoke skeleton helper (Aegisub-compatible subset)
include("utils.lua")
karaskel = {}

function karaskel.collect_head(subs, generate_furigana)
  local meta, styles = {res_x = 0, res_y = 0}, {}
  local last_style_idx = 0
  for i = 1, #subs do
    local l = subs[i]
    if l.class == "info" then
      local k = l.key:lower()
      if k == "playresx" then meta.res_x = tonumber(l.value) or 0
      elseif k == "playresy" then meta.res_y = tonumber(l.value) or 0
      elseif k == "wrapstyle" then meta.wrap_style = tonumber(l.value) end
      meta[l.key] = l.value
    elseif l.class == "style" then
      styles[#styles + 1] = l; styles[l.name] = l; l.margin_v = l.margin_t
      last_style_idx = i
    elseif l.class == "dialogue" then break end
  end
  if meta.res_x == 0 and meta.res_y == 0 then meta.res_x, meta.res_y = 384, 288
  elseif meta.res_x == 0 then meta.res_x = meta.res_y / 3 * 4
  elseif meta.res_y == 0 then meta.res_y = meta.res_x * 3 / 4 end
  if not styles["Default"] and #styles > 0 then styles["Default"] = styles[1] end
  return meta, styles
end

function karaskel.dialogue_start(subs)
  for i = 1, #subs do if subs[i].class == "dialogue" then return i end end
  return #subs + 1
end

function karaskel.preproc_line_text(meta, styles, line)
  line.text_stripped = line.text:gsub("%b{}", "")
  line.duration = line.end_time - line.start_time
  line.styleref = styles[line.style] or styles["Default"]
  line.kara = {n = 0}
  local pos, t = 1, line.text
  local cur_dur, syls, k = 0, {}, nil
  local cur_start = 0
  local last_end = 0
  -- split into syllables on \k tags
  local segs, i = {}, 1
  local cur = {text = "", tags = "", duration = 0, start_time = 0, i = 0}
  local function flush()
    if cur.text ~= "" or cur.duration > 0 or cur.tags ~= "" then segs[#segs + 1] = cur end
  end
  local started = false
  while i <= #t do
    local a, b = t:find("^%b{}", i)
    if a then
      local blk = t:sub(a, b)
      local kd = blk:match("\\[kK][fo]?(%d+)")
      if kd then
        if started then flush() end
        started = true
        cur = {text = "", tags = blk, duration = tonumber(kd) * 10, start_time = (segs[#segs] and (segs[#segs].start_time + segs[#segs].duration) or 0), i = #segs + 1}
        cur.tag = blk:match("\\([kK][fo]?)%d+"); cur.text_spacestripped = ""
      else
        cur.tags = cur.tags .. blk
      end
      i = b + 1
    else
      local nxt = t:find("{", i, true) or (#t + 1)
      cur.text = cur.text .. t:sub(i, nxt - 1)
      i = nxt
    end
  end
  if started then flush() end
  for n, s in ipairs(segs) do
    s.i = n; s.text_stripped = s.text; s.text_spacestripped = s.text:gsub("^%s+", ""):gsub("%s+$", "")
    s.end_time = s.start_time + s.duration
    s.syl_dur = s.duration
    s.kdur = s.duration / 10
    line.kara[n] = s
  end
  line.kara.n = #segs
end

function karaskel.preproc_line_size(meta, styles, line)
  local st = line.styleref
  local w, h, d, e = aegisub.text_extents(st, line.text_stripped)
  line.width, line.height, line.descent, line.extlead = w, h, d, e
  for _, s in ipairs(line.kara or {}) do
    s.width, s.height, s.descent, s.extlead = aegisub.text_extents(st, s.text_stripped)
    s.prespacewidth = 0; s.postspacewidth = 0
  end
  local left = 0
  for _, s in ipairs(line.kara or {}) do
    s.left = left; s.center = left + s.width / 2; s.right = left + s.width; left = s.right
  end
end

function karaskel.preproc_line_pos(meta, styles, line)
  local st = line.styleref
  line.margin_v = line.margin_t
  line.eff_margin_l = line.margin_l > 0 and line.margin_l or st.margin_l
  line.eff_margin_r = line.margin_r > 0 and line.margin_r or st.margin_r
  line.eff_margin_t = line.margin_t > 0 and line.margin_t or st.margin_t
  line.eff_margin_b = line.eff_margin_t; line.eff_margin_v = line.eff_margin_t
  local al = st.align
  if line.text:match("{[^}]*\\an(%d)") then al = tonumber(line.text:match("{[^}]*\\an(%d)")) end
  local hal = (al - 1) % 3
  if hal == 0 then
    line.left = line.eff_margin_l; line.center = line.left + line.width / 2; line.right = line.left + line.width
  elseif hal == 1 then
    line.left = (meta.res_x - line.eff_margin_l - line.eff_margin_r - line.width) / 2 + line.eff_margin_l
    line.center = line.left + line.width / 2; line.right = line.left + line.width
  else
    line.right = meta.res_x - line.eff_margin_r; line.left = line.right - line.width; line.center = line.left + line.width / 2
  end
  local val = math.floor((al - 1) / 3)
  if val == 0 then line.bottom = meta.res_y - line.eff_margin_b; line.middle = line.bottom - line.height / 2; line.top = line.bottom - line.height
  elseif val == 1 then line.middle = meta.res_y / 2; line.top = line.middle - line.height / 2; line.bottom = line.top + line.height
  else line.top = line.eff_margin_t; line.middle = line.top + line.height / 2; line.bottom = line.top + line.height end
  line.hcenter = line.center; line.vcenter = line.middle
  for _, s in ipairs(line.kara or {}) do
    s.left = s.left + line.left; s.center = s.center + line.left; s.right = s.right + line.left
    s.top, s.middle, s.bottom = line.top, line.middle, line.bottom
  end
end

function karaskel.preproc_line(subs, meta, styles, line)
  karaskel.preproc_line_text(meta, styles, line)
  karaskel.preproc_line_size(meta, styles, line)
  karaskel.preproc_line_pos(meta, styles, line)
end

function karaskel.is_syl_line(line) return line.kara and line.kara.n > 0 end
return karaskel
