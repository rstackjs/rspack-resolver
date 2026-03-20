import { describe, it } from "node:test";
import { ResolverFactory } from "../index.js";
import * as assert from "node:assert";
import * as path from "node:path";
import { fileURLToPath } from "url";

const fixtureDir = fileURLToPath(
  new URL("../../fixtures/enhanced_resolve/test/fixtures", import.meta.url)
);
const fixture = path.resolve(fixtureDir, "extensions");

describe("extensions", () => {
  const resolver = new ResolverFactory({
    extensions: [".ts", ".js"]
  });

  it("should resolve according to order of provided extensions", () => {
    const result = resolver.sync(fixture, "./foo");
    assert.strictEqual(result.path, path.resolve(fixture, "foo.ts"));
  });

  it("should resolve according to order of provided extensions (dir index)", () => {
    const result = resolver.sync(fixture, "./dir");
    assert.strictEqual(result.path, path.resolve(fixture, "dir/index.ts"));
  });

  it("should resolve according to main field in module root", () => {
    const result = resolver.sync(fixture, ".");
    assert.strictEqual(result.path, path.resolve(fixture, "index.js"));
  });

  it("should resolve single file module before directory", () => {
    const result = resolver.sync(fixture, "module");
    assert.strictEqual(
      result.path,
      path.resolve(fixture, "node_modules/module.js")
    );
  });

  it("should resolve trailing slash directory before single file", () => {
    const result = resolver.sync(fixture, "module/");
    assert.strictEqual(
      result.path,
      path.resolve(fixture, "node_modules/module/index.ts")
    );
  });

  it("should not resolve to file when request has a trailing slash (relative)", () => {
    const result = resolver.sync(fixture, "./foo.js/");
    assert.ok(result.error);
  });

  it("should not resolve to file when request has a trailing slash (module)", () => {
    const result = resolver.sync(fixture, "module.js/");
    assert.ok(result.error);
  });
});
