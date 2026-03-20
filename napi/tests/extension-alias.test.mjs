import { describe, it } from "node:test";
import { ResolverFactory } from "../index.js";
import * as assert from "node:assert";
import * as path from "node:path";
import { fileURLToPath } from "url";

const fixtureDir = fileURLToPath(
  new URL("../../fixtures/enhanced_resolve/test/fixtures", import.meta.url)
);
const fixture = path.resolve(fixtureDir, "extension-alias");

describe("extension-alias", () => {
  const resolver = new ResolverFactory({
    extensions: [".js"],
    mainFiles: ["index.js"],
    extensionAlias: {
      ".js": [".ts", ".js"],
      ".mjs": [".mts"]
    }
  });

  it("should alias fully specified file", () => {
    const result = resolver.sync(fixture, "./index.js");
    assert.strictEqual(result.path, path.resolve(fixture, "index.ts"));
  });

  it("should alias fully specified file when there are two alternatives", () => {
    const result = resolver.sync(fixture, "./dir/index.js");
    assert.strictEqual(result.path, path.resolve(fixture, "dir", "index.ts"));
  });

  it("should also allow the second alternative", () => {
    const result = resolver.sync(fixture, "./dir2/index.js");
    assert.strictEqual(result.path, path.resolve(fixture, "dir2", "index.js"));
  });

  it("should support alias option without an array", () => {
    const result = resolver.sync(fixture, "./dir2/index.mjs");
    assert.strictEqual(result.path, path.resolve(fixture, "dir2", "index.mts"));
  });

  it("should not allow to fallback to the original extension or add extensions", () => {
    const result = resolver.sync(fixture, "./index.mjs");
    assert.ok(result.error);
  });

  describe("should not apply extension alias to extensions or mainFiles field", () => {
    const resolver2 = new ResolverFactory({
      extensions: [".js"],
      mainFiles: ["index.js"],
      extensionAlias: {
        ".js": []
      }
    });

    it("directory", () => {
      const result = resolver2.sync(fixture, "./dir2");
      assert.strictEqual(
        result.path,
        path.resolve(fixture, "dir2", "index.js")
      );
    });

    it("file", () => {
      const result = resolver2.sync(fixture, "./dir2/index");
      assert.strictEqual(
        result.path,
        path.resolve(fixture, "dir2", "index.js")
      );
    });
  });
});
