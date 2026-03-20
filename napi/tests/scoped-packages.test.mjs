import { describe, it } from "node:test";
import { ResolverFactory } from "../index.js";
import * as assert from "node:assert";
import * as path from "node:path";
import { fileURLToPath } from "url";

const fixtureDir = fileURLToPath(
  new URL("../../fixtures/enhanced_resolve/test/fixtures", import.meta.url)
);
const fixture = path.join(fixtureDir, "scoped");

describe("scoped-packages", () => {
  const resolver = new ResolverFactory({
    aliasFields: ["browser"]
  });

  it("main field should work", () => {
    const result = resolver.sync(fixture, "@scope/pack1");
    assert.strictEqual(
      result.path,
      path.resolve(fixture, "./node_modules/@scope/pack1/main.js")
    );
  });

  it("browser field should work", () => {
    const result = resolver.sync(fixture, "@scope/pack2");
    assert.strictEqual(
      result.path,
      path.resolve(fixture, "./node_modules/@scope/pack2/main.js")
    );
  });

  it("folder request should work", () => {
    const result = resolver.sync(fixture, "@scope/pack2/lib");
    assert.strictEqual(
      result.path,
      path.resolve(fixture, "./node_modules/@scope/pack2/lib/index.js")
    );
  });
});
