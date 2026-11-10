export script_name = "Hatalı moon"

-- birkaç satır boşluk ve yorum: derlenmiş Lua satırları farklı olsun
class A
  new: => @x = 1

isle = (subs, sel) ->
  a = A!
  y = nil
  y.alan = 3
  sel

aegisub.register_macro script_name, "test", isle
