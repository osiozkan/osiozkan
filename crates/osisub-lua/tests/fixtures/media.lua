script_name = "Medya"
aegisub.register_macro("Medya bilgisi", "", function(subs, sel)
  local w, h = aegisub.video_size()
  local a, b = aegisub.get_audio_selection()
  aegisub.log("fps:%d,%d size:%dx%d aud:%d-%d pos:%d", aegisub.frame_from_ms(1000), aegisub.ms_from_frame(24), w, h, a, b, aegisub.project_properties().video_position)
end)
