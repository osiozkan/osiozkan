//! Headless UI tests: the real `App::frame` runs on a bare egui context with synthetic input.

use super::*;
use eframe::egui::{Event as UiEvent, Modifiers, PointerButton, Pos2, RawInput, Rect, Vec2};
use asscore::lines::{MatchMode, SelectAction, SelectOpts};
use asscore::replace;

fn config_once() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let dir = std::env::temp_dir().join(format!("yuzu-gui-tests-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::env::set_var("XDG_CONFIG_HOME", &dir);
        std::env::set_var("YUZU_NO_MPV", "1");
    });
}

fn app_with(lines: &[(i64, i64, &str)]) -> (egui::Context, App) {
    config_once();
    let ctx = egui::Context::default();
    let mut app = App::with_context(&ctx, None, None);
    // tests share one settings file; always start from the default keys
    app.keymap = Keymap::default();
    app.settings.shortcuts.clear();
    app.doc.events = lines.iter().map(|(s, e, t)| Event::new(*s, *e, "Default", t)).collect();
    app.select_only(0);
    (ctx, app)
}

fn frame(ctx: &egui::Context, app: &mut App, events: Vec<UiEvent>) -> egui::FullOutput {
    let mut input = RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1400.0, 900.0))), ..Default::default() };
    input.events = events;
    ctx.run(input, |ctx| app.frame(ctx))
}

fn key(k: Key) -> UiEvent {
    UiEvent::Key { key: k, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE }
}

fn click(ctx: &egui::Context, app: &mut App, pos: Pos2, button: PointerButton) {
    frame(ctx, app, vec![UiEvent::PointerMoved(pos)]);
    frame(ctx, app, vec![UiEvent::PointerButton { pos, button, pressed: true, modifiers: Modifiers::NONE }]);
    frame(ctx, app, vec![UiEvent::PointerButton { pos, button, pressed: false, modifiers: Modifiers::NONE }]);
    frame(ctx, app, vec![]);
}

#[test]
fn timing_keys_move_between_lines_and_commit_adds_a_line() {
    let (ctx, mut app) = app_with(&[(0, 1000, "a"), (1000, 2000, "b"), (2000, 3000, "c")]);
    frame(&ctx, &mut app, vec![]);
    frame(&ctx, &mut app, vec![key(Key::X)]);
    assert_eq!(app.active, 1);
    frame(&ctx, &mut app, vec![key(Key::Z)]);
    assert_eq!(app.active, 0);
    frame(&ctx, &mut app, vec![key(Key::G)]);
    frame(&ctx, &mut app, vec![key(Key::G)]);
    assert_eq!(app.active, 2);
    // commit on the last line starts a new line where this one ends
    frame(&ctx, &mut app, vec![key(Key::G)]);
    assert_eq!(app.doc.events.len(), 4);
    assert_eq!(app.active, 3);
    assert_eq!(app.doc.events[3].start, 3000);
    // undo removes it again
    app.do_undo();
    assert_eq!(app.doc.events.len(), 3);
}

#[test]
fn typing_in_the_editor_does_not_trigger_timing_keys() {
    let (ctx, mut app) = app_with(&[(0, 1000, "a"), (1000, 2000, "b")]);
    frame(&ctx, &mut app, vec![]);
    ctx.memory_mut(|m| m.request_focus(egui::Id::new("text_edit")));
    frame(&ctx, &mut app, vec![]);
    frame(&ctx, &mut app, vec![key(Key::X), UiEvent::Text("x".into())]);
    assert_eq!(app.active, 0, "X typed into the text box must not move to the next line");
    assert_eq!(app.doc.events[0].text, "ax");
}

#[test]
fn set_times_from_video_frame_are_frame_exact() {
    let (ctx, mut app) = app_with(&[(0, 5000, "a")]);
    app.settings.fps = (24000, 1001);
    let fps = app.fps();
    app.media.info = Some(yuzu_media::MediaInfo { width: 1280, height: 720, fps_num: 24000, fps_den: 1001, duration_ms: 60_000, has_video: true, has_audio: true, frames: 0 });
    frame(&ctx, &mut app, vec![]);
    for n in [1i64, 24, 100, 1001, 2345] {
        app.media.pos_ms = fps.ms_from_frame(n);
        app.set_line_time_from_pos(true);
        app.set_line_time_from_pos(false);
        let e = &app.doc.events[0];
        // after the centisecond rounding of the file format, the line covers exactly frame n
        let (s, en) = (time::parse_ass(&time::format_ass(e.start)).unwrap(), time::parse_ass(&time::format_ass(e.end)).unwrap());
        assert_eq!((fps.frame_for_start(s), fps.frame_for_end(en)), (n, n), "frame {}: {}..{}", n, s, en);
    }
    // audio only: the playhead is used as is
    app.media.info = None;
    app.media.pos_ms = 1234;
    app.set_line_time_from_pos(true);
    assert_eq!(app.doc.events[0].start, 1234);
}

#[test]
fn clicks_in_the_timing_view_set_start_and_end() {
    let (ctx, mut app) = app_with(&[(0, 500, "a"), (600, 900, "b")]);
    app.media.audio = Some(PathBuf::from("/nonexistent.wav"));
    app.media.view_start = 0.0;
    app.media.ms_per_px = 10.0;
    app.media.snap_keyframes = false;
    frame(&ctx, &mut app, vec![]);
    frame(&ctx, &mut app, vec![]);
    let r = app.wave_rect.expect("timing view drawn");
    let x_of = |t: f64| r.left() + (t / 10.0) as f32;
    // left click: start; right click: end (Aegisub)
    click(&ctx, &mut app, Pos2::new(x_of(2000.0), r.center().y), PointerButton::Primary);
    click(&ctx, &mut app, Pos2::new(x_of(4500.0), r.center().y), PointerButton::Secondary);
    let e = &app.doc.events[0];
    assert!((e.start - 2000).abs() <= 10, "{}", e.start);
    assert!((e.end - 4500).abs() <= 10, "{}", e.end);
    // the other line is untouched, and both edits can be undone
    assert_eq!((app.doc.events[1].start, app.doc.events[1].end), (600, 900));
    app.do_undo();
    app.do_undo();
    assert_eq!((app.doc.events[0].start, app.doc.events[0].end), (0, 500));
}

