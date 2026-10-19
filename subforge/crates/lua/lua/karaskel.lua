-- SubForge: karaskel.lua compatible with Aegisub's Automation 4 karaoke skeleton.
include("utils.lua")

karaskel = {}

-- Collect styles and script metadata. Returns meta, styles.
function karaskel.collect_head(subs, generate_furigana)
	local meta = { res_x = 0, res_y = 0 }
	local styles = { n = 0 }
	local i = 1
	while i <= #subs do
		local l = subs[i]
		if l.class == "style" then
			styles.n = styles.n + 1
			styles[styles.n] = l
			styles[l.name] = l
			l.margin_v = l.margin_t
		elseif l.class == "info" then
			local k = l.key:lower()
			if k == "playresx" then meta.res_x = math.floor(tonumber(l.value) or 0)
			elseif k == "playresy" then meta.res_y = math.floor(tonumber(l.value) or 0)
			elseif k == "wrapstyle" then meta.wrapstyle = tonumber(l.value)
			elseif k == "scaledborderandshadow" then meta.scaledborder = (l.value:lower() == "yes")
			end
			meta[l.key] = l.value
		end
		i = i + 1
	end
	if meta.res_x == 0 and meta.res_y == 0 then
		meta.res_x = 384
		meta.res_y = 288
	elseif meta.res_x == 0 then
		meta.res_x = meta.res_y / 3 * 4
	elseif meta.res_y == 0 then
		meta.res_y = meta.res_x * 3 / 4
	end
	meta.video_x_correct_factor = 1
	return meta, styles
end

-- Remove all {} blocks.
local function strip_tags(text) return (text:gsub("{.-}", "")) end

local function extents(style, text)
	local w, h, d, e = aegisub.text_extents(style, text)
	return w, h, d, e
end

