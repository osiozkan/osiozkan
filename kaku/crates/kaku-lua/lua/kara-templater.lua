-- Kaku karaoke templater: Aegisub "Apply karaoke template" compatible (clean-room implementation).
script_name = "Karaoke templater"
script_description = "Macro and export filter to apply karaoke effect templates to a script"
script_author = "kaku"
script_version = "1"

include("karaskel.lua")
local util = require "util"

local function trim(s) return (s:gsub("^%s+", ""):gsub("%s+$", "")) end

local function fmtnum(v)
  if type(v) ~= "number" then return tostring(v) end
  if v == math.floor(v) then return string.format("%d", v) end
  local s = string.format("%.3f", v)
  s = s:gsub("0+$", ""):gsub("%.$", "")
  return s
end

-- Parse the Effect field of a template/code line -------------------------------------
local function parse_effect(effect)
  local toks = {}
  for w in effect:gmatch("%S+") do toks[#toks + 1] = w end
  local kind = toks[1] and toks[1]:lower()
  if kind ~= "template" and kind ~= "code" then return nil end
  local t = { kind = kind, type = (toks[2] or "line"):lower(), loop = 1, mods = {} }
  local i = 3
  if t.type == "pre" or t.type == "post" then -- "pre-line" written with a space
    t.type = t.type .. "-" .. (toks[3] or "line"); i = 4
  end
  while toks[i] do
    local m = toks[i]:lower()
    if m == "fx" then t.fx = toks[i + 1]; i = i + 1
    elseif m == "fxgroup" then t.fxgroup = toks[i + 1]; i = i + 1
    elseif m == "loop" or m == "repeat" then t.loop = tonumber(toks[i + 1]) or 1; i = i + 1
    else t.mods[m] = true end
    i = i + 1
  end
  return t
end

local function new_env(subs, meta, styles)
  local env = setmetatable({}, { __index = _G })
  env.util, env.meta, env.styles, env.subs = util, meta, styles, subs
  env.karaskel = karaskel
  env.math, env.string, env.table = math, string, table
  env.tostring, env.tonumber = tostring, tonumber
  env.remembered = {}
  env.remember = function(k, v) env.remembered[k] = v; return v end
  env.recall = function(k, d) local v = env.remembered[k]; if v == nil then return d end; return v end
  env.j, env.maxj = 1, 1
  env.maxloop = function(n) env.maxj = n; return "" end
  env.fxgroup = {}
  env.relayer = function(layer) env.__out.layer = layer; return "" end
  env.retime = function(mode, a, b)
    a, b = a or 0, b or 0
    local l, s, o = env.line, env.syl, env.__out
    local ls, le = l.start_time, l.end_time
    mode = mode:lower()
    if mode == "set" or mode == "abs" then o.start_time, o.end_time = a, b
    elseif mode == "line" then o.start_time, o.end_time = ls + a, le + b
    elseif mode == "preline" then o.start_time, o.end_time = ls + a, ls + b
    elseif mode == "postline" then o.start_time, o.end_time = le + a, le + b
    elseif s then
      if mode == "syl" then o.start_time, o.end_time = ls + s.start_time + a, ls + s.end_time + b
      elseif mode == "presyl" then o.start_time, o.end_time = ls + s.start_time + a, ls + s.start_time + b
      elseif mode == "postsyl" then o.start_time, o.end_time = ls + s.end_time + a, ls + s.end_time + b
      elseif mode == "start2syl" then o.start_time, o.end_time = ls + a, ls + s.start_time + b
      elseif mode == "syl2end" then o.start_time, o.end_time = ls + s.end_time + a, le + b
      elseif mode == "sylpct" then
        o.start_time = ls + s.start_time + math.floor(s.duration * a / 100)
        o.end_time = ls + s.start_time + math.floor(s.duration * b / 100)
      else error("retime: unknown mode " .. mode) end
    else error("retime: mode '" .. mode .. "' needs a syllable") end
    return ""
  end
  return env
end

local function run_chunk(env, code, name)
  local fn, err = loadstring("return (" .. code .. "\n)", name)
  if not fn then fn, err = loadstring(code, name) end
  if not fn then error("template code error: " .. tostring(err) .. " in `" .. code .. "`") end
  setfenv(fn, env)
  return fn()
end

local OBJ_FIELDS = {
  start = function(o, kind) return kind == "line" and o.start_time or o.start_time end,
  ["end"] = function(o) return o.end_time end,
  mid = function(o, kind) if kind == "line" then return (o.start_time + o.end_time) / 2 end return (o.start_time + o.end_time) / 2 end,
  dur = function(o, kind) return kind == "line" and (o.end_time - o.start_time) or o.duration end,
  kdur = function(o, kind) local d = (kind == "line") and (o.end_time - o.start_time) or o.duration; return d / 10 end,
  i = function(o) return o.i end,
  left = function(o) return o.left end, center = function(o) return o.center end, right = function(o) return o.right end,
  top = function(o) return o.top end, middle = function(o) return o.middle end, bottom = function(o) return o.bottom end,
  x = function(o) return o.x end, y = function(o) return o.y end,
  width = function(o) return o.width end, height = function(o) return o.height end,
}

local function varvalue(env, name, cur_kind)
  local prefixes = { l = "line", s = "syl", c = "char", f = "furi" }
  local special = {
    syln = function() return env.line.kara.n end, layer = function() return env.__out.layer end,
    style = function() return env.line.style end, actor = function() return env.line.actor end,
    lastsyl = function() return env.line.kara.n end, j = function() return env.j end, maxj = function() return env.maxj end,
  }
  if special[name] then return special[name]() end
  local first, rest = name:sub(1, 1), name:sub(2)
  if prefixes[first] and OBJ_FIELDS[rest] then
    local kind = prefixes[first]
    local obj = env[kind]
    if obj then return OBJ_FIELDS[rest](obj, kind) end
  end
  if OBJ_FIELDS[name] then
    local obj = env[cur_kind]
    if obj then return OBJ_FIELDS[name](obj, cur_kind) end
  end
  return nil
end

local function substitute(env, text, cur_kind)
  -- $variables first, then !code!
  text = text:gsub("%$([%a_][%w_]*)", function(name)
    local v = varvalue(env, name, cur_kind)
    if v == nil then return "$" .. name end
    return fmtnum(v)
  end)
  text = text:gsub("!(.-)!", function(code)
    local r = run_chunk(env, code, "template code")
    if r == nil then return "" end
    if type(r) == "number" then return fmtnum(r) end
    return tostring(r)
  end)
  return text
end

local function is_blank(syl)
  return syl.text_stripped:match("^%s*$") ~= nil
end

local function apply_template(env, out, tmpl, kind, obj, base_line, text_of)
  env[kind] = obj
  env.j, env.maxj = 1, tmpl.loop
  while env.j <= env.maxj do
    local o = { layer = tmpl.layer, start_time = base_line.start_time, end_time = base_line.end_time }
    env.__out = o
    local txt = substitute(env, tmpl.text, kind)
    if not tmpl.mods.notext then txt = txt .. text_of end
    local nl = util.copy(base_line)
    nl.class = "dialogue"
    nl.comment = false
    nl.effect = "fx"
    nl.layer = o.layer
    nl.start_time, nl.end_time = o.start_time, o.end_time
    nl.text = txt
    -- strip karaskel working fields so they do not leak into the file
    for _, k in ipairs { "kara", "styleref", "text_stripped", "furi", "width", "height", "descent", "extlead", "margin_v", "eff_margin_l", "eff_margin_r", "eff_margin_t", "eff_margin_b", "eff_margin_v", "halign", "valign", "left", "center", "right", "top", "middle", "bottom", "x", "y", "duration" } do nl[k] = nil end
    if nl.end_time > nl.start_time or tmpl.mods.all then out[#out + 1] = nl end
    env.j = env.j + 1
  end
end

local function syl_matches(tmpl, syl)
  if tmpl.fx then return syl.fx == tmpl.fx end
  if syl.fx and not tmpl.mods.all then return false end
  return true
end

local function char_objects(line, syl)
  local chars, n = {}, 0
  local text = syl.text_stripped
  local total = 0
  for _ in text:gmatch("[%z\1-\127\194-\244][\128-\191]*") do total = total + 1 end
  local idx = 0
  for ch in text:gmatch("[%z\1-\127\194-\244][\128-\191]*") do
    idx = idx + 1
    local w = aegisub.text_extents(line.styleref, ch)
    local c = { text = ch, text_stripped = ch, i = idx, width = w, height = line.height, syl = syl }
    c.duration = syl.duration / total
    c.start_time = syl.start_time + (idx - 1) * c.duration
    c.end_time = c.start_time + c.duration
    chars[idx] = c
  end
  -- positions
  local x = syl.left
  for _, c in ipairs(chars) do
    c.left = x; c.center = x + c.width / 2; c.right = x + c.width
    c.top, c.middle, c.bottom = syl.top, syl.middle, syl.bottom
    local ha = line.halign
    c.x = (ha == "left") and c.left or ((ha == "right") and c.right or c.center)
    c.y = syl.y
    x = x + c.width
  end
  return chars
end

local function apply_karaoke_template(subs, sel)
  local meta, styles = karaskel.collect_head(subs, false)
  local env = new_env(subs, meta, styles)
  local templates = { once = {}, ["pre-line"] = {}, line = {}, syl = {}, char = {}, furi = {}, ["post-line"] = {} }
  local code = { once = {}, line = {}, syl = {}, char = {}, furi = {} }
  local karaoke_idx = {}
  local fx_idx = {}
  for i = 1, #subs do
    local l = subs[i]
    if l.class == "dialogue" then
      if l.comment then
        local t = parse_effect(l.effect)
        if t then
          t.text, t.style, t.layer, t.actor = l.text, l.style, l.layer, l.actor
          if t.kind == "code" then
            if code[t.type] then table.insert(code[t.type], t) end
          else
            if templates[t.type] then table.insert(templates[t.type], t) end
          end
        elseif l.effect:lower():match("^karaoke") then
          karaoke_idx[#karaoke_idx + 1] = i
        end
      elseif l.effect == "fx" then
        fx_idx[#fx_idx + 1] = i
      end
    end
  end
  -- which karaoke lines to process: selected ones if the selection hits any, else all
  local selset = {}
  for _, i in ipairs(sel or {}) do selset[i] = true end
  local targets = {}
  for _, i in ipairs(karaoke_idx) do if next(selset) == nil or selset[i] then targets[#targets + 1] = i end end

  -- remove previously generated lines
  for k = #fx_idx, 1, -1 do subs.delete(fx_idx[k]) end
  -- indices shift after deletions: recompute targets
  if #fx_idx > 0 then
    local shift_targets = {}
    for _, ti in ipairs(targets) do
      local removed = 0
      for _, fi in ipairs(fx_idx) do if fi < ti then removed = removed + 1 end end
      shift_targets[#shift_targets + 1] = ti - removed
    end
    targets = shift_targets
  end

  for _, t in ipairs(code.once) do env.line, env.syl = nil, nil; run_chunk(env, t.text, "code once") end

  local out = {}
  local function style_ok(t, l) return t.style == l.style end
  for _, ti in ipairs(targets) do
    local l = subs[ti]
    karaskel.preproc_line(subs, meta, styles, l)
    env.line, env.orgline, env.syl, env.char, env.furi = l, l, nil, nil, nil
    for _, t in ipairs(code.line) do if style_ok(t, l) then env.__out = {}; run_chunk(env, t.text, "code line") end end
    for _, tm in ipairs(templates["pre-line"]) do
      if style_ok(tm, l) then apply_template(env, out, tm, "line", l, l, "") end
    end
    for si = 1, l.kara.n do
      local syl = l.kara[si]
      local fxname = syl.tags and syl.tags:match("\\%-([%w_]+)")
      syl.fx = fxname
      env.syl, env.char = syl, nil
      env.__out = {}
      for _, t in ipairs(code.syl) do if style_ok(t, l) then run_chunk(env, t.text, "code syl") end end
      for _, tm in ipairs(templates.syl) do
        if style_ok(tm, l) and syl_matches(tm, syl) and (tm.mods.all or not is_blank(syl)) then
          local text = tm.mods.keeptags and syl.text or syl.text_stripped
          if tm.mods.noblank and is_blank(syl) then text = nil end
          if text then apply_template(env, out, tm, "syl", syl, l, text) end
        end
      end
      if #templates.char > 0 then
        for _, c in ipairs(char_objects(l, syl)) do
          env.char = c
          for _, t in ipairs(code.char) do if style_ok(t, l) then env.__out = {}; run_chunk(env, t.text, "code char") end end
          for _, tm in ipairs(templates.char) do
            if style_ok(tm, l) and syl_matches(tm, syl) and (tm.mods.all or not is_blank(syl)) then
              apply_template(env, out, tm, "char", c, l, c.text)
            end
          end
        end
        env.char = nil
      end
    end
    env.syl = nil
    for _, tm in ipairs(templates.line) do
      if style_ok(tm, l) then
        apply_template(env, out, tm, "line", l, l, tm.mods.keeptags and l.text or l.text_stripped)
      end
    end
    for _, tm in ipairs(templates["post-line"]) do
      if style_ok(tm, l) then apply_template(env, out, tm, "line", l, l, "") end
    end
  end
  for _, nl in ipairs(out) do subs.append(nl) end
  aegisub.debug.out("Karaoke templater: generated %d line(s) from %d karaoke line(s)", #out, #targets)
  return sel
end

local function validate(subs, sel)
  for i = 1, #subs do
    local l = subs[i]
    if l.class == "dialogue" and l.comment and l.effect:lower():match("^karaoke") then return true end
  end
  return false
end

aegisub.register_macro("Apply karaoke template", "Applies karaoke effect templates to the selected karaoke lines", apply_karaoke_template, validate)