#[test]
fn keyframe_snapping_uses_frame_exact_times() {
    let (_ctx, mut app) = app_with(&[(990, 2050, "a")]);
    app.settings.fps = (24000, 1001);
    let fps = app.fps();
    // keyframes reported by ffprobe at frames 24 and 48 (millisecond timestamps)
    app.media.keyframes = vec![fps.ms_from_frame(24), fps.ms_from_frame(48)];
    let snap = app.media.keyframe_line_times(fps);
    assert_eq!(snap, vec![fps.line_start(24), fps.line_start(48)]);
    let pp = ops::PostProc { lead_in: 0, lead_out: 0, keyframe_snap_before: 300, keyframe_snap_after: 300, ..Default::default() };
    ops::post_process(&mut app.doc, &[0], &pp, &snap);
    let e = &app.doc.events[0];
    assert_eq!(fps.frame_for_start(e.start), 24);
    assert_eq!(fps.frame_for_end(e.end), 47, "ends on the frame before the keyframe");
}

#[test]
fn grid_click_selects_the_row_even_on_its_text() {
    let (ctx, mut app) = app_with(&[(0, 1000, "first"), (1000, 2000, "second"), (2000, 3000, "third")]);
    frame(&ctx, &mut app, vec![]);
    let out = frame(&ctx, &mut app, vec![]);
    // click right on the text of the third row
    let pos = find_text_pos(&out, "third").expect("row text laid out");
    click(&ctx, &mut app, pos, PointerButton::Primary);
    assert_eq!(app.active, 2);
}

/// Screen position of the first text shape containing `needle` in a frame's output.
fn find_text_pos(out: &egui::FullOutput, needle: &str) -> Option<Pos2> {
    out.shapes.iter().find_map(|c| match &c.shape {
        egui::epaint::Shape::Text(t) if t.galley.text().contains(needle) => Some(t.pos + t.galley.rect.center().to_vec2()),
        _ => None,
    })
}

fn key_mod(k: Key, m: Modifiers) -> UiEvent {
    UiEvent::Key { key: k, physical_key: None, pressed: true, repeat: false, modifiers: m }
}

#[test]
fn palette_goes_to_a_line_and_runs_commands() {
    let (ctx, mut app) = app_with(&[(0, 1000, "a"), (1000, 2000, "b"), (2000, 3000, "c")]);
    frame(&ctx, &mut app, vec![]);
    frame(&ctx, &mut app, vec![key_mod(Key::P, Modifiers::COMMAND | Modifiers::SHIFT)]);
    assert!(app.palette.is_some(), "Ctrl+Shift+P opens the palette");
    frame(&ctx, &mut app, vec![UiEvent::Text(":3".into())]);
    frame(&ctx, &mut app, vec![key(Key::Enter)]);
    assert!(app.palette.is_none());
    assert_eq!(app.active, 2);
    // a command through the palette: Turkish-insensitive search for "Tüm satırları seç"
    app.run_command(&ctx, "palette");
    frame(&ctx, &mut app, vec![]);
    frame(&ctx, &mut app, vec![UiEvent::Text("tum satirlari sec".into())]);
    frame(&ctx, &mut app, vec![key(Key::Enter)]);
    assert_eq!(app.selection.len(), 3);
    assert_eq!(app.settings.palette_recent.first().map(|s| s.as_str()), Some("select_all"));
    // while the palette is open its keys do not reach the timing keys
    app.run_command(&ctx, "palette");
    frame(&ctx, &mut app, vec![]);
    frame(&ctx, &mut app, vec![key(Key::Z), UiEvent::Text("z".into())]);
    assert_eq!(app.active, 2);
    frame(&ctx, &mut app, vec![key(Key::Escape)]);
    assert!(app.palette.is_none());
}

#[test]
fn rebinding_a_shortcut_takes_effect_and_is_saved() {
    let (ctx, mut app) = app_with(&[(0, 1000, "a")]);
    frame(&ctx, &mut app, vec![]);
    let mut p = Prefs::new(&app.settings, &app.keymap);
    p.keymap.set("qc", vec![commands::parse("Ctrl+Shift+Q").unwrap()]);
    p.keymap.set("delete", vec![]);
    app.apply_prefs(&ctx, p);
    assert_eq!(app.settings.shortcuts.len(), 2);
    frame(&ctx, &mut app, vec![key_mod(Key::Q, Modifiers::COMMAND | Modifiers::SHIFT)]);
    assert!(app.is_open(Panel::Qc), "the new binding opens the quality check");
    // Delete is unbound now: the line stays
    frame(&ctx, &mut app, vec![key(Key::Delete)]);
    assert_eq!(app.doc.events.len(), 1);
    // and what gets written keeps it (other tests share the settings file, so check the text)
    let reloaded = Settings::parse(&app.settings.to_text());
    assert!(reloaded.shortcuts.iter().any(|(id, k)| id == "qc" && k == "Ctrl+Shift+Q"));
}

#[test]
fn ctrl_shift_z_is_redo_not_undo() {
    let (ctx, mut app) = app_with(&[(0, 1000, "a")]);
    frame(&ctx, &mut app, vec![]);
    app.push_undo();
    app.doc.events[0].text = "b".into();
    frame(&ctx, &mut app, vec![key_mod(Key::Z, Modifiers::COMMAND)]);
    assert_eq!(app.doc.events[0].text, "a");
    frame(&ctx, &mut app, vec![key_mod(Key::Z, Modifiers::COMMAND | Modifiers::SHIFT)]);
    assert_eq!(app.doc.events[0].text, "b");
}

