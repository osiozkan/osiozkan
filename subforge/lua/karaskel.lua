-- karaskel.lua compatible implementation for SubForge.
karaskel = {}

local function strip_tags(t) return (t:gsub("{[^}]*}", "")) end

function karaskel.collect_head(subs, generate_furigana)
	local meta = { res_x = 0, res_y = 0 }
	local styles = { n = 0 }
	for i = 1, #subs do
		local l = subs[i]
		if l.class == "info" then
			local k = l.key:lower()
			if k == "playresx" then meta.res_x = tonumber(l.value) or 0
			elseif k == "playresy" then meta.res_y = tonumber(l.value) or 0
			elseif k == "wrapstyle" then meta.wrapstyle = tonumber(l.value)
			elseif k == "scaledborderandshadow" then meta.scaled_border_and_shadow = l.value:lower() == "yes" end
		elseif l.class == "style" then
			styles.n = styles.n + 1
			styles[styles.n] = l
			styles[l.name] = l
			l.margin_v = l.margin_t
		elseif l.class == "dialogue" then
			break
		end
	end
	if meta.res_x == 0 and meta.res_y == 0 then meta.res_x = 384; meta.res_y = 288
	elseif meta.res_x == 0 then meta.res_x = meta.res_y / 3 * 4
	elseif meta.res_y == 0 then meta.res_y = meta.res_x * 3 / 4 end
	meta.video_x_correct_factor = 1
	if aegisub.video_size then
		local vx, vy = aegisub.video_size()
		if vx and vy and meta.res_y > 0 then meta.video_x_correct_factor = (vx / vy) / (meta.res_x / meta.res_y) end
	end
	return meta, styles
end

function karaskel.use_furigana(styles) end

local function check_style(styles, name)
	local s = styles[name]
	if s then return s end
	aegisub.log(1, "Style not found: " .. tostring(name) .. "\n")
	return styles["Default"] or styles[1]
end

function karaskel.preproc_line_text(meta, styles, line)
	line.text_stripped = strip_tags(line.text)
	line.duration = line.end_time - line.start_time
	line.styleref = check_style(styles, line.style)
	local sty = line.styleref
	local w, h, d, el = aegisub.text_extents(sty, line.text_stripped:gsub("\\[Nnh]", " "))
	line.width, line.height, line.descent, line.extlead = w, h, d, el
end

function karaskel.preproc_line_size(meta, styles, line)
	local sty = line.styleref
	line.margin_v = line.margin_t
	line.eff_margin_l = (line.margin_l > 0) and line.margin_l or sty.margin_l
	line.eff_margin_r = (line.margin_r > 0) and line.margin_r or sty.margin_r
	line.eff_margin_t = (line.margin_t > 0) and line.margin_t or sty.margin_t
	line.eff_margin_b = (line.margin_b > 0) and line.margin_b or sty.margin_b
	line.eff_margin_v = line.eff_margin_t
end

function karaskel.preproc_line_pos(meta, styles, line)
	local sty = line.styleref
	local align = sty.align
	local ov = line.text:match("{[^}]*\\an(%d)")
	if ov then align = tonumber(ov) end
	line.halign = ({ "left", "center", "right" })[(align - 1) % 3 + 1]
	line.valign = (align <= 3) and "bottom" or ((align <= 6) and "middle" or "top")
	line.hspace = meta.res_x - line.eff_margin_l - line.eff_margin_r
	if line.halign == "left" then
		line.left = line.eff_margin_l; line.center = line.left + line.width / 2; line.right = line.left + line.width
	elseif line.halign == "center" then
		line.left = line.eff_margin_l + (line.hspace - line.width) / 2; line.center = line.left + line.width / 2; line.right = line.left + line.width
	else
		line.left = meta.res_x - line.eff_margin_r - line.width; line.center = line.left + line.width / 2; line.right = line.left + line.width
	end
	if line.valign == "bottom" then
		line.bottom = meta.res_y - line.eff_margin_b; line.middle = line.bottom - line.height / 2; line.top = line.bottom - line.height
	elseif line.valign == "middle" then
		line.middle = meta.res_y / 2; line.top = line.middle - line.height / 2; line.bottom = line.top + line.height
	else
		line.top = line.eff_margin_t; line.middle = line.top + line.height / 2; line.bottom = line.top + line.height
	end
	line.x = ({ left = line.left, center = line.center, right = line.right })[line.halign]
	line.y = ({ top = line.top, middle = line.middle, bottom = line.bottom })[line.valign]
