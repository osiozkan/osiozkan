script_name = "Kötü kullanım"
aegisub.register_macro("kotu", "", function(subs, sel)
  local tests = {
    function() subs[1.5] = {} end,
    function() subs["x"] = 1 end,
    function() subs[1] = 5 end,
    function() subs[1] = "str" end,
    function() subs[-100000] = {class = "dialogue", text = "x"} end,
    function() subs[100000] = {class = "dialogue", text = "x"} end,
    function() subs.insert(0, {}) end,
    function() subs.insert(100000, {class = "dialogue"}) end,
    function() subs.insert() end,
    function() subs.delete(100000, -5, 0) end,
    function() subs.deleterange(10, 2) end,
    function() subs.deleterange(-3, 100000) end,
    function() subs.append(1) end,
    function() subs.append({}) end,
    function() return subs[0] end,
    function() return subs[-1] end,
    function() return subs[1e300] end,
    function() return #subs end,
    function() aegisub.text_extents({}, "x") end,
    function() aegisub.text_extents({fontname = "Yok", fontsize = -5}, "x") end,
    function() aegisub.text_extents(nil, nil) end,
    function() aegisub.dialog.display({}, {}, {}) end,
    function() aegisub.dialog.display({{class = "yok"}, {class = "dropdown"}, {class = "intedit", min = 5, max = 1}}) end,
    function() aegisub.dialog.display({{class = "edit", name = "a", value = {}}}) end,
    function() aegisub.register_macro() end,
    function() aegisub.register_macro("a", "b", 5) end,
    function() aegisub.frame_from_ms("x") end,
    function() aegisub.decode_path(nil) end,
    function() aegisub.log(nil) end,
    function() aegisub.log(1, nil, nil) end,
    function() aegisub.log("%d %s", "x") end,
    function() require("re").match("a", "(") end,
    function() require("re").sub("a", "a", nil) end,
    function() require("re").find(nil, "a") end,
    function() require("unicode").len(nil) end,
    function() require("lfs").attributes(nil) end,
    function() require("lfs").dir("/nonexistent")() end,
    function() include("yok.lua") end,
    function() karaskel.collect_head(subs) end,
  }
  local errs = 0
  for i, t in ipairs(tests) do
    local ok, err = pcall(t)
    if not ok then errs = errs + 1 end
  end
  aegisub.log("hata=%d/%d", errs, #tests)
end)