fn temp_file(name: &str, content: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("yuzu-gui-files-{}-{}", std::process::id(), name.replace('.', "_")));
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join(name);
    std::fs::write(&p, content).unwrap();
    p
}

/// 40 lines in 4 scenes (gaps ≥ 6 s between scenes), varied durations.
fn scene_lines() -> Vec<(i64, i64)> {
    let mut v = Vec::new();
    let mut t = 3000;
    for scene in 0..4 {
        for k in 0..10 {
            let d = 900 + ((k * 37 + scene * 11) % 17) * 130;
            v.push((t, t + d));
            t += d + 200 + ((k * 13) % 7) * 120;
        }
        t += 6000;
    }
    v
}

fn srt_of(lines: &[(i64, i64, String)]) -> String {
    lines.iter().enumerate().map(|(i, (s, e, t))| format!("{}\n{} --> {}\n{}\n\n", i + 1, time::format_srt(*s), time::format_srt(*e), t)).collect()
}

#[test]
fn reference_offset_column_and_missing_lines() {
    let base = scene_lines();
    // reference (English) at the true times; the document is 1.75 s later and lacks line 7
    let reference: Vec<(i64, i64, String)> = base.iter().enumerate().map(|(i, &(s, e))| (s, e, format!("English line number {}", i))).collect();
    let doc: Vec<(i64, i64, String)> = base.iter().enumerate().filter(|(i, _)| *i != 7).map(|(i, &(s, e))| (s + 1750, e + 1750, format!("Türkçe satır {}", i))).collect();
    let doc_ref: Vec<(i64, i64, &str)> = doc.iter().map(|(s, e, t)| (*s, *e, t.as_str())).collect();
    let (ctx, mut app) = app_with(&doc_ref);
    frame(&ctx, &mut app, vec![]);
    app.set_reference(temp_file("ref_en.srt", &srt_of(&reference)));
    assert!(app.osi.reference.is_some());
    app.find_reference_offset();
    assert!((app.osi.ref_offset_ms - 1750).abs() <= 20, "{}", app.osi.ref_offset_ms);
    // the grid column / editor show the matching English line
    assert_eq!(app.reference_text(10).as_deref(), Some("English line number 11"));
    let out = frame(&ctx, &mut app, vec![]);
    assert!(out.shapes.iter().any(|c| matches!(&c.shape, egui::epaint::Shape::Text(t) if t.galley.text().contains("English line number 0"))), "reference column drawn");
    // aligncompare against the reference: one EKSİK, added once as a comment at the document's time
    app.run_compare();
    let r = app.osi.cmp_result.as_ref().unwrap();
    assert_eq!(r.counts()["EKSİK"], 1);
    assert!((r.shift_ms - 1750).abs() <= 20);
    let n = app.doc.events.len();
    app.compare_add_missing();
    assert_eq!(app.doc.events.len(), n + 1);
    let added = app.doc.events.iter().find(|e| e.comment).unwrap();
    assert!(added.text.contains("English line number 7"));
    assert!((added.start - (base[7].0 + 1750)).abs() <= 10, "{}", added.start);
    app.compare_add_missing();
    assert_eq!(app.doc.events.len(), n + 1, "not added twice");
}

#[test]
fn sync_to_reference_moves_each_scene() {
    let base = scene_lines();
    let reference: Vec<(i64, i64, String)> = base.iter().map(|&(s, e)| (s, e, "ref".to_string())).collect();
    // scenes off by different amounts (the offset changes only at scene breaks)
    let shifts = [1200i64, 1200, -700, 2100];
    let doc: Vec<(i64, i64, String)> = base.iter().enumerate().map(|(i, &(s, e))| (s + shifts[i / 10], e + shifts[i / 10], format!("satır {}", i))).collect();
    let doc_ref: Vec<(i64, i64, &str)> = doc.iter().map(|(s, e, t)| (*s, *e, t.as_str())).collect();
    let (ctx, mut app) = app_with(&doc_ref);
    frame(&ctx, &mut app, vec![]);
    app.set_reference(temp_file("ref_sync.srt", &srt_of(&reference)));
    app.osi.sync_use_ref = true;
    app.start_sync_plan();
    let plan = app.osi.sync_plan.as_ref().expect("planned");
    // every scene break (≥ 4 s) starts a new segment, each with its own offset
    let found: Vec<i64> = plan.segs.iter().map(|s| s.shift).collect();
    assert_eq!(found.len(), 4);
    for (f, sh) in found.iter().zip(shifts) { assert!((f + sh).abs() <= 20, "{:?}", found); }
    app.apply_sync();
    for (i, &(s, e)) in base.iter().enumerate() {
        let ev = &app.doc.events[i];
        assert!((ev.start - s).abs() <= 60 && (ev.end - e).abs() <= 60, "line {}: {}..{} vs {}..{}", i, ev.start, ev.end, s, e);
    }
    // one undo restores everything
    app.do_undo();
    assert_eq!(app.doc.events[0].start, base[0].0 + 1200);
}

#[test]
fn glossary_is_found_next_to_the_script_and_fixes_terms() {
    let script = temp_file("bolum.ass", "[Script Info]\nScriptType: v4.00+\n\n[V4+ Styles]\nFormat: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\nStyle: Default,Arial,48,&H00FFFFFF,&H000000FF,&H00000000,&H00000000,0,0,0,0,100,100,0,0,1,2,2,2,10,10,10,1\n\n[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\nDialogue: 0,0:00:01.00,0:00:03.00,Default,,0,0,0,,{\\i1}Stantını{\\i0} çağır!\nDialogue: 0,0:00:04.00,0:00:06.00,Default,,0,0,0,,Her şey yolunda.\n");
    std::fs::write(script.parent().unwrap().join("terimler.tsv"), "kaynak\ttürkçe\tyasaklar\tnot\nStand\tStand\tStant|Ruh\t\n").unwrap();
    let (ctx, mut app) = app_with(&[]);
    app.open_path(script);
    frame(&ctx, &mut app, vec![]);
    assert!(app.osi.glossary.is_some(), "terimler.tsv next to the script is loaded");
    assert!(app.qc_issues.iter().any(|i| i.code == "glossary" && i.index == 0), "glossary findings appear in QC");
    app.osi.gl_issues = app.glossary_issues();
    app.glossary_fix(None);
    assert_eq!(app.doc.events[0].text, "{\\i1}Standını{\\i0} çağır!", "only the text outside tags changes, the suffix stays");
    assert!(app.glossary_issues().is_empty());
}

