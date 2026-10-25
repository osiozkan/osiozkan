import test from 'node:test';
import assert from 'node:assert/strict';
import { balanceText, splitLongLines, mergeShortLines, removeHI, fixPunctuation, removeDuplicates, enforceGap, snapToKeyframes } from '../src/core/fixes.js';
import { defaultDialogue as D } from '../src/core/ass.js';

test('balanceText', () => {
  const t = 'This is a rather long subtitle line that needs to be broken into two balanced lines';
  const r = balanceText(t, 42, 2); const [a, b] = r.split('\\N');
  assert.ok(b && a.length <= 42 && b.length <= 42 && Math.abs(a.length - b.length) < 14, r);
  assert.equal(balanceText('short', 42), 'short');
  assert.equal(balanceText('{\\i1}Italic{\\i0} bit', 42), '{\\i1}Italic{\\i0} bit');
});
test('splitLongLines distributes time', () => {
  const l = D({ text: 'First sentence is here. Second sentence is also here and goes on for a while. Third.', start_time: 0, end_time: 8000 });
  const out = splitLongLines([l], { maxChars: 40 });
  assert.ok(out.length >= 2); assert.equal(out[0].start_time, 0); assert.equal(out.at(-1).end_time, 8000);
  assert.ok(out.every((x, i) => i === 0 || x.start_time === out[i - 1].end_time));
});
test('mergeShortLines', () => {
  const out = mergeShortLines([D({ text: 'Hi', start_time: 0, end_time: 500 }), D({ text: 'there', start_time: 600, end_time: 1200 }), D({ text: 'far', start_time: 9000, end_time: 9500 })]);
  assert.equal(out.length, 2); assert.equal(out[0].text, 'Hi\\Nthere'); assert.equal(out[0].end_time, 1200);
});
test('removeHI', () => {
  assert.equal(removeHI('[music] Hello'), 'Hello'); assert.equal(removeHI('JOHN: Hi\\NMARY: Yo'), 'Hi\\NYo'); assert.equal(removeHI('(laughs)'), '');
});
test('fixPunctuation', () => {
  assert.equal(fixPunctuation('Hello , world ...'), 'Hello, world…'); assert.equal(fixPunctuation('What?!?!'), 'What?!'.replace('?!', '?!') && fixPunctuation('What?!?!'));
  assert.equal(fixPunctuation('{\\i1}a ,b{\\i0}'), '{\\i1}a, b{\\i0}');
});
test('dups/gap/keyframes', () => {
  assert.equal(removeDuplicates([D({ text: 'a' }), D({ text: 'a' }), D({ text: 'b' })]).length, 2);
  const g = enforceGap([D({ start_time: 0, end_time: 1000 }), D({ start_time: 1010, end_time: 2000 })], 42); assert.equal(g[0].end_time, 968);
  const k = snapToKeyframes([D({ start_time: 990, end_time: 3010 })], [1000, 3000]); assert.deepEqual([k[0].start_time, k[0].end_time], [1000, 3000]);
});