-- Split a line's text into syllables.
function karaskel.preproc_line_text(meta, styles, line)
	line.text_stripped = strip_tags(line.text)
	line.duration = line.end_time - line.start_time
	line.kara = { n = 0 }
	local kara = line.kara

	local function newsyl(dur, tag, text_blocks, inline_fx, tags_prefix)
		kara.n = kara.n + 1
		local s = {
			i = kara.n,
			duration = dur * 10,
			kdur = dur,
			tag = tag,
			inline_fx = inline_fx or "",
			text = "",
			tags = tags_prefix or "",
		}
		kara[kara.n] = s
		return s
	end

	-- state
	local text = line.text
	local pos = 1
	local cur = nil
	local pending_tags = ""
	-- implicit leading syllable (text before the first \k)
	local function ensure_syl()
		if not cur then cur = newsyl(0, "k", nil, "", "") end
	end

	while pos <= #text do
		local bs, be = text:find("{.-}", pos)
		if bs == pos then
			local block = text:sub(bs + 1, be - 1)
			local started = false
			-- walk tags in the block, a \k begins a new syllable
			local i = 1
			local out_other = {}
			while i <= #block do
				local ks, ke, ktag, kval = block:find("^\\([kK][fo]?)(%-?%d+%.?%d*)", i)
				if ks then
					if ktag == "K" then ktag = "kf" end
					cur = newsyl(math.floor(tonumber(kval) + 0.5), ktag, nil, "", "")
					cur.tags = table.concat(out_other)
					out_other = {}
					started = true
					i = ke + 1
				else
					local fs, fe, fx = block:find("^\\%-([^\\}]*)", i)
					if fs then
						ensure_syl()
						cur.inline_fx = fx
						i = fe + 1
					else
						-- other tag: copy until next backslash outside parentheses
						local j = i + 1
						local depth = 0
						while j <= #block do
							local c = block:sub(j, j)
							if c == "(" then depth = depth + 1
							elseif c == ")" then depth = depth - 1
							elseif c == "\\" and depth <= 0 then break end
							j = j + 1
						end
						local piece = block:sub(i, j - 1)
						if block:sub(i, i) == "\\" then
							if started or cur then
								ensure_syl()
								cur.tags = (cur.tags or "") .. piece
							else
								out_other[#out_other + 1] = piece
							end
						end
						i = j
					end
				end
			end
			pos = be + 1
		else
			local stop = bs and (bs - 1) or #text
			ensure_syl()
			cur.text = cur.text .. text:sub(pos, stop)
			pos = stop + 1
		end
	end

	-- timing + stripped variants
	local t = 0
	for i = 1, kara.n do
		local s = kara[i]
		s.start_time = t
		s.end_time = t + s.duration
		t = s.end_time
		s.text_stripped = strip_tags(s.text)
		s.text_spacestripped = s.text_stripped:gsub("^%s+", ""):gsub("%s+$", "")
	end
	-- drop an empty leading placeholder syllable created by implicit start
	if kara.n > 1 and kara[1].duration == 0 and kara[1].text == "" then
		table.remove(kara, 1)
		kara.n = kara.n - 1
		for i = 1, kara.n do kara[i].i = i end
	end
end

function karaskel.preproc_line_size(meta, styles, line)
	local style = line.styleref
	local w, h, d, e = extents(style, line.text_stripped)
	line.width, line.height, line.descent, line.extlead = w, h, d, e
	local left = 0
	for i = 1, line.kara.n do
		local s = line.kara[i]
		local pre = s.text_stripped:match("^%s*")
		local post = s.text_stripped:match("%s*$")
		s.prespace = extents(style, pre)
		s.postspace = extents(style, post)
		if #pre == #s.text_stripped then s.postspace = 0 end
		local sw, sh, sd, se = extents(style, s.text_spacestripped)
		local fullw = extents(style, s.text_stripped)
		s.width, s.height = fullw - s.prespace - s.postspace, sh
		s.left = left + s.prespace
		s.center = s.left + s.width / 2
		s.right = s.left + s.width
		left = left + fullw
	end
	line.furi = line.furi or {}
end

function karaskel.preproc_line_pos(meta, styles, line)
	local st = line.styleref
	line.margin_l = line.margin_l or 0
	line.margin_r = line.margin_r or 0
	line.margin_t = line.margin_t or line.margin_v or 0
	line.margin_b = line.margin_b or line.margin_v or 0
	line.eff_margin_l = (line.margin_l > 0) and line.margin_l or st.margin_l
	line.eff_margin_r = (line.margin_r > 0) and line.margin_r or st.margin_r
	line.eff_margin_t = (line.margin_t > 0) and line.margin_t or st.margin_t
	line.eff_margin_b = (line.margin_b > 0) and line.margin_b or st.margin_b
	line.eff_margin_v = line.eff_margin_t
	line.margin_v = line.eff_margin_t

	local al = st.align
	if al % 3 == 1 then line.halign = "left" elseif al % 3 == 2 then line.halign = "center" else line.halign = "right" end
	if al > 6 then line.valign = "top" elseif al > 3 then line.valign = "middle" else line.valign = "bottom" end

	if line.halign == "left" then
		line.left = line.eff_margin_l
		line.center = line.left + line.width / 2
		line.right = line.left + line.width
		line.x = line.left
	elseif line.halign == "center" then
		line.left = (meta.res_x - line.eff_margin_l - line.eff_margin_r - line.width) / 2 + line.eff_margin_l
		line.center = line.left + line.width / 2
		line.right = line.left + line.width
		line.x = line.center
	else
		line.right = meta.res_x - line.eff_margin_r
		line.center = line.right - line.width / 2
		line.left = line.right - line.width
		line.x = line.right
	end

	if line.valign == "top" then
		line.top = line.eff_margin_t
		line.middle = line.top + line.height / 2
		line.bottom = line.top + line.height
		line.y = line.top
	elseif line.valign == "middle" then
		line.top = (meta.res_y - line.height) / 2
		line.middle = line.top + line.height / 2
		line.bottom = line.top + line.height
		line.y = line.middle
	else
		line.bottom = meta.res_y - line.eff_margin_b
		line.middle = line.bottom - line.height / 2
		line.top = line.bottom - line.height
		line.y = line.bottom
	end
end

function karaskel.preproc_line(subs, meta, styles, line)
	line.styleref = styles[line.style]
	if not line.styleref then
		aegisub.debug.out(2, "WARNING: Style not found: " .. tostring(line.style) .. "\n")
		line.styleref = styles["Default"] or styles[1]
	end
	karaskel.preproc_line_text(meta, styles, line)
	karaskel.preproc_line_size(meta, styles, line)
	karaskel.preproc_line_pos(meta, styles, line)
end

-- Convenience used by many templates.
function karaskel.syl_text(syl) return syl.text_stripped end

return karaskel
