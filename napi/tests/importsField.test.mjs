import { describe, it } from "node:test";
import { ResolverFactory } from "../index.js";
import * as assert from "node:assert";
import * as path from "node:path";
import { fileURLToPath } from "url";

const fixtureDir = fileURLToPath(
  new URL("../../fixtures/enhanced_resolve/test/fixtures", import.meta.url)
);

const fixture = path.resolve(fixtureDir, "imports-field");
const fixture1 = path.resolve(fixtureDir, "imports-field-different");

describe("importsFieldPlugin", () => {
  const resolver = new ResolverFactory({
    extensions: [".js"],
    mainFiles: ["index.js"],
    conditionNames: ["webpack"]
  });

  it("should resolve using imports field instead of self-referencing", () => {
    const result = resolver.sync(fixture, "#imports-field");
    assert.strictEqual(result.path, path.resolve(fixture, "b.js"));
  });

  it("should resolve using imports field instead of self-referencing for a subpath", () => {
    const result = resolver.sync(
      path.resolve(fixture, "dir"),
      "#imports-field"
    );
    assert.strictEqual(result.path, path.resolve(fixture, "b.js"));
  });

  it("should disallow resolve out of package scope", () => {
    const result = resolver.sync(fixture, "#b");
    assert.ok(result.error);
  });

  it("field name #1", () => {
    const r = new ResolverFactory({
      extensions: [".js"],
      mainFiles: ["index.js"],
      importsFields: [["imports"]],
      conditionNames: ["webpack"]
    });
    const result = r.sync(fixture, "#imports-field");
    assert.strictEqual(result.path, path.resolve(fixture, "b.js"));
  });

  it("field name #2", () => {
    const r = new ResolverFactory({
      extensions: [".js"],
      mainFiles: ["index.js"],
      importsFields: [["other", "imports"], "imports"],
      conditionNames: ["webpack"]
    });
    const result = r.sync(fixture, "#b");
    assert.strictEqual(result.path, path.resolve(fixture, "a.js"));
  });

  it("should resolve package #1", () => {
    const result = resolver.sync(fixture, "#a/dist/main.js");
    assert.strictEqual(
      result.path,
      path.resolve(fixture, "node_modules/a/lib/lib2/main.js")
    );
  });

  it("should resolve package #2", () => {
    const result = resolver.sync(fixture, "#a");
    assert.ok(result.error);
  });

  it("should resolve package #3", () => {
    const result = resolver.sync(fixture, "#ccc/index.js");
    assert.strictEqual(
      result.path,
      path.resolve(fixture, "node_modules/c/index.js")
    );
  });

  it("should resolve package #4", () => {
    const result = resolver.sync(fixture, "#c");
    assert.strictEqual(
      result.path,
      path.resolve(fixture, "node_modules/c/index.js")
    );
  });

  it("should resolve with wildcard pattern", () => {
    const wcFixture = path.resolve(
      fixtureDir,
      "imports-exports-wildcard/node_modules/m"
    );
    const result = resolver.sync(wcFixture, "#internal/i.js");
    assert.strictEqual(
      result.path,
      path.resolve(wcFixture, "./src/internal/i.js")
    );
  });

  it(
    "should work and throw an error on invalid imports #1",
    { todo: "#/ slash pattern (node.js PR #60864) not yet supported" },
    () => {
      const result = resolver.sync(fixture, "#/dep");
      assert.ok(result.error);
    }
  );

  it("should work and throw an error on invalid imports #2", () => {
    const result = resolver.sync(fixture, "#dep/");
    assert.ok(result.error);
  });

  it(
    "should work with invalid imports #1",
    { todo: "query strings containing ../ treated as invalid targets" },
    () => {
      const result = resolver.sync(fixture1, "#dep");
      assert.strictEqual(
        result.path,
        `${path.resolve(fixture1, "./a.js")}?foo=../`
      );
    }
  );

  it(
    "should work with invalid imports #2",
    { todo: "query strings containing ../ treated as invalid targets" },
    () => {
      const result = resolver.sync(fixture1, "#dep/foo/a.js");
      assert.strictEqual(
        result.path,
        `${path.resolve(fixture1, "./a.js")}?foo=../#../`
      );
    }
  );

  it("should work with invalid imports #3", () => {
    const result = resolver.sync(fixture1, "#dep/bar");
    assert.ok(result.error);
  });

  it("should work with invalid imports #4", () => {
    const result = resolver.sync(fixture1, "#dep/baz");
    assert.ok(result.error);
  });

  it("should work with invalid imports #5", () => {
    const result = resolver.sync(fixture1, "#dep/baz-multi");
    assert.ok(result.error);
  });

  it(
    "should work with invalid imports #7",
    { todo: "invalid specifier array handling differences" },
    () => {
      const result = resolver.sync(fixture1, "#dep/pattern/a.js");
      assert.ok(result.error);
    }
  );

  it(
    "should work with invalid imports #8",
    { todo: "invalid specifier array handling differences" },
    () => {
      const result = resolver.sync(fixture1, "#dep/array");
      assert.strictEqual(result.path, path.resolve(fixture1, "./a.js"));
    }
  );

  it(
    "should work with invalid imports #9",
    { todo: "invalid specifier array handling differences" },
    () => {
      const result = resolver.sync(fixture1, "#dep/array2");
      assert.ok(result.error);
    }
  );

  it("should work with invalid imports #10", () => {
    const result = resolver.sync(fixture1, "#dep/array3");
    assert.strictEqual(result.path, path.resolve(fixture1, "./a.js"));
  });

  it("should work with invalid imports #11", () => {
    const result = resolver.sync(fixture1, "#dep/empty");
    assert.ok(result.error);
  });

  it("should work with invalid imports #12", () => {
    const result = resolver.sync(fixture1, "#dep/with-bad");
    assert.strictEqual(result.path, path.resolve(fixture1, "./a.js"));
  });

  it("should work with invalid imports #13", () => {
    const result = resolver.sync(fixture1, "#dep/with-bad2");
    assert.strictEqual(result.path, path.resolve(fixture1, "./a.js"));
  });

  it("should work with invalid imports #14", () => {
    const result = resolver.sync(fixture1, "#timezones/pdt.mjs");
    assert.ok(result.error);
  });

  it("should work with invalid imports #15", () => {
    const result = resolver.sync(fixture1, "#dep/multi1");
    assert.ok(result.error);
  });

  it("should work with invalid imports #16", () => {
    const result = resolver.sync(fixture1, "#dep/multi2");
    assert.ok(result.error);
  });

  it("should work and resolve with array imports", () => {
    const result = resolver.sync(fixture1, "#dep/multi");
    assert.strictEqual(result.path, path.resolve(fixture1, "./a.js"));
  });
});
