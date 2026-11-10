export script_name = "Moon örneği"
export script_description = "MoonScript ile yazılmış makro"

class Sayac
  new: (@n = 0) =>
  artir: => @n += 1

make_upper = (subs, sel) ->
  c = Sayac!
  for i in *sel
    line = subs[i]
    line.text = line.text\upper!
    subs[i] = line
    c\artir!
  aegisub.log "degisti=%d\n", c.n
  sel

aegisub.register_macro script_name, script_description, make_upper
