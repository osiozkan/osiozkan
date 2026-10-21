script_name="fd"
aegisub.register_macro("pick","p",function(subs,sel)
  local f=aegisub.dialog.open("Pick a file","","","*.*",false)
  aegisub.log("picked: %s\n",tostring(f))
  if not f then aegisub.cancel() end
  local l=subs[sel[1]]; l.effect=f; subs[sel[1]]=l
end)
