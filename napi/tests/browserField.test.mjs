import { describe, it } from "node:test";
import { ResolverFactory } from "../index.js";
import * as assert from "node:assert";
import * as path from "node:path";
import { fileURLToPath } from "url";

const fixtureDir = fileURLToPath(
  new URL("../../fixtures/enhanced_resolve/test/fixtures", import.meta.url)
);
const browserModule = path.join(fixtureDir, "browser-module");

function p(...args) {
  return path.join(browserModule, ...args);
}

describe("browserField", () => {
  const resolver = new ResolverFactory({
    aliasFields: [
      "browser",
      ["innerBrowser1", "field2", "browser"],
      ["innerBrowser1", "field", "browser"],
      ["innerBrowser2", "browser"]
    ]
  });

  it("should ignore", () => {
    const result = resolver.sync(p(), "./lib/ignore");
    assert.strictEqual(result.path, undefined);
  });

  it("should ignore #2", () => {
    assert.strictEqual(resolver.sync(p(), "./lib/ignore.js").path, undefined);
    assert.strictEqual(resolver.sync(p("lib"), "./ignore").path, undefined);
    assert.strictEqual(resolver.sync(p("lib"), "./ignore.js").path, undefined);
  });

  it("should replace a file", () => {
    assert.strictEqual(
      resolver.sync(p(), "./lib/replaced").path,
      p("lib", "browser.js")
    );
    assert.strictEqual(
      resolver.sync(p(), "./lib/replaced.js").path,
      p("lib", "browser.js")
    );
    assert.strictEqual(
      resolver.sync(p("lib"), "./replaced").path,
      p("lib", "browser.js")
    );
    assert.strictEqual(
      resolver.sync(p("lib"), "./replaced.js").path,
      p("lib", "browser.js")
    );
  });

  it("should replace a module with a file", () => {
    assert.strictEqual(
      resolver.sync(p(), "module-a").path,
      p("browser", "module-a.js")
    );
    assert.strictEqual(
      resolver.sync(p("lib"), "module-a").path,
      p("browser", "module-a.js")
    );
  });

  it("should replace a module with a module", () => {
    assert.strictEqual(
      resolver.sync(p(), "module-b").path,
      p("node_modules", "module-c.js")
    );
    assert.strictEqual(
      resolver.sync(p("lib"), "module-b").path,
      p("node_modules", "module-c.js")
    );
  });

  it("should resolve in nested property", () => {
    assert.strictEqual(
      resolver.sync(p(), "./lib/main1.js").path,
      p("lib", "main.js")
    );
    assert.strictEqual(
      resolver.sync(p(), "./lib/main2.js").path,
      p("lib", "browser.js")
    );
  });

  it("should check only alias field properties", () => {
    assert.strictEqual(
      resolver.sync(p(), "./toString").path,
      p("lib", "toString.js")
    );
  });
});
