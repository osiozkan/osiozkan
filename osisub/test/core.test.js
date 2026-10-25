import test from 'node:test';
import assert from 'node:assert/strict';
import { parseAss, writeAss, dialogues, styles, getInfo } from '../src/core/ass.js';
import { parseSrt, writeSrt, writeVtt } from '../src/core/srt.js';
import { runQC } from '../src/core/qc.js';
import { parseAssTime, formatAssTime } from '../src/core/time.js';
import { karaokeSyllables } from '../src/core/tags.js';

const ASS = `[Script Info]
Title: t
PlayResX: 1280
PlayResY: 720

[V4+ Styles]
Format: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding
Style: Default,Arial,40,&H00FFFFFF,&H000000FF,&H00000000,&H00000000,-1,0,0,0,100,100,0,0,1,2,2,2,10,10,10,1

[Events]
Format: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text
Dialogue: 0,0:00:01.00,0:00:03.50,Default,,0,0,0,,Hello, {\\i1}world{\\i0}\\Nline2
Comment: 0,0:00:04.00,0:00:05.00,Default,,0,0,0,,note
`;

test('ass roundtrip', () => {
  const l = parseAss(ASS);
  assert.equal(getInfo(l, 'PlayResX'), '1280');
  assert.equal(styles(l)[0].bold, true);
  const d = dialogues(l); assert.equal(d.length, 2);
  assert.equal(d[0].text, 'Hello, {\\i1}world{\\i0}\\Nline2'); assert.equal(d[0].end_time, 3500); assert.equal(d[1].comment, true);
  const l2 = parseAss(writeAss(l));
  assert.deepEqual(dialogues(l2), d); assert.deepEqual(styles(l2), styles(l));
  assert.equal(writeAss(l2), writeAss(l));
});
test('times', () => { assert.equal(parseAssTime('1:02:03.45'), 3723450); assert.equal(formatAssTime(3723450), '1:02:03.45'); });
test('srt/vtt', () => {
  const s = parseSrt('1\r\n00:00:01,000 --> 00:00:02,500\r\n<i>Hi</i>\r\nthere\r\n\r\n2\r\n00:00:03,000 --> 00:00:04,000\r\nBye\r\n');
  assert.equal(s.length, 2); assert.equal(s[0].text, '{\\i1}Hi{\\i0}\\Nthere');
  assert.ok(writeSrt(s).includes('<i>Hi</i>\nthere') && writeSrt(s).startsWith('1\r\n00:00:01,000'));
  assert.ok(writeVtt(s).startsWith('WEBVTT') && writeVtt(s).includes('00:00:01.000 -->'));
});
test('qc', () => {
  const l = parseAss(ASS); l.push({ ...dialogues(l)[0], style: 'Nope', start_time: 2000, end_time: 2100 });
  const codes = runQC(l).map(i => i.code);
  assert.ok(['style', 'dur-short', 'cps'].every(c => codes.includes(c)), codes.join());
});
test('karaoke syllables', () => assert.deepEqual(karaokeSyllables('{\\k50}a{\\kf20}b'), [{ dur: 500, text: 'a' }, { dur: 200, text: 'b' }]));
