import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

test("built page contains the declared observable content", async () => {
  const html = await readFile("dist/index.html", "utf8");
  assert.match(html, /Deterministic fixture/);
});