// ---------------------------------------------------------------- Aegisub line tools (0.5)

fn frame_mod(ctx: &egui::Context, app: &mut App, events: Vec<UiEvent>, modifiers: Modifiers) -> egui::FullOutput {
    let mut input = RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1400.0, 900.0))), modifiers, ..Default::default() };
    input.events = events;
    ctx.run(input, |ctx| app.frame(ctx))
}

fn copied_text(out: &egui::FullOutput) -> Option<String> {
    out.platform_output.commands.iter().find_map(|c| match c { egui::OutputCommand::CopyText(s) => Some(s.clone()), _ => None })
}

fn texts(app: &App) -> Vec<String> { app.doc.events.iter().map(|e| e.text.clone()).collect() }
fn sel(app: &App) -> Vec<usize> { app.selection.iter().copied().collect() }

#[test]
fn copy_cut_paste_and_paste_over_lines() {
    let (ctx, mut app) = app_with(&[(0, 1000, "bir"), (1000, 2000, "iki"), (2000, 3000, "üç")]);
    frame(&ctx, &mut app, vec![]);
    app.selection = [0, 1].into_iter().collect();
    let out = frame(&ctx, &mut app, vec![UiEvent::Copy]);
    let clip = copied_text(&out).expect("Ctrl+C copies the selected rows");
    assert_eq!(clip.lines().count(), 2);
    assert!(clip.starts_with("Dialogue: 0,0:00:00.00,0:00:01.00,Default,"), "{}", clip);
    frame(&ctx, &mut app, vec![UiEvent::Cut]);
    assert_eq!(texts(&app), ["üç"]);
    // paste goes above the active line and selects what was pasted
    frame(&ctx, &mut app, vec![UiEvent::Paste(clip.clone())]);
    assert_eq!(texts(&app), ["bir", "iki", "üç"]);
    assert_eq!(sel(&app), vec![0, 1]);
    assert_eq!(app.doc.events[1].start, 1000);
    // Ctrl+Shift+V asks which fields to overwrite; with one line selected it goes downwards
    app.select_only(1);
    frame_mod(&ctx, &mut app, vec![UiEvent::Paste("Two\nThree\nFour".into())], Modifiers::COMMAND | Modifiers::SHIFT);
    assert!(app.edit.paste_over.is_some(), "paste over waits for the field choice");
    let out = frame(&ctx, &mut app, vec![]);
    assert!(find_text_pos(&out, "Değişecek alanlar").is_some() || find_text_pos(&out, "Fields to overwrite").is_some());
    app.apply_paste_over(asscore::lines::field::TEXT);
    assert_eq!(texts(&app), ["bir", "Two", "Three"]);
    assert_eq!(app.doc.events[2].start, 2000, "times are kept");
    // with several lines selected it goes over exactly those
    app.selection = [0, 2].into_iter().collect();
    app.begin_paste_over("Dialogue: 0,0:01:00.00,0:01:02.00,Sign,,0,0,0,,A\nDialogue: 0,0:01:03.00,0:01:04.00,Sign,,0,0,0,,B");
    app.apply_paste_over(asscore::lines::field::START | asscore::lines::field::STYLE);
    assert_eq!(app.doc.events[0].start, 60_000);
    assert_eq!(app.doc.events[2].style, "Sign");
    assert_eq!(app.doc.events[1].style, "Default");
    assert_eq!(texts(&app), ["bir", "Two", "Three"]);
    app.do_undo();
    assert_eq!(app.doc.events[0].start, 0);
}

#[test]
fn ctrl_v_in_the_text_box_pastes_rows_as_lines_and_text_as_text() {
    let (ctx, mut app) = app_with(&[(0, 1000, "a")]);
    frame(&ctx, &mut app, vec![]);
    ctx.memory_mut(|m| m.request_focus(egui::Id::new("text_edit")));
    frame(&ctx, &mut app, vec![]);
    frame(&ctx, &mut app, vec![UiEvent::Paste("bc".into())]);
    assert_eq!(texts(&app), ["abc"], "plain text is pasted into the text box");
    frame(&ctx, &mut app, vec![UiEvent::Paste("Dialogue: 0,0:00:05.00,0:00:06.00,Default,,0,0,0,,yeni".into())]);
    assert_eq!(texts(&app), ["yeni", "abc"], "subtitle rows become lines (Aegisub)");
}

#[test]
fn select_lines_matches_turkish_case_and_combines() {
    let (ctx, mut app) = app_with(&[(0, 1, "Işık"), (1, 2, "karanlık"), (2, 3, "ışıklar"), (3, 4, "gölge")]);
    app.doc.events[1].style = "Sign".into();
    app.edit.sel_opts = SelectOpts { pattern: "IŞIK".into(), ..Default::default() };
    app.run_select_lines();
    assert_eq!(sel(&app), vec![0, 2]);
    app.edit.sel_opts = SelectOpts { pattern: "sign".into(), field: 1, mode: MatchMode::Exact, ..Default::default() };
    app.edit.sel_action = SelectAction::Add;
    app.run_select_lines();
    assert_eq!(sel(&app), vec![0, 1, 2]);
    app.edit.sel_opts = SelectOpts { pattern: "lar$".into(), mode: MatchMode::Regex, ..Default::default() };
    app.edit.sel_action = SelectAction::Subtract;
    app.run_select_lines();
    assert_eq!(sel(&app), vec![0, 1]);
    // nothing left → the active line stays selected
    app.edit.sel_action = SelectAction::Intersect;
    app.edit.sel_opts = SelectOpts { pattern: "gölge".into(), ..Default::default() };
    app.run_select_lines();
    assert_eq!(app.selection.len(), 1);
    // the window draws and Enter in the pattern box runs it
    app.edit.sel_action = SelectAction::Set;
    app.run_command(&ctx, "select_lines");
    frame(&ctx, &mut app, vec![]);
    let out = frame(&ctx, &mut app, vec![]);
    assert!(find_text_pos(&out, "Seçimden çıkar").is_some() || find_text_pos(&out, "Remove from selection").is_some());
}

