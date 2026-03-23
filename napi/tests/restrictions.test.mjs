import { describe, it } from "node:test";
import { ResolverFactory } from "../index.js";
import * as assert from "node:assert";
import * as path from "node:path";
import { fileURLToPath } from "url";

const fixtureDir = fileURLToPath(
  new URL("../../fixtures/enhanced_resolve/test/fixtures", import.meta.url)
);
const fixture = path.resolve(fixtureDir, "restrictions");

describe("restrictions", () => {
  it("should respect RegExp restriction", () => {
    const resolver = new ResolverFactory({
      extensions: [".js"],
      restrictions: [{ regex: "\\.(sass|scss|css)$" }]
    });
    const result = resolver.sync(fixture, "pck1");
    assert.ok(result.error);
  });

  it("should try to find alternative #1", () => {
    const resolver = new ResolverFactory({
      extensions: [".js", ".css"],
      mainFiles: ["index"],
      restrictions: [{ regex: "\\.(sass|scss|css)$" }]
    });
    const result = resolver.sync(fixture, "pck1");
    assert.strictEqual(
      result.path,
      path.resolve(fixture, "node_modules/pck1/index.css")
    );
  });

  it("should respect string restriction", () => {
    const resolver = new ResolverFactory({
      extensions: [".js"],
      restrictions: [{ path: fixture }]
    });
    const result = resolver.sync(fixture, "pck2");
    assert.ok(result.error);
  });

  it(
    "should try to find alternative #2",
    { skip: "restrictions with multiple mainFields" },
    () => {
      const resolver = new ResolverFactory({
        extensions: [".js"],
        mainFields: ["main", "style"],
        restrictions: [{ path: fixture }, { regex: "\\.(sass|scss|css)$" }]
      });
      const result = resolver.sync(fixture, "pck2");
      assert.strictEqual(
        result.path,
        path.resolve(fixture, "node_modules/pck2/index.css")
      );
    }
  );
});
