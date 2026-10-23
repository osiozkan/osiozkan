script_name = "Demo"
script_description = "Exercises the Aegisub API shim"
include("utils.lua")
include("karaskel.lua")

local function shift(subs, sel)
  for _, i in ipairs(sel) do
    local l = subs[i]
    l.start_time = l.start_time + 1000
    l.end_time = l.end_time + 1000
    subs[i] = l
  end
end

local function split_syl(subs, sel)
  local meta, styles = karaskel.collect_head(subs)
  local out = 0
  for k = #sel, 1, -1 do
    local i = sel[k]
    local l = subs[i]
    karaskel.preproc_line(subs, meta, styles, l)
    if l.kara then
      local at = i
      for _, syl in ipairs(l.kara) do
        local n = table.copy(l)
        n.start_time = l.start_time + syl.start_time
        n.end_time = n.start_time + syl.duration
        n.text = syl.text_stripped
        n.layer = 1
        at = at + 1
        subs.insert(at, n)
        out = out + 1
      end
      l.comment = true
      subs[i] = l
    end
  end
  aegisub.log("created %d syllable lines\n", out)
  return sel
end

aegisub.register_macro("Shift +1s", "Shift selected lines", shift)
aegisub.register_macro("Split karaoke", "Split into syllables", split_syl)