#[test]
fn style_storage_copies_between_script_and_catalog() {
    let (ctx, mut app) = app_with(&[(0, 1000, "a"), (1000, 2000, "b")]);
    app.settings.style_catalog = format!("seri-{}", std::process::id());
    let mut sign = Style::default();
    sign.name = "Sign".into();
    sign.alignment = 8;
    app.styles_into_storage(vec![sign.clone()]);
    // next episode: the script lacks the style, the storage (read back from disk) has it
    app.edit.storage_loaded = None;
    app.ensure_storage();
    assert_eq!(app.edit.storage.iter().find(|s| s.name == "Sign").map(|s| s.alignment), Some(8));
    assert!(app.edit.catalogs.contains(&app.settings.style_catalog));
    let st = app.edit.storage.clone();
    assert_eq!(app.styles_into_script(st.clone()), (1, 0));
    assert_eq!(app.styles_into_script(st), (0, 1), "same name is replaced, not doubled");
    app.selection = [0, 1].into_iter().collect();
    app.apply_style_to_selection("Sign");
    assert!(app.doc.events.iter().all(|e| e.style == "Sign"));
    app.run_command(&ctx, "styles");
    frame(&ctx, &mut app, vec![]);
    let out = frame(&ctx, &mut app, vec![]);
    assert!(find_text_pos(&out, "Depoya kopyala").is_some() || find_text_pos(&out, "Copy to storage").is_some());
    app.do_undo();
    assert!(app.doc.events.iter().all(|e| e.style == "Default"));
}

#[test]
fn multiple_replace_previews_applies_and_undoes() {
    let (ctx, mut app) = app_with(&[(0, 1, "Merhaba !"), (1, 2, "{\\an8}Nasılsın ?"), (2, 3, "tamam")]);
    app.edit.mr_loaded = true;
    app.edit.mr_groups = vec![replace::Group { name: "T".into(), enabled: true, rules: vec![replace::Rule::new(replace::Kind::Regex, " +([!?])", "$1", "")] }];
    app.preview_rules();
    assert_eq!(app.edit.mr_preview.len(), 2);
    app.edit.mr_preview[1].2 = false;
    app.apply_rules();
    assert_eq!(texts(&app), ["Merhaba!", "{\\an8}Nasılsın ?", "tamam"]);
    app.do_undo();
    assert_eq!(app.doc.events[0].text, "Merhaba !");
    app.edit.mr_groups[0].rules[0].find = "(".into();
    app.preview_rules();
    assert!(app.edit.mr_preview.is_empty() && app.edit.mr_msg.starts_with("T / 1"));
    app.run_command(&ctx, "multi_replace");
    frame(&ctx, &mut app, vec![]);
    frame(&ctx, &mut app, vec![]);
}

#[test]
fn translation_assistant_replaces_text_blocks_and_moves_on() {
    let (ctx, mut app) = app_with(&[(0, 1000, "{\\i1}Hello{\\i0} world"), (1000, 2000, "{\\an8}"), (2000, 3000, "Bye")]);
    frame(&ctx, &mut app, vec![]);
    app.run_command(&ctx, "translate_assist");
    frame(&ctx, &mut app, vec![]);
    frame(&ctx, &mut app, vec![]);
    frame(&ctx, &mut app, vec![UiEvent::Text("Merhaba".into())]);
    frame(&ctx, &mut app, vec![key(Key::Enter)]);
    assert_eq!(app.doc.events[0].text, "{\\i1}Merhaba{\\i0} world");
    assert_eq!((app.edit.tr_line, app.edit.tr_block), (0, 1));
    frame(&ctx, &mut app, vec![]);
    frame(&ctx, &mut app, vec![UiEvent::Text(" dünya".into())]);
    frame(&ctx, &mut app, vec![key(Key::Enter)]);
    assert_eq!(app.doc.events[0].text, "{\\i1}Merhaba{\\i0} dünya");
    // the line with only tags is skipped; the grid follows
    assert_eq!((app.edit.tr_line, app.active), (2, 2));
    // an empty box keeps the original; past the last line there is a note
    frame(&ctx, &mut app, vec![]);
    frame(&ctx, &mut app, vec![key(Key::Enter)]);
    assert_eq!(app.doc.events[2].text, "Bye");
    assert!(!app.edit.tr_msg.is_empty());
    assert_eq!(app.doc.events.len(), 3, "Enter in the assistant never adds lines");
}

#[test]
fn styling_assistant_applies_the_typed_style_and_moves_on() {
    let (ctx, mut app) = app_with(&[(0, 1000, "a"), (1000, 2000, "b")]);
    let mut s = Style::default();
    s.name = "Sign - Top".into();
    app.doc.styles.push(s);
    frame(&ctx, &mut app, vec![]);
    app.run_command(&ctx, "style_assist");
    frame(&ctx, &mut app, vec![]);
    frame(&ctx, &mut app, vec![]);
    frame(&ctx, &mut app, vec![UiEvent::Text("top".into())]);
    frame(&ctx, &mut app, vec![key(Key::Enter)]);
    assert_eq!(app.doc.events[0].style, "Sign - Top");
    assert_eq!(app.active, 1);
}

