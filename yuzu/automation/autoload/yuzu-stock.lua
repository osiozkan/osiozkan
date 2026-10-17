script_name = "Yuzu stock macros"
script_description = "Aegisub-style helper macros: select overlaps, clean tags, strip tags, blur, full-width, number lines"
script_author = "Yuzu"
script_version = "1.0"

local function dialogue_lines(subs)
  local i = 0
  return function()
    while true do
      i = i + 1
      if i > #subs then return nil end
      if subs[i].class == "dialogue" then return i, subs[i] end
    end
  end
end

local function select_overlaps(subs, sel)
  local out, prev_end, prev_i = {}, nil, nil
  local list = {}
  for i, l in dialogue_lines(subs) do
    if not l.comment then list[#list + 1] = { i = i, s = l.start_time, e = l.end_time } end
  end
  table.sort(list, function(a, b) return a.s < b.s end)
  local seen = {}
  local max_end, max_i = -1, nil
  for _, x in ipairs(list) do
    if x.s < max_end then
      if not seen[x.i] then seen[x.i] = true; out[#out + 1] = x.i end
      if max_i and not seen[max_i] then seen[max_i] = true; out[#out + 1] = max_i end
    end
    if x.e > max_end then max_end = x.e; max_i = x.i end
  end
  table.sort(out)
  if #out == 0 then aegisub.debug.out("No overlapping lines found.\n") end
  return out
end

local function clean(text)
  text = text:gsub("}{", "")
  text = text:gsub("{}", "")
  return text
end

local function clean_tags(subs, sel)
  for _, i in ipairs(sel) do
    local l = subs[i]
    l.text = clean(l.text)
    subs[i] = l
  end
  return sel
end

local function strip_tags(subs, sel)
  for _, i in ipairs(sel) do
    local l = subs[i]
    l.text = l.text:gsub("%b{}", "")
    subs[i] = l
  end
  return sel
end

local function add_blur(subs, sel)
  local btn, res = aegisub.dialog.display({
    { class = "label", label = "Blur strength", x = 0, y = 0 },
    { class = "floatedit", name = "blur", value = 0.6, min = 0, max = 20, step = 0.1, x = 1, y = 0 },
  }, { "Apply", "Cancel" }, { ok = "Apply", cancel = "Cancel" })
  if btn ~= "Apply" then aegisub.cancel() end
  for _, i in ipairs(sel) do
    local l = subs[i]
    local tag = string.format("\\blur%g", res.blur)
    if l.text:sub(1, 1) == "{" then
      l.text = l.text:gsub("^{", "{" .. tag:gsub("%%", "%%%%"), 1)
    else
      l.text = "{" .. tag .. "}" .. l.text
    end
    subs[i] = l
  end
  return sel
end

local function fullwidth(subs, sel)
  local unicode = require "unicode"
  for _, i in ipairs(sel) do
    local l = subs[i]
    local out, depth = {}, 0
    for ch in unicode.chars(l.text) do
      if ch == "{" then depth = depth + 1 elseif ch == "}" then depth = math.max(0, depth - 1) end
      local cp = unicode.codepoint(ch)
      if depth == 0 and ch ~= "}" and cp >= 33 and cp <= 126 and ch ~= "\\" then ch = unicode.char(cp + 0xFEE0) end
      out[#out + 1] = ch
    end
    l.text = table.concat(out)
    subs[i] = l
  end
  return sel
end

local function number_lines(subs, sel)
  local n = 0
  for _, i in ipairs(sel) do
    n = n + 1
    local l = subs[i]
    l.effect = tostring(n)
    subs[i] = l
  end
  return sel
end

local function comment_selected(subs, sel)
  for _, i in ipairs(sel) do local l = subs[i]; l.comment = true; subs[i] = l end
  return sel
end

local function uncomment_selected(subs, sel)
  for _, i in ipairs(sel) do local l = subs[i]; l.comment = false; subs[i] = l end
  return sel
end

local function remove_empty(subs, sel)
  local del = {}
  for i, l in dialogue_lines(subs) do
    if l.text:gsub("%b{}", ""):match("^%s*$") and not l.comment then del[#del + 1] = i end
  end
  if #del > 0 then subs.delete(unpack(del)) end
  return {}
end

aegisub.register_macro("Yuzu/Select overlaps", "Select lines whose times overlap another line", select_overlaps)
aegisub.register_macro("Yuzu/Clean tags", "Merge adjacent override blocks and remove empty ones", clean_tags)
aegisub.register_macro("Yuzu/Strip tags", "Remove all override tags", strip_tags)
aegisub.register_macro("Yuzu/Add blur…", "Prefix \\blur to selected lines", add_blur)
aegisub.register_macro("Yuzu/Full-width Latin", "Convert ASCII text to full-width forms", fullwidth)
aegisub.register_macro("Yuzu/Number lines (effect)", "Write 1..n into the Effect field", number_lines)
aegisub.register_macro("Yuzu/Comment selected", "Turn selected lines into comments", comment_selected)
aegisub.register_macro("Yuzu/Uncomment selected", "Turn selected comments into dialogue", uncomment_selected)
aegisub.register_macro("Yuzu/Delete empty lines", "Remove dialogue lines without visible text", remove_empty)
