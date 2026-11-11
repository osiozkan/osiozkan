-- osisub: küçük JSON modülü (json.encode / json.decode). Kullanıcının include klasöründe
-- kendi json.lua'sı varsa o kullanılır; bu yalnızca yedektir.
local json = { _osisub = true }

local escapes = { ['"'] = '\\"', ['\\'] = '\\\\', ['\b'] = '\\b', ['\f'] = '\\f', ['\n'] = '\\n', ['\r'] = '\\r', ['\t'] = '\\t' }

local function is_array(t)
  local n = 0
  for k in pairs(t) do
    if type(k) ~= "number" or k <= 0 or k % 1 ~= 0 then return false end
    n = n + 1
  end
  for i = 1, n do if t[i] == nil then return false end end
  return true, n
end

local encode
local function encode_string(s)
  return '"' .. s:gsub('[%c"\\]', function(c)
    return escapes[c] or string.format("\\u%04x", c:byte())
  end) .. '"'
end

encode = function(v, stack)
  local t = type(v)
  if t == "nil" then return "null"
  elseif t == "boolean" then return tostring(v)
  elseif t == "number" then
    if v ~= v or v == math.huge or v == -math.huge then error("json: sayı kodlanamaz: " .. tostring(v)) end
    if v % 1 == 0 and math.abs(v) < 1e15 then return string.format("%d", v) end
    return string.format("%.14g", v)
  elseif t == "string" then return encode_string(v)
  elseif t == "table" then
    stack = stack or {}
    if stack[v] then error("json: döngüsel tablo") end
    stack[v] = true
    local out = {}
    local arr, n = is_array(v)
    if arr and n > 0 then
      for i = 1, n do out[i] = encode(v[i], stack) end
      stack[v] = nil
      return "[" .. table.concat(out, ",") .. "]"
    end
    local keys = {}
    for k in pairs(v) do
      if type(k) ~= "string" and type(k) ~= "number" then error("json: geçersiz anahtar türü " .. type(k)) end
      keys[#keys + 1] = k
    end
    table.sort(keys, function(a, b) return tostring(a) < tostring(b) end)
    for _, k in ipairs(keys) do
      out[#out + 1] = encode_string(tostring(k)) .. ":" .. encode(v[k], stack)
    end
    stack[v] = nil
    if #out == 0 and arr then return "[]" end
    return "{" .. table.concat(out, ",") .. "}"
  end
  error("json: kodlanamayan tür " .. t)
end

function json.encode(v) return encode(v) end

local function decode_error(s, i, msg)
  local line = select(2, s:sub(1, i):gsub("\n", "")) + 1
  error(string.format("json: %s (satır %d, konum %d)", msg, line, i), 0)
end

local function skip(s, i)
  return (s:find("[^ \t\r\n]", i)) or #s + 1
end

local function utf8_char(n)
  if n < 0x80 then return string.char(n)
  elseif n < 0x800 then return string.char(0xC0 + math.floor(n / 0x40), 0x80 + n % 0x40)
  elseif n < 0x10000 then return string.char(0xE0 + math.floor(n / 0x1000), 0x80 + math.floor(n / 0x40) % 0x40, 0x80 + n % 0x40)
  else return string.char(0xF0 + math.floor(n / 0x40000), 0x80 + math.floor(n / 0x1000) % 0x40, 0x80 + math.floor(n / 0x40) % 0x40, 0x80 + n % 0x40) end
end

local decode_value
local function decode_string(s, i)
  local out, j = {}, i + 1
  while true do
    local c = s:sub(j, j)
    if c == "" then decode_error(s, i, "kapanmamış dize") end
    if c == '"' then return table.concat(out), j + 1 end
    if c == "\\" then
      local e = s:sub(j + 1, j + 1)
      local map = { b = "\b", f = "\f", n = "\n", r = "\r", t = "\t", ['"'] = '"', ["\\"] = "\\", ["/"] = "/" }
      if map[e] then out[#out + 1] = map[e]; j = j + 2
      elseif e == "u" then
        local hex = s:sub(j + 2, j + 5)
        local n = tonumber(hex, 16)
        if not n then decode_error(s, j, "geçersiz \\u") end
        j = j + 6
        if n >= 0xD800 and n <= 0xDBFF and s:sub(j, j + 1) == "\\u" then
          local lo = tonumber(s:sub(j + 2, j + 5), 16)
          if lo and lo >= 0xDC00 and lo <= 0xDFFF then
            n = 0x10000 + (n - 0xD800) * 0x400 + (lo - 0xDC00)
            j = j + 6
          end
        end
        out[#out + 1] = utf8_char(n)
      else decode_error(s, j, "geçersiz kaçış") end
    else
      local k = s:find('["\\]', j) or #s + 1
      out[#out + 1] = s:sub(j, k - 1)
      j = k
    end
  end
end

decode_value = function(s, i)
  i = skip(s, i)
  local c = s:sub(i, i)
  if c == "{" then
    local t = {}
    i = skip(s, i + 1)
    if s:sub(i, i) == "}" then return t, i + 1 end
    while true do
      i = skip(s, i)
      if s:sub(i, i) ~= '"' then decode_error(s, i, "anahtar bekleniyordu") end
      local k
      k, i = decode_string(s, i)
      i = skip(s, i)
      if s:sub(i, i) ~= ":" then decode_error(s, i, "':' bekleniyordu") end
      t[k], i = decode_value(s, i + 1)
      i = skip(s, i)
      local d = s:sub(i, i)
      if d == "}" then return t, i + 1 end
      if d ~= "," then decode_error(s, i, "',' ya da '}' bekleniyordu") end
      i = i + 1
    end
  elseif c == "[" then
    local t, n = {}, 0
    i = skip(s, i + 1)
    if s:sub(i, i) == "]" then return t, i + 1 end
    while true do
      n = n + 1
      t[n], i = decode_value(s, i)
      i = skip(s, i)
      local d = s:sub(i, i)
      if d == "]" then return t, i + 1 end
      if d ~= "," then decode_error(s, i, "',' ya da ']' bekleniyordu") end
      i = i + 1
    end
  elseif c == '"' then return decode_string(s, i)
  elseif s:sub(i, i + 3) == "true" then return true, i + 4
  elseif s:sub(i, i + 4) == "false" then return false, i + 5
  elseif s:sub(i, i + 3) == "null" then return nil, i + 4
  else
    local num = s:match("^-?%d+%.?%d*[eE]?[-+]?%d*", i)
    if num and num ~= "" and tonumber(num) then return tonumber(num), i + #num end
    decode_error(s, i, "beklenmeyen karakter '" .. c .. "'")
  end
end

function json.decode(s)
  if type(s) ~= "string" then error("json.decode: dize bekleniyordu", 2) end
  local v, i = decode_value(s, 1)
  i = skip(s, i)
  if i <= #s then decode_error(s, i, "fazladan veri") end
  return v
end

return json