#[test]
fn shift_times_amounts_and_scopes() {
    let (_ctx, mut app) = app_with(&[(0, 1000, "a"), (1000, 2000, "b"), (2000, 3000, "c")]);
    app.select_only(1);
    app.shift_scope = 2;
    app.shift_ms = "500".into();
    app.apply_shift();
    let starts = |app: &App| app.doc.events.iter().map(|e| e.start).collect::<Vec<_>>();
    assert_eq!(starts(&app), vec![0, 1500, 2500], "a whole number is milliseconds, from the selection onward");
    app.shift_scope = 0;
    app.shift_ms = "-0:00:00.50".into();
    app.apply_shift();
    assert_eq!(starts(&app), vec![0, 1000, 2000]);
    app.shift_scope = 1;
    app.shift_ms = "1.5".into();
    app.apply_shift();
    assert_eq!(starts(&app), vec![0, 2500, 2000]);
}

fn with_fake_video(ctx: &egui::Context, app: &mut App) {
    app.media.info = Some(yuzu_media::MediaInfo { width: 1920, height: 1080, fps_num: 24000, fps_den: 1001, duration_ms: 60_000, has_video: true, has_audio: false, frames: 0 });
    app.media.texture = Some(ctx.load_texture("test-frame", egui::ColorImage::filled([64, 36], Color32::BLACK), Default::default()));
    app.doc.info_set("PlayResX", "1920");
    app.doc.info_set("PlayResY", "1080");
    frame(ctx, app, vec![]);
    frame(ctx, app, vec![]);
}

/// Screen point of a script position on the drawn video.
fn on_video(app: &App, x: f64, y: f64) -> Pos2 {
    let r = app.video_rect.expect("video drawn");
    Pos2::new(r.left() + (x / 1920.0) as f32 * r.width(), r.top() + (y / 1080.0) as f32 * r.height())
}

fn drag(ctx: &egui::Context, app: &mut App, from: Pos2, to: Pos2, mods: Modifiers) {
    frame_mod(ctx, app, vec![UiEvent::PointerMoved(from)], mods);
    frame_mod(ctx, app, vec![UiEvent::PointerButton { pos: from, button: PointerButton::Primary, pressed: true, modifiers: mods }], mods);
    for k in 1..=6 { frame_mod(ctx, app, vec![UiEvent::PointerMoved(from + (to - from) * (k as f32 / 6.0))], mods); }
    frame_mod(ctx, app, vec![UiEvent::PointerButton { pos: to, button: PointerButton::Primary, pressed: false, modifiers: mods }], mods);
    frame(ctx, app, vec![]);
}

/// Drag / click in script coordinates of the video.
fn vdrag(ctx: &egui::Context, app: &mut App, from: (f64, f64), to: (f64, f64), mods: Modifiers) {
    let (a, b) = (on_video(app, from.0, from.1), on_video(app, to.0, to.1));
    drag(ctx, app, a, b, mods);
}

fn vclick(ctx: &egui::Context, app: &mut App, at: (f64, f64), button: PointerButton) {
    let p = on_video(app, at.0, at.1);
    click(ctx, app, p, button);
}

fn nums(text: &str, tag: &str) -> Vec<f64> {
    let a = asscore::typeset::get_tag(text, tag).unwrap_or_default();
    a.trim_start_matches('(').trim_end_matches(')').split(',').filter_map(|x| x.trim().parse().ok()).collect()
}

fn close(a: &[f64], b: &[f64], tol: f64) -> bool { a.len() == b.len() && a.iter().zip(b).all(|(x, y)| (x - y).abs() <= tol) }

#[test]
fn visual_tools_position_rotate_scale_and_clips() {
    use visual::Tool;
    let (ctx, mut app) = app_with(&[(0, 5000, "{\\an5}Merhaba")]);
    with_fake_video(&ctx, &mut app);
    let tol = 1920.0 / app.video_rect.unwrap().width() as f64 + 0.6;
    // position: grab the anchor (centre of the frame for \an5) and move it; one drag, one undo step
    app.vis.tool = Tool::Pos;
    let undo0 = app.undo.len();
    vdrag(&ctx, &mut app, (960.0, 540.0), (1160.0, 640.0), Modifiers::NONE);
    let t0 = app.doc.events[0].text.clone();
    assert!(close(&nums(&t0, "pos"), &[1160.0, 640.0], tol), "{}", t0);
    assert_eq!(app.undo.len(), undo0 + 1, "a drag is one undo step");
    // Shift keeps the drag on one axis
    vdrag(&ctx, &mut app, (1160.0, 640.0), (1460.0, 660.0), Modifiers::SHIFT);
    let p = nums(&app.doc.events[0].text, "pos");
    assert!((p[1] - 640.0).abs() < 0.01 && (p[0] - 1460.0).abs() <= tol, "{:?}", p);
    app.do_undo();
    app.do_undo();
    assert_eq!(app.doc.events[0].text, "{\\an5}Merhaba");

    // rotation around \org: a quarter turn counter-clockwise, snapped with Shift
    app.doc.events[0].text = "{\\pos(960,540)}x".into();
    app.vis.tool = Tool::Rotate;
    frame(&ctx, &mut app, vec![]);
    vdrag(&ctx, &mut app, (1260.0, 540.0), (962.0, 240.0), Modifiers::SHIFT);
    assert_eq!(asscore::typeset::get_num(&app.doc.events[0].text, "frz"), Some(90.0), "{}", app.doc.events[0].text);
    // the next drag is relative: another 15° from where it is
    vdrag(&ctx, &mut app, (960.0, 240.0), (960.0 - 300.0 * 0.2588, 540.0 - 300.0 * 0.9659), Modifiers::SHIFT);
    assert_eq!(asscore::typeset::get_num(&app.doc.events[0].text, "frz"), Some(105.0));

    // scale with Shift keeps the ratio: 100 screen px to the right = ×1.5
    app.vis.tool = Tool::Scale;
    frame(&ctx, &mut app, vec![]);
    let a = on_video(&app, 960.0, 540.0);
    drag(&ctx, &mut app, a, a + Vec2::new(100.0, 0.0), Modifiers::SHIFT);
    let t = app.doc.events[0].text.clone();
    assert!(close(&nums(&t, "fscx"), &[150.0], 0.6) && close(&nums(&t, "fscy"), &[150.0], 0.6), "{}", t);

    // rectangular clip: draw, then pull a corner, then move it by its inside
    app.vis.tool = Tool::Clip;
    frame(&ctx, &mut app, vec![]);
    vdrag(&ctx, &mut app, (100.0, 100.0), (500.0, 300.0), Modifiers::NONE);
    assert!(close(&nums(&app.doc.events[0].text, "clip"), &[100.0, 100.0, 500.0, 300.0], tol), "{}", app.doc.events[0].text);
    let c = nums(&app.doc.events[0].text, "clip");
    vdrag(&ctx, &mut app, (c[2], c[3]), (700.0, 400.0), Modifiers::NONE);
    let c = nums(&app.doc.events[0].text, "clip");
    assert!(close(&c, &[100.0, 100.0, 700.0, 400.0], 2.0 * tol), "{:?}", c);
    vdrag(&ctx, &mut app, (300.0, 200.0), (400.0, 250.0), Modifiers::NONE);
    let d = nums(&app.doc.events[0].text, "clip");
    assert!(close(&[d[0] - c[0], d[1] - c[1], d[2] - c[2], d[3] - c[3]], &[100.0, 50.0, 100.0, 50.0], 2.0 * tol), "{:?} → {:?}", c, d);

    // vector clip: three clicks make a triangle, a drag moves a point, right-click deletes one
    app.vis.tool = Tool::VClip;
    frame(&ctx, &mut app, vec![]);
    for (x, y) in [(200.0, 200.0), (600.0, 200.0), (400.0, 500.0)] { vclick(&ctx, &mut app, (x, y), PointerButton::Primary); }
    let drawing = |app: &App| match asscore::typeset::get_clip(&app.doc.events[0].text) { Some(asscore::typeset::Clip::Vector { drawing, .. }) => drawing.points(), c => panic!("{:?}", c) };
    let pts = drawing(&app);
    assert_eq!(pts.len(), 3, "{}", app.doc.events[0].text);
    assert!(close(&[pts[2].0, pts[2].1], &[400.0, 500.0], tol));
    vdrag(&ctx, &mut app, (pts[1].0, pts[1].1), (700.0, 300.0), Modifiers::NONE);
    let pts = drawing(&app);
    assert!(close(&[pts[1].0, pts[1].1], &[700.0, 300.0], 2.0 * tol), "{:?}", pts);
    vclick(&ctx, &mut app, (pts[0].0, pts[0].1), PointerButton::Secondary);
    assert_eq!(drawing(&app).len(), 2);
    assert!(app.doc.events[0].text.contains("\\clip(m "), "{}", app.doc.events[0].text);
}

