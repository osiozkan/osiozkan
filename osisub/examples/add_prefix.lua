script_name = "Add prefix"
script_description = "Prefix every selected dialogue line (dialog demo)"
script_author = "OsiSub"
script_version = "1"

function add_prefix(subs, sel)
  local btn, res = aegisub.dialog.display({
    { class = "label", label = "Prefix:", x = 0, y = 0 },
    { class = "edit", name = "p", value = "- ", x = 1, y = 0 },
    { class = "checkbox", name = "up", label = "Uppercase", value = false, x = 0, y = 1, width = 2 },
  }, { "Apply", "Cancel" })
  if btn ~= "Apply" then aegisub.cancel() end
  for _, i in ipairs(sel) do
    local l = subs[i]
    l.text = res.p .. (res.up and l.text:upper() or l.text)
    subs[i] = l
  end
  return sel
end
aegisub.register_macro("Add prefix", "Prefix lines", add_prefix)
