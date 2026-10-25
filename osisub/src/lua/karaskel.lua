-- karaskel.lua: Aegisub-compatible karaoke skeleton
local karaskel = {}
_G.karaskel = karaskel
local util = require 'utils'

function karaskel.collect_head(subs, generate_furigana)
  local meta = { res_x = 0, res_y = 0 }
  local styles = { n = 0 }
  local first_style_line = nil
  for i = 1, #subs do
    local l = subs[i]
    if l.class == 'info' then
      local k = l.key:lower()
      if k == 'playresx' then meta.res_x = math.floor(tonumber(l.value) or 0)
      elseif k == 'playresy' then meta.res_y = math.floor(tonumber(l.value) or 0) end
      meta[l.key] = l.value
    elseif l.class == 'style' then
      if not first_style_line then first_style_line = i end
      styles.n = styles.n + 1
      styles[styles.n] = l
      styles[l.name] = l
      l.margin_v = l.margin_t
      -- derived
      l.color1, l.color2, l.color3, l.color4 = l.color1, l.color2, l.color3, l.color4
    elseif l.class == 'dialogue' then break end
  end
  if meta.res_x == 0 and meta.res_y == 0 then meta.res_x = 384; meta.res_y = 288
  elseif meta.res_x == 0 then meta.res_x = meta.res_y / 3 * 4
  elseif meta.res_y == 0 then meta.res_y = meta.res_x * 3 / 4 end
  meta.video_x_correct_factor = 1
  return meta, styles
end

function karaskel.use_ass_features() end

local function style_extents(style, text) return aegisub.text_extents(style, text) end

function karaskel.preproc_line_size(meta, styles, line)
  local st = line.styleref or styles[line.style]
  if not st then return end
  line.styleref = st
  local w, h, d, el = style_extents(st, line.text_stripped or line.text)
  line.width, line.height, line.descent, line.extlead = w, h, d, el
end

function karaskel.preproc_line_pos(meta, styles, line)
  local st = line.styleref
  if not st then return end
  local mv = line.margin_t ~= 0 and line.margin_t or st.margin_t
  line.margin_v = mv
  line.eff_margin_l = line.margin_l ~= 0 and line.margin_l or st.margin_l
  line.eff_margin_r = line.margin_r ~= 0 and line.margin_r or st.margin_r
  line.eff_margin_t = mv; line.eff_margin_b = mv; line.eff_margin_v = mv
  local al = st.align
  local w = line.width or 0
  if al % 3 == 1 then line.left = line.eff_margin_l; line.center = line.left + w / 2; line.right = line.left + w
  elseif al % 3 == 2 then line.left = (meta.res_x - line.eff_margin_l - line.eff_margin_r - w) / 2 + line.eff_margin_l; line.center = line.left + w / 2; line.right = line.left + w
  else line.right = meta.res_x - line.eff_margin_r; line.center = line.right - w / 2; line.left = line.right - w end
  line.hcenter = line.center
  if al > 6 then line.top = mv; line.middle = line.top + (line.height or 0) / 2; line.bottom = line.top + (line.height or 0)
  elseif al > 3 then line.middle = meta.res_y / 2; line.top = line.middle - (line.height or 0) / 2; line.bottom = line.top + (line.height or 0)
  else line.bottom = meta.res_y - mv; line.middle = line.bottom - (line.height or 0) / 2; line.top = line.bottom - (line.height or 0) end
  line.vcenter = line.middle
  line.x = line.center; line.y = line.middle
  if al % 3 == 1 then line.x = line.left elseif al % 3 == 0 then line.x = line.right end
  if al > 6 then line.y = line.top elseif al <= 3 then line.y = line.bottom end
end

