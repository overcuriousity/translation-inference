import { test } from "node:test";
import assert from "node:assert/strict";
import { exportText } from "./talk.js";

test("a conversation exports as speaker, words and translation", () => {
  const out = exportText([{ fromName: "English", source: "Hello", translation: "Hallo" }]);
  assert.equal(out, "English: Hello\n→ Hallo");
});
