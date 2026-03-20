import { describe, it } from "node:test";
import { ResolverFactory } from "../index.js";
import * as assert from "node:assert";
import * as path from "node:path";
import { fileURLToPath } from "url";

const fixtureDir = fileURLToPath(
  new URL("../../fixtures/enhanced_resolve/test/fixtures", import.meta.url)
);
const testDir = path.resolve(fixtureDir, "..");

describe("roots", () => {
  const resolver = new ResolverFactory({
    extensions: [".js"],
    alias: {
      foo: "/fixtures"
    },
    roots: [testDir, fixtureDir]
  });

  it("should respect roots option", () => {
    const result = resolver.sync(fixtureDir, "/fixtures/b.js");
    assert.strictEqual(result.path, path.resolve(fixtureDir, "b.js"));
  });

  it("should try another root option, if it exists", () => {
    const result = resolver.sync(fixtureDir, "/b.js");
    assert.strictEqual(result.path, path.resolve(fixtureDir, "b.js"));
  });

  it("should respect extension", () => {
    const result = resolver.sync(fixtureDir, "/fixtures/b");
    assert.strictEqual(result.path, path.resolve(fixtureDir, "b.js"));
  });

  it("should resolve in directory", () => {
    const result = resolver.sync(fixtureDir, "/fixtures/extensions/dir");
    assert.strictEqual(
      result.path,
      path.resolve(fixtureDir, "extensions/dir/index.js")
    );
  });

  it("should respect aliases", () => {
    const result = resolver.sync(fixtureDir, "foo/b");
    assert.strictEqual(result.path, path.resolve(fixtureDir, "b.js"));
  });

  it("should support roots options with resolveToContext", () => {
    const contextResolver = new ResolverFactory({
      roots: [testDir],
      resolveToContext: true
    });
    const result = contextResolver.sync(fixtureDir, "/fixtures/lib");
    assert.strictEqual(result.path, path.resolve(fixtureDir, "lib"));
  });

  it("should not work with relative path", () => {
    const result = resolver.sync(fixtureDir, "fixtures/b.js");
    assert.ok(result.error);
  });

  it("should resolve an absolute path (prefer absolute)", () => {
    const resolverPreferAbsolute = new ResolverFactory({
      extensions: [".js"],
      alias: {
        foo: "/fixtures"
      },
      roots: [testDir, fixtureDir],
      preferAbsolute: true
    });
    const result = resolverPreferAbsolute.sync(
      fixtureDir,
      path.join(fixtureDir, "b.js")
    );
    assert.strictEqual(result.path, path.resolve(fixtureDir, "b.js"));
  });
});