-- split karaoke into syllables
function karaskel.parse_karaoke(line, meta, styles)
  local kara = { n = 0 }
  line.kara = kara
  local text = line.text
  local i, tstart = 1, 0
  local cur_t, cur_dur = '', 0
  local pending = { dur = 0, tags = '', text = '', tag = 'k' }
  local blocks = {}
  local pos = 1
  local function push(syl) kara.n = kara.n + 1; kara[kara.n] = syl end
  local syl
  local first = true
  -- tokenise
  local s = text
  local idx = 1
  local curtime = 0
  local cursyl = { text = '', tags = '', duration = 0, start_time = 0, end_time = 0, tag = 'k', text_stripped = '', inline_fx = '' }
  local started = false
  local function finish()
    if started then
      cursyl.text_stripped = cursyl.text:gsub('{[^}]*}', '')
      cursyl.end_time = cursyl.start_time + cursyl.duration
      cursyl.i = kara.n + 1
      cursyl.text_spacestripped = cursyl.text_stripped:gsub('^%s+', ''):gsub('%s+$', '')
      cursyl.prespace = #(cursyl.text_stripped:match('^%s*'))
      cursyl.postspace = #(cursyl.text_stripped:match('%s*$'))
      push(cursyl)
      curtime = cursyl.end_time
    end
  end
  local lead = s:match('^[^{]*')
  if lead ~= '' then
    started = true; cursyl = { text = lead, tags = '', duration = 0, start_time = 0, tag = 'k', inline_fx = '' }
  end
  for block, rest in s:gmatch('{([^}]*)}([^{]*)') do
    local ktag, kdur = block:match('\\(k[fo]?)(%d+)')
    if not ktag then ktag, kdur = block:match('\\(K)(%d+)'); if ktag then ktag = 'kf' end end
    local fx = block:match('\\%-([^\\]*)') or ''
    if ktag then
      finish()
      started = true
      cursyl = { text = '{' .. block:gsub('\\k[fo]?%d+', '') .. '}' .. rest, tags = block, duration = tonumber(kdur) * 10, start_time = curtime, tag = ktag, inline_fx = fx }
      cursyl.text = rest
      cursyl.tags = block:gsub('\\[kK][fo]?%d+', '')
    else
      if not started then started = true; cursyl = { text = '', tags = '', duration = 0, start_time = 0, tag = 'k', inline_fx = '' } end
      cursyl.text = cursyl.text .. '{' .. block .. '}' .. rest
    end
  end
  finish()
  -- compute syllable positions
  local acc = 0
  for _, syl in ipairs(kara) do
    syl.start_time = acc; syl.end_time = acc + syl.duration; acc = syl.end_time
  end
  line.duration = line.end_time - line.start_time
  if line.styleref then
    local left = 0
    local st = line.styleref
    for i = 1, kara.n do
      local syl = kara[i]
      syl.width, syl.height, syl.descent, syl.extlead = aegisub.text_extents(st, syl.text_stripped)
      syl.prespacewidth = (syl.prespace > 0) and (aegisub.text_extents(st, string.rep(' ', syl.prespace))) or 0
      syl.postspacewidth = (syl.postspace > 0) and (aegisub.text_extents(st, string.rep(' ', syl.postspace))) or 0
      syl.left = left; syl.center = left + syl.width / 2; syl.right = left + syl.width
      left = left + syl.width
    end
    line.kara_width = left
    -- absolute (relative to line.left) positions
    for i = 1, kara.n do
      local syl = kara[i]
      syl.left = syl.left + (line.left or 0); syl.center = syl.center + (line.left or 0); syl.right = syl.right + (line.left or 0)
    end
  end
  return kara
end

function karaskel.preproc_line_text(meta, styles, line)
  line.text_stripped = line.text:gsub('{[^}]*}', '')
end

function karaskel.preproc_line(subs, meta, styles, line)
  line.styleref = styles[line.style]
  line.text_stripped = line.text:gsub('{[^}]*}', '')
  line.duration = line.end_time - line.start_time
  line.kdur = 0
  line.layer0 = line.layer
  if line.styleref then
    karaskel.preproc_line_size(meta, styles, line)
    karaskel.preproc_line_pos(meta, styles, line)
    karaskel.parse_karaoke(line, meta, styles)
  end
  -- raw kara tags in text
end

function karaskel.preproc_lines(subs, meta, styles)
  local n = 0
  for i = 1, #subs do
    local l = subs[i]
    if l.class == 'dialogue' then
      n = n + 1
      karaskel.preproc_line(subs, meta, styles, l)
      l.i = n
    end
  end
end

function karaskel.preproc_line_compat() end
function karaskel.get_tags(text) return text:gmatch('\\[%w]+[^\\}]*') end

return karaskel