end

function karaskel.parse_k(text)
	-- returns syllable list in karaskel format
	local kara = { n = 0 }
	local pos, elapsed = 1, 0
	local cur = { text = "", tag = "\\k0", duration = 0, start_time = 0, inline_fx = "" }
	local started = false
	local function push()
		kara.n = kara.n + 1
		cur.i = kara.n
		cur.end_time = cur.start_time + cur.duration
		kara[kara.n] = cur
	end
	local s = text
	local i = 1
	while i <= #s do
		local b, e = s:find("^{[^}]*}", i)
		if b then
			local blk = s:sub(b + 1, e - 1)
			local kt, kv = blk:match("\\([kK][fo]?)(%-?%d+%.?%d*)")
			if kt then
				if started or cur.text ~= "" then push(); elapsed = elapsed + cur.duration end
				cur = { text = "", tag = "\\" .. kt .. kv, duration = math.floor(tonumber(kv) * 10 + 0.5), start_time = elapsed, inline_fx = blk:match("\\%-([%w_]+)") or "", tagtype = kt }
				if kt == "K" then cur.tagtype = "kf" end
				started = true
				cur.text = "{" .. blk:gsub("\\[kK][fo]?%-?%d+%.?%d*", "") .. "}"
				if cur.text == "{}" then cur.text = "" end
			else
				cur.text = cur.text .. s:sub(b, e)
			end
			i = e + 1
		else
			local nxt = s:find("{", i, true)
			local chunk = nxt and s:sub(i, nxt - 1) or s:sub(i)
			if not nxt or nxt > i then
				cur.text = cur.text .. chunk
				i = i + #chunk
			else
				cur.text = cur.text .. s:sub(i, i); i = i + 1
			end
		end
	end
	if started or cur.text ~= "" then push() end
	return kara
end

function karaskel.preproc_line_kara(meta, styles, line)
	line.kara = karaskel.parse_k(line.text)
	local sty = line.styleref
	local left = 0
	for _, syl in ipairs(line.kara) do
		syl.text_stripped = strip_tags(syl.text)
		syl.text_spacestripped = syl.text_stripped:gsub("^%s+", ""):gsub("%s+$", "")
		syl.prespace = #(syl.text_stripped:match("^%s*"))
		syl.postspace = #(syl.text_stripped:match("%s*$"))
		syl.end_time = syl.start_time + syl.duration
		syl.width, syl.height = aegisub.text_extents(sty, syl.text_stripped)
		syl.prespace_width = aegisub.text_extents(sty, (" "):rep(syl.prespace))
		syl.postspace_width = aegisub.text_extents(sty, (" "):rep(syl.postspace))
		syl.left = left
		syl.center = left + syl.width / 2
		syl.right = left + syl.width
		syl.x = line.left and (line.left + syl.center) or syl.center
		syl.y = line.y
		left = left + syl.width
	end
end

function karaskel.preproc_line(subs, meta, styles, line)
	karaskel.preproc_line_text(meta, styles, line)
	karaskel.preproc_line_size(meta, styles, line)
	karaskel.preproc_line_pos(meta, styles, line)
	karaskel.preproc_line_kara(meta, styles, line)
	for _, syl in ipairs(line.kara) do
		syl.left = syl.left + (line.left or 0)
		syl.center = syl.center + (line.left or 0)
		syl.right = syl.right + (line.left or 0)
		syl.x = syl.center
	end
end

function karaskel.fix_line_syls(line) end
