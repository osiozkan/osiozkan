script_name = "Test macro"
script_description = "Exercises the Automation 4 API"
script_author = "subforge"
script_version = "1"
include("karaskel.lua")
local re = require "aegisub.re"
local util = require "aegisub.util"

function dbl_space(subs, sel)
	for _, i in ipairs(sel) do
		local l = subs[i]
		l.text = re.sub(l.text, "\\s{2,}", " ")
		subs[i] = l
	end
	aegisub.log("fixed %d lines\n", #sel)
end

function kara(subs, sel)
	local meta, styles = karaskel.collect_head(subs)
	aegisub.log("res %dx%d, styles %d\n", meta.res_x, meta.res_y, styles.n)
	local added = 0
	for i = 1, #subs do
		local l = subs[i]
		if l.class == "dialogue" and not l.comment and l.text:find("\\k") then
			karaskel.preproc_line(subs, meta, styles, l)
			aegisub.log("line %d: %d syls, dur %d, first syl start=%d dur=%d\n", i, l.kara.n, l.duration, l.kara[1].start_time, l.kara[1].duration)
			local n = util.deep_copy(l)
			n.layer = 1; n.text = "{\\an5}" .. l.text_stripped
			subs.append(n)
			added = added + 1
		end
	end
	return {}
end

function dlg(subs, sel)
	local btn, res = aegisub.dialog.display({
		{ class = "label", label = "Shift", x = 0, y = 0 },
		{ class = "intedit", name = "ms", value = 100, x = 1, y = 0 },
		{ class = "edit", name = "tag", text = "x", x = 1, y = 1 },
	}, { "Apply", "Cancel" }, { ok = "Apply", cancel = "Cancel" })
	if btn ~= "Apply" then aegisub.cancel() end
	for _, i in ipairs(sel) do
		local l = subs[i]
		l.start_time = l.start_time + res.ms
		l.effect = res.tag
		subs[i] = l
	end
end

function boom(subs, sel) error("kaboom") end
function cancel(subs, sel) subs.delete(1); aegisub.cancel() end

aegisub.register_macro("Collapse spaces", "d", dbl_space)
aegisub.register_macro("Kara dup", "k", kara)
aegisub.register_macro("Dialog", "dlg", dlg)
aegisub.register_macro("Boom", "b", boom)
aegisub.register_macro("Cancel", "c", cancel)
