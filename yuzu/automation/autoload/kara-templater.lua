-- Karaoke templater for Yuzu (Aegisub 3.x "kara-templater.lua" compatible re-implementation).
--
-- Template lines are Comment lines whose Effect field starts with `code`, `template` or `mixed`.
-- Karaoke lines are lines whose Effect is `karaoke`.  See automation/README.md for the syntax.

script_name = "Karaoke templater"
script_description = "Macro and export filter to apply karaoke effect templates to a script"
script_author = "Yuzu (after Aegisub's kara-templater)"
script_version = "3"

local tr = aegisub.gettext
local karaskel = require "karaskel"
require "utils"
local unicode = require "unicode"
local util = aegisub.util

local unpack = unpack or table.unpack

------------------------------------------------------------------------------------------
-- small helpers
------------------------------------------------------------------------------------------

local function fmt_num(v)
  if v ~= v or v == math.huge or v == -math.huge then return "0" end
  if v == math.floor(v) and math.abs(v) < 1e15 then return string.format("%d", v) end
  local s = string.format("%.3f", v)
  s = s:gsub("0+$", ""):gsub("%.$", "")
  if s == "-0" then s = "0" end
  return s
end

local function fmt_val(v)
  local t = type(v)
  if t == "number" then return fmt_num(v)
  elseif t == "nil" then return ""
  else return tostring(v) end
end

local function round(x) return math.floor(x + 0.5) end

local function format_time(ms)
  local cs = round(ms / 10)
  local neg = cs < 0
  if neg then cs = -cs end
  local s = string.format("%d:%02d:%02d.%02d", math.floor(cs / 360000), math.floor(cs / 6000) % 60, math.floor(cs / 100) % 60, cs % 100)
  return neg and ("-" .. s) or s
end

-- remove \k \kf \ko \K tags (and \-fx markers) from the override blocks of a text
local function strip_k(text)
  return (text:gsub("%b{}", function(b)
    local inner = b:sub(2, -2)
    inner = inner:gsub("\\[kK][fo]?%-?%d+%.?%d*", ""):gsub("\\%-[%w_]+", "")
    if inner == "" then return "" end
    return "{" .. inner .. "}"
  end))
end

local function trim(s) return (s:gsub("^%s+", ""):gsub("%s+$", "")) end

