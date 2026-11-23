-- aegisub.gui (Aegisub 3.3+/arch1t3cht): imleç/seçim 1 tabanlı bayt, ayarlananlar getter'da görünür
script_name = "gui testi"
aegisub.register_macro("Gui", "", function(subs, sel, act)
    local a, b = aegisub.gui.get_selection()
    aegisub.log("cursor=%d sel=%d,%d mod=%s\n", aegisub.gui.get_cursor(), a, b, tostring(aegisub.gui.is_modified()))
    local line = subs[act]
    local s = line.text:find("ra", 1, true)
    aegisub.gui.set_selection(s + 2, s)
    local c, d = aegisub.gui.get_selection()
    aegisub.log("after=%d,%d cur=%d\n", c, d, aegisub.gui.get_cursor())
    local pp = aegisub.project_properties()
    aegisub.log("row=%s filt=%s\n", tostring(pp.active_row), type(pp.export_filters))
    aegisub.set_status_text("bitti")
end)
