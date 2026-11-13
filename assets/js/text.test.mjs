import { test } from "node:test";
import assert from "node:assert/strict";
import { countLabel, significantChange } from "./text.js";

test("detection runs again only after a real change", () => {
  const t = "Dies ist ein Satz auf Deutsch, der lang genug ist. ".repeat(3);
  assert.equal(significantChange(t, ""), true);
  assert.equal(significantChange(t + " Und", t), false);
  assert.equal(significantChange("Something else entirely, in English now.", t), true);
});

test("the count shows the limit only when there is one", () => {
  assert.equal(countLabel(0, 0), "");
  assert.equal(countLabel(1234, 0), "1,234");
  assert.equal(countLabel(12, 16000), "12 / 16,000");
});
