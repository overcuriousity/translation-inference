import { test } from "node:test";
import assert from "node:assert/strict";
import { EventParser, vadStep, VAD } from "./common.js";

test("events split across chunks are put back together", () => {
  const p = new EventParser();
  assert.deepEqual(p.push('event: text\ndata: {"text":"Hal'), []);
  assert.deepEqual(p.push('lo"}\n\nevent: done\r\ndata: {}\r\n\r\n'), [
    { event: "text", data: '{"text":"Hallo"}' },
    { event: "done", data: "{}" },
  ]);
});

test("comments and keep-alives carry no event", () => {
  assert.deepEqual(new EventParser().push(":\n\n: ping\n\n"), []);
});

test("several data lines join with a newline", () => {
  assert.deepEqual(new EventParser().push("data: a\ndata: b\n\n"), [{ event: "message", data: "a\nb" }]);
});

test("recording stops only after speech and then a long enough pause", () => {
  let s = { speechStart: null, silenceStart: null };
  const quiet = VAD.threshold / 2;
  const loud = VAD.threshold * 2;
  let r = vadStep(s, quiet, 0);
  assert.equal(r.stop, false, "silence before speaking never stops");
  r = vadStep(r.state, quiet, 5000);
  assert.equal(r.stop, false);
  r = vadStep({ speechStart: null, silenceStart: null }, loud, 0);
  r = vadStep(r.state, loud, 600);
  r = vadStep(r.state, quiet, 700);
  assert.equal(r.stop, false, "a short pause does not stop");
  r = vadStep(r.state, quiet, 700 + VAD.graceMs);
  assert.equal(r.stop, true);
});
