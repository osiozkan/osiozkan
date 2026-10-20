-- Small JSON library (encode / decode), used when no other "json" module is installed.
-- API as in rxi/json.lua: json.encode(value), json.decode(text); json.null encodes null.
local json = { null = setmetatable({}, { __tostring = function() return "null" end }) }

local escapes = { ['"'] = '\\"', ['\\'] = '\\\\', ['\b'] = '\\b', ['\f'] = '\\f', ['\n'] = '\\n', ['\r'] = '\\r', ['\t'] = '\\t' }

local function escape_str(s)
  return '"' .. s:gsub('[%c"\\]', function(c) return escapes[c] or string.format("\\u%04x", c:byte()) end) .. '"'
end

local function array_length(t)
  local n = 0
  for k in pairs(t) do
    if type(k) ~= "number" or k < 1 or k % 1 ~= 0 then return nil end
    if k > n then n = k end
  end
  for i = 1, n do if t[i] == nil then return nil end end
  return n
end

local encode
encode = function(v, seen)
  local t = type(v)
  if v == nil or v == json.null then return "null" end
  if t == "boolean" then return tostring(v) end
  if t == "number" then
    if v ~= v or v == math.huge or v == -math.huge then error("json: cannot encode " .. tostring(v), 0) end
    if v % 1 == 0 and v > -1e15 and v < 1e15 then return string.format("%d", v) end
    return string.format("%.14g", v)
  end
  if t == "string" then return escape_str(v) end
  if t ~= "table" then error("json: cannot encode a " .. t, 0) end
  seen = seen or {}
  if seen[v] then error("json: circular reference", 0) end
  seen[v] = true
  local out = {}
  local n = array_length(v)
  if n then
    for i = 1, n do out[i] = encode(v[i], seen) end
    seen[v] = nil
    return "[" .. table.concat(out, ",") .. "]"
  end
  local keys = {}
  for k in pairs(v) do
    if type(k) ~= "string" and type(k) ~= "number" then error("json: object keys must be strings", 0) end
    keys[#keys + 1] = k
  end
  table.sort(keys, function(a, b) return tostring(a) < tostring(b) end)
  for i, k in ipairs(keys) do out[i] = escape_str(tostring(k)) .. ":" .. encode(v[k], seen) end
  seen[v] = nil
  return "{" .. table.concat(out, ",") .. "}"
end

function json.encode(v) return encode(v) end

-- decoding ---------------------------------------------------------------

local function utf8_char(cp)
  if cp < 0x80 then return string.char(cp) end
  if cp < 0x800 then return string.char(0xC0 + math.floor(cp / 0x40), 0x80 + cp % 0x40) end
  if cp < 0x10000 then return string.char(0xE0 + math.floor(cp / 0x1000), 0x80 + math.floor(cp / 0x40) % 0x40, 0x80 + cp % 0x40) end
  return string.char(0xF0 + math.floor(cp / 0x40000), 0x80 + math.floor(cp / 0x1000) % 0x40, 0x80 + math.floor(cp / 0x40) % 0x40, 0x80 + cp % 0x40)
end

local function fail(s, i, msg)
  local line = 1
  for _ in s:sub(1, i):gmatch("\n") do line = line + 1 end
  error(string.format("json: %s at line %d (character %d)", msg, line, i), 0)
end

local decode_value

local function skip(s, i)
  local _, e = s:find("^[ \n\r\t]*", i)
  return e + 1
end

local function decode_string(s, i)
  local out, j = {}, i + 1
  while true do
    local c = s:sub(j, j)
    if c == "" then fail(s, i, "unterminated string") end
    if c == '"' then return table.concat(out), j + 1 end
    if c == "\\" then
      local e = s:sub(j + 1, j + 1)
      local map = { b = "\b", f = "\f", n = "\n", r = "\r", t = "\t", ['"'] = '"', ["\\"] = "\\", ["/"] = "/" }
      if map[e] then out[#out + 1] = map[e]; j = j + 2
      elseif e == "u" then
        local hex = s:sub(j + 2, j + 5)
        local cp = tonumber(hex, 16)
        if not cp or #hex < 4 then fail(s, j, "bad \\u escape") end
        j = j + 6
        if cp >= 0xD800 and cp <= 0xDBFF and s:sub(j, j + 1) == "\\u" then
          local lo = tonumber(s:sub(j + 2, j + 5), 16)
          if lo and lo >= 0xDC00 and lo <= 0xDFFF then cp = 0x10000 + (cp - 0xD800) * 0x400 + (lo - 0xDC00); j = j + 6 end
        end
        out[#out + 1] = utf8_char(cp)
      else fail(s, j, "bad escape") end
    else
      local k = s:find('["\\]', j)
      if not k then fail(s, i, "unterminated string") end
      out[#out + 1] = s:sub(j, k - 1)
      j = k
    end
  end
end

decode_value = function(s, i)
  i = skip(s, i)
  local c = s:sub(i, i)
  if c == "{" then
    local obj = {}
    i = skip(s, i + 1)
    if s:sub(i, i) == "}" then return obj, i + 1 end
    while true do
      i = skip(s, i)
      if s:sub(i, i) ~= '"' then fail(s, i, "expected a key") end
      local k; k, i = decode_string(s, i)
      i = skip(s, i)
      if s:sub(i, i) ~= ":" then fail(s, i, "expected ':'") end
      local v; v, i = decode_value(s, i + 1)
      obj[k] = v
      i = skip(s, i)
      local d = s:sub(i, i)
      if d == "}" then return obj, i + 1 end
      if d ~= "," then fail(s, i, "expected ',' or '}'") end
      i = i + 1
    end
  elseif c == "[" then
    local arr, n = {}, 0
    i = skip(s, i + 1)
    if s:sub(i, i) == "]" then return arr, i + 1 end
    while true do
      local v; v, i = decode_value(s, i)
      n = n + 1
      arr[n] = v
      i = skip(s, i)
      local d = s:sub(i, i)
      if d == "]" then return arr, i + 1 end
      if d ~= "," then fail(s, i, "expected ',' or ']'") end
      i = i + 1
    end
  elseif c == '"' then
    return decode_string(s, i)
  elseif s:sub(i, i + 3) == "true" then return true, i + 4
  elseif s:sub(i, i + 4) == "false" then return false, i + 5
  elseif s:sub(i, i + 3) == "null" then return nil, i + 4
  else
    local num = s:match("^-?%d+%.?%d*[eE]?[-+]?%d*", i)
    if not num or num == "" or num == "-" then fail(s, i, "unexpected character '" .. c .. "'") end
    return tonumber(num), i + #num
  end
end

function json.decode(s)
  if type(s) ~= "string" then error("json: expected a string, got " .. type(s), 2) end
  local v, i = decode_value(s, 1)
  i = skip(s, i)
  if i <= #s then fail(s, i, "trailing characters") end
  return v
end

return json