-- split on whitespace, but keep !expression! tokens (which may contain spaces) whole
local function tokenize(s)
  local toks, pos = {}, 1
  while true do
    local a = s:find("%S", pos)
    if not a then break end
    if s:sub(a, a) == "!" then
      local b = s:find("!", a + 1, true)
      if b then
        toks[#toks + 1] = s:sub(a, b)
        pos = b + 1
      else
        toks[#toks + 1] = s:sub(a)
        break
      end
    else
      local b = s:find("%s", a) or (#s + 1)
      toks[#toks + 1] = s:sub(a, b - 1)
      pos = b
    end
  end
  return toks
end

local function linedesc(l, idx)
  return string.format("line %s (%s)", tostring(idx), l.effect)
end

------------------------------------------------------------------------------------------
-- the template environment
------------------------------------------------------------------------------------------

local function make_env(meta, styles)
  local env = {}
  local varctx = {}
  local memory = {}
  setmetatable(env, { __index = _G })

  env.meta = meta
  env.styles = styles
  env.util = util
  env.unicode = unicode
  env.aegisub = aegisub
  env.tenv = env
  env.varctx = varctx
  env.fxgroup = {}
  env.format_time = format_time
  env.fmt = fmt_num
  env.j, env.maxj, env.i, env.maxi = 1, 1, 1, 1

  env.maxloop = function(n)
    env.maxj = n
    return ""
  end
  env.maxloops = env.maxloop

  env.relayer = function(layer)
    env.line.layer = layer
    return ""
  end

  env.remember = function(name, value)
    memory[name] = value
    return value
  end
  env.remember_if = function(name, value, cond)
    if cond then memory[name] = value end
    return value
  end
  env.recall = function(name, default)
    local v = memory[name]
    if v == nil then return default end
    return v
  end

  env.retime = function(mode, addstart, addend)
    local line, org, syl = env.line, env.orgline, env.syl
    addstart = addstart or 0
    addend = addend or 0
    local ls, le = org.start_time, org.end_time
    local function need()
      if not syl then error("retime(\"" .. tostring(mode) .. "\"): no syllable in this context", 3) end
    end
    local ns, ne
    if mode == "syl" then
      need(); ns, ne = ls + syl.start_time + addstart, ls + syl.end_time + addend
    elseif mode == "presyl" then
      need(); ns, ne = ls + syl.start_time + addstart, ls + syl.start_time + addend
    elseif mode == "postsyl" then
      need(); ns, ne = ls + syl.end_time + addstart, ls + syl.end_time + addend
    elseif mode == "line" then
      ns, ne = ls + addstart, le + addend
    elseif mode == "preline" then
      ns, ne = ls + addstart, ls + addend
    elseif mode == "postline" then
      ns, ne = le + addstart, le + addend
    elseif mode == "start2syl" then
      need(); ns, ne = ls + addstart, ls + syl.start_time + addend
    elseif mode == "syl2end" then
      need(); ns, ne = ls + syl.end_time + addstart, le + addend
    elseif mode == "set" or mode == "abs" then
      ns, ne = addstart, addend
    elseif mode == "sylpct" then
      need(); ns = ls + syl.start_time + addstart * syl.duration / 100
      ne = ls + syl.start_time + addend * syl.duration / 100
    else
      error("retime: unknown mode \"" .. tostring(mode) .. "\"", 2)
    end
    line.start_time = round(ns)
    line.end_time = round(ne)
    line.duration = line.end_time - line.start_time
    return ""
  end

  return env, varctx
end

------------------------------------------------------------------------------------------
-- compiling expressions and template text
------------------------------------------------------------------------------------------

local function compile_expr(src, env, name)
  src = src:gsub("%$([%a_][%w_]*)", 'varctx["%1"]')
  local f, err = loadstring("return (" .. src .. ")", name)
  if not f then
    local f2 = loadstring(src, name)
    if not f2 then error(name .. ": syntax error in expression `" .. src .. "`: " .. tostring(err), 0) end
    f = f2
  end
  setfenv(f, env)
  return f
end

-- Split template text into literal / !expr! / $var parts.
local function parse_text(text, env, name)
  local parts = {}
  local lit = {}
  local function flush()
    if #lit > 0 then parts[#parts + 1] = { "lit", table.concat(lit) }; lit = {} end
  end
  local pos, n = 1, #text
  while pos <= n do
    local c = text:sub(pos, pos)
    if c == "!" then
      -- find the closing "!" (skipping quoted strings)
      local q, p = nil, pos + 1
      local close
      while p <= n do
        local ch = text:sub(p, p)
        if q then
          if ch == "\\" then p = p + 1 elseif ch == q then q = nil end
        elseif ch == '"' or ch == "'" then q = ch
        elseif ch == "!" then close = p; break end
        p = p + 1
      end
      if close then
        flush()
        local src = text:sub(pos + 1, close - 1)
        parts[#parts + 1] = { "expr", compile_expr(src, env, name), src }
        pos = close + 1
      else
        lit[#lit + 1] = c; pos = pos + 1
      end
    elseif c == "$" then
      local nm = text:match("^%$([%a_][%w_]*)", pos)
      if nm then
        flush()
        parts[#parts + 1] = { "var", nm }
        pos = pos + 1 + #nm
      else
        lit[#lit + 1] = c; pos = pos + 1
      end
    else
      local nxt = text:find("[!$]", pos + 1) or (n + 1)
      lit[#lit + 1] = text:sub(pos, nxt - 1)
      pos = nxt
    end
  end
  flush()
  return parts
end

------------------------------------------------------------------------------------------
-- parsing template / code lines
------------------------------------------------------------------------------------------

local TEMPLATE_TYPES = { ["pre-line"] = true, line = true, syl = true, furi = true, char = true, all = true }
local CODE_TYPES = { once = true, line = true, syl = true, furi = true, char = true, word = true, all = true }

local function parse_item(l, idx, env)
  local toks = tokenize(l.effect)
  local cmd = toks[1]:lower()
  local item = {
    kind = (cmd == "code") and "code" or "template",
    cmd = cmd,
    idx = idx,
    desc = linedesc(l, idx),
    style = l.style,
    actor = l.actor,
    layer = l.layer,
    loops = 1,
    text = l.text,
    notext = false, keeptags = false, noblank = false, multi = false,
    fx = nil, fxgroup = nil, conds = {},
  }
  local ti = 2
  if cmd == "mixed" then
    item.type = "syl"
    item.multi = true
  else
    local ty = toks[2] and toks[2]:lower()
    local valid = (cmd == "code") and CODE_TYPES or TEMPLATE_TYPES
    if ty and valid[ty] then
      item.type = ty
      ti = 3
    elseif ty and cmd == "code" then
      error(item.desc .. ": unknown code type `" .. ty .. "`", 0)
    else
      error(item.desc .. ": missing or unknown " .. cmd .. " type `" .. tostring(toks[2]) .. "`", 0)
    end
    if item.type == "all" and cmd == "template" then
      item.type = "syl"; item.style = nil
    elseif item.type == "char" then
      item.type = "syl"; item.perchar = true
    end
  end

  local function nexttok(what)
    local v = toks[ti]
    if v == nil then error(item.desc .. ": modifier `" .. what .. "` needs an argument", 0) end
    ti = ti + 1
    return v
  end
  while ti <= #toks do
    local m = toks[ti]; ti = ti + 1
    local ml = m:lower()
    if ml == "all" then item.style = nil
    elseif ml == "noblank" or ml == "no-blank" then item.noblank = true
    elseif ml == "repeat" or ml == "loop" then
      local v = tonumber(nexttok(ml))
      if not v then error(item.desc .. ": `" .. ml .. "` needs a number", 0) end
      item.loops = v
    elseif ml == "fxgroup" then item.fxgroup = nexttok(ml)
    elseif ml == "fx" then item.fx = nexttok(ml)
    elseif ml == "multi" then item.multi = true
    elseif ml == "char" then item.perchar = true
    elseif ml == "furi" then item.type = "furi"
    elseif ml == "notext" then item.notext = true
    elseif ml == "keeptags" then item.keeptags = true
    elseif ml == "style" then item.style = nexttok(ml)
    elseif ml == "layer" then item.layer = tonumber(nexttok(ml)) or item.layer
    elseif ml == "if" or ml == "unless" then
      local src = nexttok(ml)
      if src:sub(1, 1) == "!" and src:sub(-1) == "!" and #src >= 2 then src = src:sub(2, -2) end
      item.conds[#item.conds + 1] = { negate = (ml == "unless"), fn = compile_expr(src, env, item.desc), src = src }
    elseif ml == "retime" or ml == "strip" then
      -- accepted for compatibility, nothing to do
    else
      error(item.desc .. ": unknown modifier `" .. m .. "`", 0)
    end
  end

  if item.kind == "code" then
    local chunk, err = loadstring(l.text, "=" .. item.desc)
    if not chunk then error(item.desc .. ": " .. tostring(err), 0) end
    setfenv(chunk, env)
    item.fn = chunk
  else
    item.parts = parse_text(l.text, env, item.desc)
  end
  return item
end

local function parse_templates(subs, env)
  local T = { once = {}, pre = {}, line = {}, syl = {}, char = {}, furi = {}, n = 0 }
  for i = 1, #subs do
    local l = subs[i]
    if l.class == "dialogue" and l.comment and l.effect ~= "" then
      local head = l.effect:match("^%s*(%S+)")
      head = head and head:lower()
      if head == "code" or head == "template" or head == "mixed" then
        local it = parse_item(l, i, env)
        T.n = T.n + 1
        if it.kind == "code" then
          local ty = it.type
          if ty == "once" then T.once[#T.once + 1] = it
          elseif ty == "word" then
            aegisub.debug.out(2, "Karaoke templater: %s: `code word` is not supported, ignored\n", it.desc)
          elseif ty == "all" then
            it.style = nil
            for _, k in ipairs({ "line", "syl", "char", "furi" }) do T[k][#T[k] + 1] = it end
          else T[ty][#T[ty] + 1] = it end
        else
          local ty = it.type
          if ty == "pre-line" then T.pre[#T.pre + 1] = it
          elseif ty == "syl" and it.perchar then T.char[#T.char + 1] = it
          else T[ty][#T[ty] + 1] = it end
        end
      end
    end
  end
  return T
end

------------------------------------------------------------------------------------------
-- applying templates to one karaoke line
------------------------------------------------------------------------------------------

local OUT_FIELDS = { "layer", "start_time", "end_time", "style", "actor", "text", "margin_l", "margin_r", "margin_t", "margin_b" }

local function apply_templates(subs)
  local meta, styles = karaskel.collect_head(subs, true)
  local env, varctx = make_env(meta, styles)
  local T = parse_templates(subs, env)

  -- throw away the output of previous runs
  for i = #subs, 1, -1 do
    local l = subs[i]
    if l.class == "dialogue" and not l.comment and l.effect == "fx" then subs.delete(i) end
  end

  if T.n == 0 then
    aegisub.debug.out(2, "Karaoke templater: no template or code lines found\n")
  end

  -- locate karaoke lines
  local klines = {}
  for i = 1, #subs do
    local l = subs[i]
    if l.class == "dialogue" and l.effect == "karaoke" then klines[#klines + 1] = i end
  end

  local function run_code(it)
    local ok, err = pcall(it.fn)
    if not ok then error("Error in " .. it.desc .. ": " .. tostring(err), 0) end
  end

  for _, it in ipairs(T.once) do
    env.orgline, env.line, env.syl, env.basesyl, env.char = nil, nil, nil, nil, nil
    run_code(it)
  end

  local offset = 0
  local nk = #klines
  for ki, idx in ipairs(klines) do
    if aegisub.progress.is_cancelled() then aegisub.cancel() end
    aegisub.progress.set((ki - 1) / nk * 100)
    local pos = idx + offset
    local pristine = subs[pos]
    local L = util.copy(pristine)
    karaskel.preproc_line(subs, meta, styles, L)
    local outputs = {}

    -- ---- per line state
    env.orgline = L
    env.li = ki
    env.syl, env.basesyl, env.char = nil, nil, nil
    env.i, env.maxi, env.j, env.maxj = 1, L.kara.n, 1, 1

    local function new_line(t)
      local line = util.copy(L)
      line.comment = false
      line.effect = "fx"
      line.layer = t.layer
      line.text = ""
      line.duration = line.end_time - line.start_time
      return line
    end

    local function set_line_vars()
      local V = varctx
      for k in pairs(V) do V[k] = nil end
      V.li = ki
      V.lstart, V.lend, V.ldur = L.start_time, L.end_time, L.duration
      V.lmid = L.start_time + L.duration / 2
      V.lkdur = L.duration / 10
      V.start, V["end"], V.dur, V.kdur = 0, L.duration, L.duration, L.duration / 10
      V.mid = L.duration / 2
      V.sstart, V.send, V.sdur, V.skdur, V.smid = V.start, V["end"], V.dur, V.kdur, V.mid
      V.left, V.center, V.right, V.width = L.left, L.center, L.right, L.width
      V.top, V.middle, V.bottom, V.height = L.top, L.middle, L.bottom, L.height
      V.x, V.y = L.x, L.y
      V.lleft, V.lcenter, V.lright, V.lwidth = L.left, L.center, L.right, L.width
      V.ltop, V.lmiddle, V.lbottom, V.lheight = L.top, L.middle, L.bottom, L.height
      V.lx, V.ly = L.x, L.y
      V.layer, V.actor, V.style, V.effect = L.layer, L.actor, L.style, L.effect
      V.margin_l, V.margin_r, V.margin_t, V.margin_b, V.margin_v = L.margin_l, L.margin_r, L.margin_t, L.margin_b, L.margin_t
      V.syln = L.kara.n
      V.furin = L.furi and L.furi.n or 0
      V.si, V.ci, V.cn = 1, 1, 1
      V.j, V.maxj = 1, 1
    end

    local function set_syl_vars(S, base, ci, cn)
      local V = varctx
      V.sstart, V.send, V.sdur = S.start_time, S.end_time, S.duration
      V.smid = S.start_time + S.duration / 2
      V.skdur = S.duration / 10
      V.start, V["end"], V.mid, V.dur, V.kdur = V.sstart, V.send, V.smid, V.sdur, V.skdur
      V.left, V.center, V.right, V.width, V.height = S.left, S.center, S.right, S.width, S.height
      V.sleft, V.scenter, V.sright, V.swidth, V.sheight = S.left, S.center, S.right, S.width, S.height
      V.top, V.middle, V.bottom = S.top or L.top, S.middle or L.middle, S.bottom or L.bottom
      V.stop, V.smiddle, V.sbottom = V.top, V.middle, V.bottom
      V.x = (L.halign == "left" and S.left) or (L.halign == "right" and S.right) or S.center
      V.y = (L.valign == "top" and V.top) or (L.valign == "bottom" and V.bottom) or V.middle
      V.sx, V.sy = V.x, V.y
      V.si = (base or S).i
      V.ci, V.cn = ci or 1, cn or 1
    end

    local function applies(t, S)
      if t.style and t.style ~= L.style then
        if not (t.type == "furi" and t.style == L.style .. "-furigana") then return false end
      end
      if t.actor ~= "" and t.actor ~= L.actor then return false end
      if t.fxgroup and not env.fxgroup[t.fxgroup] then return false end
      if S then
        if t.fx and t.fx ~= S.inline_fx then return false end
        if t.noblank and (S.duration == 0 or S.text_spacestripped == "") then return false end
      end
      for _, c in ipairs(t.conds) do
        local ok, v = pcall(c.fn)
        if not ok then error("Error in condition of " .. t.desc .. ": " .. tostring(v), 0) end
        if (not v) == (not c.negate) then return false end
      end
      return true
    end

    local function eval_parts(t)
      local out = {}
      for _, p in ipairs(t.parts) do
        if p[1] == "lit" then out[#out + 1] = p[2]
        elseif p[1] == "var" then
          local v = varctx[p[2]]
          if v == nil then v = env[p[2]] end
          if v == nil then error("Error in " .. t.desc .. ": unknown variable $" .. p[2], 0) end
          out[#out + 1] = fmt_val(v)
        else
          local ok, v = pcall(p[2])
          if not ok then error("Error in " .. t.desc .. " (expression `" .. p[3] .. "`): " .. tostring(v), 0) end
          out[#out + 1] = fmt_val(v)
        end
      end
      return table.concat(out)
    end

    local function tail_text(t, kind, S)
      if t.notext then return "" end
      if kind == "line" then return strip_k(L.text) end
      if kind == "syl" then
        if t.keeptags then return trim(strip_k(S.text)) end
        return trim(S.text_stripped)
      end
      if kind == "char" then return S.text end
      return trim(S.text_stripped or S.text)
    end

    local function pre_text()
      local out = {}
      for _, p in ipairs(T.pre) do
        if applies(p, nil) then out[#out + 1] = eval_parts(p) end
      end
      return table.concat(out)
    end

    local function finish(line, t, kind)
      line.start_time = round(line.start_time)
      line.end_time = round(line.end_time)
      if line.end_time <= line.start_time then return end
      local out = util.copy(pristine)
      out.comment = false
      out.effect = "fx"
      for _, f in ipairs(OUT_FIELDS) do
        local v = line[f]
        -- margins computed by karaskel stay the original (possibly 0 = "use style") ones
        if f:sub(1, 7) == "margin_" and v == L[f] then v = pristine[f] end
        out[f] = v
      end
      if kind == "furi" and t.style and t.style:match("%-furigana$") and styles[t.style] then
        out.style = t.style
      end
      if out.text == nil then out.text = "" end
      outputs[#outputs + 1] = out
    end

    local multi_buf, multi_order = {}, {}

    -- produce output for template t in context kind with syllable S
    local function emit(t, kind, S)
      env.j = 1
      env.maxj = t.loops
      local guard = 0
      while env.j <= env.maxj do
        guard = guard + 1
        if guard > 100000 then error(t.desc .. ": loop runaway", 0) end
        varctx.j, varctx.maxj = env.j, env.maxj
        local line = new_line(t)
        env.line = line
        local pre = pre_text()
        local body = eval_parts(t)
        local tail = tail_text(t, kind, S)
        if t.multi then
          local per = multi_buf[t]
          if not per then per = {}; multi_buf[t] = per end
          local b = per[env.j]
          if not b then
            b = { line = line, pieces = { pre }, kind = kind }
            per[env.j] = b
            multi_order[#multi_order + 1] = b
          else
            if line.start_time < b.line.start_time then b.line.start_time = line.start_time end
            if line.end_time > b.line.end_time then b.line.end_time = line.end_time end
          end
          b.pieces[#b.pieces + 1] = body .. tail
          b.t = t
        else
          line.text = pre .. body .. tail
          finish(line, t, kind)
        end
        env.j = env.j + 1
      end
      env.j, env.maxj = 1, 1
    end

    local function run_items(items, kind, S)
      for _, t in ipairs(items) do
        if applies(t, S) then
          if t.kind == "code" then
            env.line = new_line(t)
            run_code(t)
          else
            emit(t, kind, S)
          end
        end
      end
    end

    -- ---- line phase
    set_line_vars()
    env.line = util.copy(L)
    run_items(T.line, "line", nil)

    -- ---- syllable (and char) phase
    for si = 1, L.kara.n do
      local S = L.kara[si]
      env.syl, env.basesyl, env.char = S, S, nil
      env.i = si
      set_syl_vars(S)
      run_items(T.syl, "syl", S)

      if #T.char > 0 then
        local text = S.text_spacestripped
        local chars = {}
        for c in unicode.chars(text) do chars[#chars + 1] = c end
        local n = #chars
        local prefix = ""
        for ci, c in ipairs(chars) do
          local w0 = (prefix == "") and 0 or (aegisub.text_extents(L.styleref, prefix))
          local w1 = aegisub.text_extents(L.styleref, prefix .. c)
          local C = {
            i = ci, n = n, text = c, text_stripped = c, text_spacestripped = c,
            syl = S, inline_fx = S.inline_fx, ischar = true,
            start_time = S.start_time + math.floor((ci - 1) * S.duration / n),
            end_time = S.start_time + math.floor(ci * S.duration / n),
            width = w1 - w0, height = S.height,
            top = S.top, middle = S.middle, bottom = S.bottom,
          }
          C.duration = C.end_time - C.start_time
          C.kdur = C.duration / 10
          C.left = S.left + w0
          C.right = C.left + C.width
          C.center = C.left + C.width / 2
          prefix = prefix .. c
          env.syl, env.char, env.basesyl = C, C, S
          env.i = ci
          env.maxi = n
          set_syl_vars(C, S, ci, n)
          run_items(T.char, "char", C)
        end
        env.char = nil
        env.maxi = L.kara.n
      end
    end

    -- ---- furigana phase
    if L.furi and L.furi.n > 0 then
      for fi = 1, L.furi.n do
        local F = L.furi[fi]
        env.syl, env.basesyl, env.char = F, F.syl, nil
        env.i = fi
        env.maxi = L.furi.n
        set_syl_vars(F, F.syl, 1, 1)
        run_items(T.furi, "furi", F)
      end
    end

    -- ---- flush combined (multi) lines
    for _, b in ipairs(multi_order) do
      if b.pieces then
        b.line.text = table.concat(b.pieces)
        finish(b.line, b.t, b.kind)
      end
    end

    -- ---- write back: comment out the original, add the generated lines after it
    if #outputs > 0 then
      pristine.comment = true
      pristine.effect = "karaoke"
      subs[pos] = pristine
      local at = pos + 1
      local CH = 200
      for a = 1, #outputs, CH do
        local chunk = {}
        for k = a, math.min(a + CH - 1, #outputs) do chunk[#chunk + 1] = outputs[k] end
        subs.insert(at, unpack(chunk))
        at = at + #chunk
      end
      offset = offset + #outputs
    end
  end
  aegisub.progress.set(100)
end

------------------------------------------------------------------------------------------
-- registration
------------------------------------------------------------------------------------------

local function has_karaoke(subs)
  for i = 1, #subs do
    local l = subs[i]
    if l.class == "dialogue" and (l.effect == "karaoke" or (l.comment and l.effect:match("^template"))) then return true end
  end
  return false
end

local function macro_apply_templates(subs, sel)
  apply_templates(subs)
end

local function macro_can_template(subs)
  return has_karaoke(subs)
end

local function filter_apply_templates(subs, config)
  apply_templates(subs)
end

aegisub.register_macro(tr "Apply karaoke template", tr "Applies karaoke effect templates to the script", macro_apply_templates, macro_can_template)
aegisub.register_filter(tr "Karaoke template", tr "Apply karaoke effect templates to the script", 0, filter_apply_templates)