#[test]
fn visual_move_tool_drags_both_ends() {
    use visual::Tool;
    let (ctx, mut app) = app_with(&[(0, 5000, "{\\move(100,100,500,300,0,5000)}x")]);
    with_fake_video(&ctx, &mut app);
    let tol = 2.0 * (1920.0 / app.video_rect.unwrap().width() as f64) + 1.0;
    app.vis.tool = Tool::Pos;
    frame(&ctx, &mut app, vec![]);
    vdrag(&ctx, &mut app, (500.0, 300.0), (900.0, 700.0), Modifiers::NONE);
    let m = asscore::typeset::get_move(&app.doc.events[0].text).expect("still a \\move");
    assert!((m.x1, m.y1) == (100.0, 100.0), "{:?}", m);
    assert!((m.x2 - 900.0).abs() <= tol && (m.y2 - 700.0).abs() <= tol, "{:?}", m);
    assert_eq!(m.times, Some((0, 5000)));
    vdrag(&ctx, &mut app, (100.0, 100.0), (50.0, 80.0), Modifiers::NONE);
    let m = asscore::typeset::get_move(&app.doc.events[0].text).unwrap();
    assert!((m.x1 - 50.0).abs() <= tol && (m.y1 - 80.0).abs() <= tol, "{:?}", m);
}

#[test]
fn karaoke_boundaries_drag_in_the_timing_view() {
    let (ctx, mut app) = app_with(&[(1000, 3000, "{\\k50}ka{\\k100}ra{\\k50}o")]);
    app.media.audio = Some(PathBuf::from("/nonexistent.wav"));
    app.media.view_start = 0.0;
    app.media.ms_per_px = 10.0;
    app.media.snap_keyframes = false;
    app.kara_audio = true;
    frame(&ctx, &mut app, vec![]);
    let out = frame(&ctx, &mut app, vec![]);
    assert!(find_text_pos(&out, "ra").is_some(), "syllables are labelled");
    let r = app.wave_rect.expect("timing view drawn");
    let x_of = |t: f64| r.left() + (t / 10.0) as f32;
    let undo0 = app.undo.len();
    // the boundary between "ka" and "ra" (1500 ms) goes to 1800 ms: ka gets longer, ra shorter
    drag(&ctx, &mut app, Pos2::new(x_of(1500.0), r.center().y), Pos2::new(x_of(1800.0), r.center().y), Modifiers::NONE);
    assert_eq!(app.doc.events[0].text, "{\\k80}ka{\\k70}ra{\\k50}o");
    assert_eq!((app.doc.events[0].start, app.doc.events[0].end), (1000, 3000), "the line itself keeps its times");
    assert_eq!(app.undo.len(), undo0 + 1, "one drag, one undo step");
    // a boundary cannot pass its neighbour
    drag(&ctx, &mut app, Pos2::new(x_of(2500.0), r.center().y), Pos2::new(x_of(1200.0), r.center().y), Modifiers::NONE);
    assert_eq!(app.doc.events[0].text, "{\\k80}ka{\\k0}ra{\\k120}o");
    // the line edges still drag as before
    drag(&ctx, &mut app, Pos2::new(x_of(3000.0), r.center().y), Pos2::new(x_of(3400.0), r.center().y), Modifiers::NONE);
    assert_eq!(app.doc.events[0].end, 3400);
    app.do_undo();
    app.do_undo();
    app.do_undo();
    assert_eq!(app.doc.events[0].text, "{\\k50}ka{\\k100}ra{\\k50}o");
}

