import { describe, it } from "node:test";
import { ResolverFactory } from "../index.js";
import * as assert from "node:assert";
import * as path from "node:path";
import { fileURLToPath } from "url";

const fixtureDir = fileURLToPath(
  new URL("../../fixtures/enhanced_resolve/test/fixtures", import.meta.url)
);
const fixtures = path.join(fixtureDir, "incorrect-package");

function p(...args) {
  return path.join(fixtures, ...args);
}

describe("incorrect description file", () => {
  const resolver = new ResolverFactory({});

  it("should not resolve main in incorrect description file #1", () => {
    const result = resolver.sync(p("pack1"), ".");
    assert.ok(result.error);
  });

  it("should not resolve main in incorrect description file #2", () => {
    const result = resolver.sync(p("pack2"), ".");
    assert.ok(result.error);
  });
});