// ---------------------------------------------------------------- automation (0.6)

/// Run frames until `cond` holds (the automation thread answers asynchronously).
fn wait_for(ctx: &egui::Context, app: &mut App, what: &str, cond: impl Fn(&App) -> bool) {
    for _ in 0..400 {
        frame(ctx, app, vec![]);
        if cond(app) { return; }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    panic!("timed out waiting for {}", what);
}

fn script_dir(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("yuzu-gui-scripts-{}-{}", std::process::id(), name));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for (f, src) in files { std::fs::write(dir.join(f), src).unwrap(); }
    dir
}

#[test]
fn macros_are_greyed_by_validation_and_not_run() {
    let dir = script_dir("valid", &[("v.lua", r#"
script_name = "Doğrulama"
aegisub.register_macro("İkili", "two or more lines", function(subs, sel)
  for _, i in ipairs(sel) do local l = subs[i]; l.text = l.text .. "!"; subs[i] = l end
end, function(subs, sel) return #sel >= 2 end, function(subs, sel, act) return subs[act].comment end)
"#)]);
    let (ctx, mut app) = app_with(&[(0, 1000, "a"), (1000, 2000, "b")]);
    app.settings.script_dirs = vec![dir];
    app.reload_scripts();
    wait_for(&ctx, &mut app, "scripts", |a| a.scripts.len() == 1);
    assert!(app.scripts[0].macros[0].has_validate && app.scripts[0].macros[0].has_active);
    app.request_macro_states();
    wait_for(&ctx, &mut app, "states", |a| a.macro_state(0, 0).is_some());
    assert_eq!(app.macro_state(0, 0).map(|s| (s.enabled, s.checked)), Some((false, Some(false))));
    // running it anyway (palette, shortcut) does nothing and says why
    app.run_macro(0, 0, "İkili");
    wait_for(&ctx, &mut app, "refusal", |a| a.busy.is_none());
    assert_eq!(texts(&app), ["a", "b"]);
    assert!(app.last_status.contains("İkili"), "{}", app.last_status);
    // two lines selected, the active one a comment: available and checked
    app.doc.events[1].comment = true;
    app.touch();
    app.selection = [0, 1].into_iter().collect();
    app.active = 1;
    app.request_macro_states();
    wait_for(&ctx, &mut app, "states", |a| a.macro_state(0, 0).map(|s| s.enabled).unwrap_or(false));
    assert_eq!(app.macro_state(0, 0).and_then(|s| s.checked), Some(true));
    app.run_macro(0, 0, "İkili");
    wait_for(&ctx, &mut app, "macro", |a| a.busy.is_none());
    assert_eq!(texts(&app), ["a!", "b!"]);
}

#[test]
fn scripts_reload_when_they_change_and_errors_have_tracebacks() {
    let dir = script_dir("reload", &[("r.lua", "aegisub.register_macro('Eski', 'd', function() end)\n")]);
    let (ctx, mut app) = app_with(&[(0, 1000, "a")]);
    app.settings.script_dirs = vec![dir.clone()];
    app.settings.auto_reload = true;
    app.reload_scripts();
    wait_for(&ctx, &mut app, "scripts", |a| a.scripts.len() == 1 && a.scripts[0].macros.len() == 1);
    std::fs::write(dir.join("r.lua"), "aegisub.register_macro('Yeni', 'd', function() end)\naegisub.register_macro('İkinci', 'd', function() end)\n").unwrap();
    app.auto.last_check = std::time::Instant::now() - std::time::Duration::from_secs(5);
    wait_for(&ctx, &mut app, "reload", |a| a.scripts.first().map(|s| s.macros.len() == 2).unwrap_or(false));
    assert_eq!(app.scripts[0].macros[0].name, "Yeni");
    // a broken script shows its error with a traceback in the manager
    std::fs::write(dir.join("bozuk.lua"), "local function f() local x = nil; return x.y end\nlocal r = f()\n").unwrap();
    app.auto.last_check = std::time::Instant::now() - std::time::Duration::from_secs(5);
    wait_for(&ctx, &mut app, "reload", |a| a.scripts.len() == 2);
    let broken = app.scripts.iter().find(|s| s.error.is_some()).expect("the broken script is listed");
    assert!(broken.error.as_ref().unwrap().contains("stack traceback"), "{:?}", broken.error);
    app.run_command(&ctx, "automation_manager");
    frame(&ctx, &mut app, vec![]);
    let out = frame(&ctx, &mut app, vec![]);
    // clicking the script's row shows its error
    let row = find_text_pos(&out, "bozuk.lua").expect("listed in the manager");
    click(&ctx, &mut app, row, PointerButton::Primary);
    assert_eq!(app.auto.picked, app.scripts.iter().position(|s| s.error.is_some()));
    let out = frame(&ctx, &mut app, vec![]);
    assert!(find_text_pos(&out, "stack traceback").is_some(), "the manager shows the error");
}

#[test]
fn extradata_markers_stay_hidden_and_intact() {
    let (ctx, mut app) = app_with(&[(0, 1000, "{=1}abc")]);
    app.doc.extradata = vec![asscore::Extra { id: 1, key: "k".into(), value: "ev".into() }];
    frame(&ctx, &mut app, vec![]);
    let out = frame(&ctx, &mut app, vec![]);
    assert!(find_text_pos(&out, "{=1}").is_none(), "the marker is not shown");
    ctx.memory_mut(|m| m.request_focus(egui::Id::new("text_edit")));
    frame(&ctx, &mut app, vec![]);
    frame(&ctx, &mut app, vec![UiEvent::Text("d".into())]);
    assert_eq!(app.doc.events[0].text, "{=1}abcd");
    app.prefix_tag("{\\an8}");
    assert_eq!(app.doc.events[0].text, "{=1}{\\an8}abcd");
}
