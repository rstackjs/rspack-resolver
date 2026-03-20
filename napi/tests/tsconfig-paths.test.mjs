import { describe, it } from "node:test";
import { ResolverFactory } from "../index.js";
import * as assert from "node:assert";
import * as path from "node:path";
import { fileURLToPath } from "url";

const fixtureDir = fileURLToPath(
  new URL("../../fixtures/enhanced_resolve/test/fixtures", import.meta.url)
);

const baseExampleDir = path.resolve(fixtureDir, "tsconfig-paths", "base");
const extendsExampleDir = path.resolve(
  fixtureDir,
  "tsconfig-paths",
  "extends-base"
);
const extendsNpmDir = path.resolve(fixtureDir, "tsconfig-paths", "extends-npm");
const extendsCircularDir = path.resolve(
  fixtureDir,
  "tsconfig-paths",
  "extends-circular"
);
const referencesProjectDir = path.resolve(
  fixtureDir,
  "tsconfig-paths",
  "references-project"
);

function makeTsconfigResolver(tsconfigPath, extra) {
  return new ResolverFactory({
    extensions: [".ts", ".tsx"],
    mainFields: ["browser", "main"],
    mainFiles: ["index"],
    tsconfig: { configFile: tsconfigPath },
    ...extra
  });
}

describe("TsconfigPathsPlugin", () => {
  it("resolves exact mapped path '@components/*' via tsconfig option", () => {
    const resolver = makeTsconfigResolver(
      path.join(baseExampleDir, "tsconfig.json")
    );
    const result = resolver.sync(baseExampleDir, "@components/button");
    assert.strictEqual(
      result.path,
      path.join(baseExampleDir, "src", "components", "button.ts")
    );
  });

  it("when multiple patterns match, the pattern with the longest matching prefix is used", () => {
    const resolver = makeTsconfigResolver(
      path.join(baseExampleDir, "tsconfig.json")
    );
    const result = resolver.sync(baseExampleDir, "longest/bar");
    assert.strictEqual(
      result.path,
      path.join(baseExampleDir, "src", "mapped", "longest", "three.ts")
    );
  });

  it("resolves exact mapped path 'foo' via tsconfig option", () => {
    const resolver = makeTsconfigResolver(
      path.join(baseExampleDir, "tsconfig.json")
    );
    const result = resolver.sync(baseExampleDir, "foo");
    assert.strictEqual(
      result.path,
      path.join(baseExampleDir, "src", "mapped", "foo", "index.ts")
    );
  });

  it("resolves wildcard mapped path 'bar/*' via tsconfig option", () => {
    const resolver = makeTsconfigResolver(
      path.join(baseExampleDir, "tsconfig.json")
    );
    const result = resolver.sync(baseExampleDir, "bar/file1");
    assert.strictEqual(
      result.path,
      path.join(baseExampleDir, "src", "mapped", "bar", "file1.ts")
    );
  });

  it("resolves wildcard mapped path '*/old-file' to specific file", () => {
    const resolver = makeTsconfigResolver(
      path.join(baseExampleDir, "tsconfig.json")
    );
    const result = resolver.sync(baseExampleDir, "utils/old-file");
    assert.strictEqual(
      result.path,
      path.join(baseExampleDir, "src", "components", "new-file.ts")
    );
  });

  it("falls through when no mapping exists", () => {
    const resolver = makeTsconfigResolver(
      path.join(baseExampleDir, "tsconfig.json")
    );
    const result = resolver.sync(baseExampleDir, "does-not-exist");
    assert.ok(result.error);
  });

  // extends-base uses ${configDir} in extends field
  it(
    "resolves '@components/*' using extends",
    { todo: "${configDir} in tsconfig extends" },
    () => {
      const resolver = makeTsconfigResolver(
        path.join(extendsExampleDir, "tsconfig.json")
      );
      const result = resolver.sync(extendsExampleDir, "@components/button");
      assert.strictEqual(
        result.path,
        path.join(extendsExampleDir, "src", "components", "button.ts")
      );
    }
  );

  describe("Path wildcard patterns", () => {
    it("resolves 'foo/*' wildcard pattern", () => {
      const resolver = makeTsconfigResolver(
        path.join(baseExampleDir, "tsconfig.json")
      );
      const result = resolver.sync(baseExampleDir, "foo/file1");
      assert.strictEqual(
        result.path,
        path.join(baseExampleDir, "src", "mapped", "bar", "file1.ts")
      );
    });

    it("resolves '*' catch-all pattern to src/mapped/star/*", () => {
      const resolver = makeTsconfigResolver(
        path.join(baseExampleDir, "tsconfig.json")
      );
      const result = resolver.sync(baseExampleDir, "star-bar/index");
      assert.strictEqual(
        result.path,
        path.join(
          baseExampleDir,
          "src",
          "mapped",
          "star",
          "star-bar",
          "index.ts"
        )
      );
    });

    it("resolves package with mainFields", () => {
      const resolver = makeTsconfigResolver(
        path.join(baseExampleDir, "tsconfig.json")
      );
      const result = resolver.sync(baseExampleDir, "main-field-package");
      assert.strictEqual(
        result.path,
        path.join(
          baseExampleDir,
          "src",
          "mapped",
          "star",
          "main-field-package",
          "node.ts"
        )
      );
    });

    it("resolves package with browser field", () => {
      const resolver = makeTsconfigResolver(
        path.join(baseExampleDir, "tsconfig.json")
      );
      const result = resolver.sync(baseExampleDir, "browser-field-package");
      assert.strictEqual(
        result.path,
        path.join(
          baseExampleDir,
          "src",
          "mapped",
          "star",
          "browser-field-package",
          "browser.ts"
        )
      );
    });

    it("resolves package with default index.ts", () => {
      const resolver = makeTsconfigResolver(
        path.join(baseExampleDir, "tsconfig.json")
      );
      const result = resolver.sync(baseExampleDir, "no-main-field-package");
      assert.strictEqual(
        result.path,
        path.join(
          baseExampleDir,
          "src",
          "mapped",
          "star",
          "no-main-field-package",
          "index.ts"
        )
      );
    });
  });

  it("should resolve paths when extending from npm package", () => {
    const resolver = makeTsconfigResolver(
      path.join(extendsNpmDir, "tsconfig.json")
    );
    const result = resolver.sync(extendsNpmDir, "@components/button");
    assert.match(result.path, /src[\\/](utils|components)[\\/]button\.ts$/);
  });

  it("should handle malformed tsconfig.json gracefully", () => {
    const malformedExampleDir = path.resolve(
      fixtureDir,
      "tsconfig-paths",
      "malformed-json"
    );
    const resolver = makeTsconfigResolver(
      path.join(malformedExampleDir, "tsconfig.json")
    );
    const result = resolver.sync(malformedExampleDir, "@components/button");
    assert.ok(result.error);
  });

  describe("${configDir} template variable support", () => {
    it("should substitute ${configDir} in path mappings", () => {
      const resolver = makeTsconfigResolver(
        path.join(baseExampleDir, "tsconfig.json")
      );
      const result = resolver.sync(baseExampleDir, "@components/button");
      assert.strictEqual(
        result.path,
        path.join(baseExampleDir, "src", "components", "button.ts")
      );
    });

    it("should substitute ${configDir} in multiple path patterns", () => {
      const resolver = makeTsconfigResolver(
        path.join(baseExampleDir, "tsconfig.json")
      );
      const result1 = resolver.sync(baseExampleDir, "@utils/date");
      assert.strictEqual(
        result1.path,
        path.join(baseExampleDir, "src", "utils", "date.ts")
      );
      const result2 = resolver.sync(baseExampleDir, "foo");
      assert.strictEqual(
        result2.path,
        path.join(baseExampleDir, "src", "mapped", "foo", "index.ts")
      );
    });

    it(
      "should handle circular extends without hanging",
      { todo: "${configDir} in tsconfig extends" },
      () => {
        const aDir = path.join(extendsCircularDir, "a");
        const resolver = makeTsconfigResolver(path.join(aDir, "tsconfig.json"));
        const result = resolver.sync(aDir, "@lib/foo");
        assert.strictEqual(
          result.path,
          path.join(aDir, "src", "lib", "foo.ts")
        );
      }
    );
  });

  it("should use baseUrl from tsconfig", () => {
    const resolver = new ResolverFactory({
      extensions: [".ts", ".tsx"],
      mainFields: ["browser", "main"],
      mainFiles: ["index"],
      tsconfig: {
        configFile: path.join(baseExampleDir, "tsconfig.json")
      }
    });
    const result = resolver.sync(baseExampleDir, "src/utils/date");
    assert.strictEqual(
      result.path,
      path.join(baseExampleDir, "src", "utils", "date.ts")
    );
  });

  describe("TypeScript Project References", () => {
    it("should support tsconfig object format with configFile", () => {
      const resolver = new ResolverFactory({
        extensions: [".ts", ".tsx"],
        mainFields: ["browser", "main"],
        mainFiles: ["index"],
        tsconfig: {
          configFile: path.join(baseExampleDir, "tsconfig.json"),
          references: "auto"
        }
      });
      const result = resolver.sync(baseExampleDir, "@components/button");
      assert.strictEqual(
        result.path,
        path.join(baseExampleDir, "src", "components", "button.ts")
      );
    });

    // references-project uses ${configDir} in tsconfig paths and references
    it(
      "should resolve own paths (without cross-project references)",
      { todo: "${configDir} in tsconfig references" },
      () => {
        const appDir = path.join(referencesProjectDir, "packages", "app");
        const resolver = new ResolverFactory({
          extensions: [".ts", ".tsx"],
          mainFields: ["browser", "main"],
          mainFiles: ["index"],
          tsconfig: {
            configFile: path.join(appDir, "tsconfig.json"),
            references: "auto"
          }
        });
        const result = resolver.sync(appDir, "@app/index");
        assert.strictEqual(result.path, path.join(appDir, "src", "index.ts"));
      }
    );

    it(
      "should resolve self-references within a referenced project",
      { todo: "${configDir} in tsconfig references" },
      () => {
        const appDir = path.join(referencesProjectDir, "packages", "app");
        const sharedDir = path.join(referencesProjectDir, "packages", "shared");
        const resolver = new ResolverFactory({
          extensions: [".ts", ".tsx"],
          mainFields: ["browser", "main"],
          mainFiles: ["index"],
          tsconfig: {
            configFile: path.join(appDir, "tsconfig.json"),
            references: "auto"
          }
        });
        const result = resolver.sync(sharedDir, "@shared/helper");
        assert.strictEqual(
          result.path,
          path.join(sharedDir, "src", "utils", "helper.ts")
        );
      }
    );

    it(
      "should support explicit references array",
      { todo: "${configDir} in tsconfig references" },
      () => {
        const appDir = path.join(referencesProjectDir, "packages", "app");
        const sharedSrcDir = path.join(
          referencesProjectDir,
          "packages",
          "shared",
          "src"
        );
        const resolver = new ResolverFactory({
          extensions: [".ts", ".tsx"],
          mainFields: ["browser", "main"],
          mainFiles: ["index"],
          tsconfig: {
            configFile: path.join(appDir, "tsconfig.json"),
            references: ["../shared"]
          }
        });
        const result = resolver.sync(sharedSrcDir, "@shared/helper");
        assert.strictEqual(
          result.path,
          path.join(sharedSrcDir, "utils", "helper.ts")
        );
      }
    );

    it(
      "should not load references when references option is omitted",
      { todo: "${configDir} in tsconfig references" },
      () => {
        const appDir = path.join(referencesProjectDir, "packages", "app");
        const resolver = new ResolverFactory({
          extensions: [".ts", ".tsx"],
          mainFields: ["browser", "main"],
          mainFiles: ["index"],
          tsconfig: {
            configFile: path.join(appDir, "tsconfig.json")
          }
        });
        const result = resolver.sync(appDir, "@shared/utils/helper");
        assert.ok(result.error);
      }
    );

    it(
      "should handle nested references",
      { todo: "${configDir} in tsconfig references" },
      () => {
        const appDir = path.join(referencesProjectDir, "packages", "app");
        const utilsSrcDir = path.join(
          referencesProjectDir,
          "packages",
          "utils",
          "src"
        );
        const resolver = new ResolverFactory({
          extensions: [".ts", ".tsx"],
          mainFields: ["browser", "main"],
          mainFiles: ["index"],
          tsconfig: {
            configFile: path.join(appDir, "tsconfig.json"),
            references: "auto"
          }
        });
        const result = resolver.sync(utilsSrcDir, "@utils/date");
        assert.strictEqual(
          result.path,
          path.join(utilsSrcDir, "core", "date.ts")
        );
      }
    );

    describe("modules resolution with references", () => {
      it(
        "should resolve modules from main project's baseUrl",
        { todo: "${configDir} in tsconfig references" },
        () => {
          const appDir = path.join(referencesProjectDir, "packages", "app");
          const resolver = new ResolverFactory({
            extensions: [".ts", ".tsx"],
            mainFields: ["browser", "main"],
            mainFiles: ["index"],
            tsconfig: {
              configFile: path.join(appDir, "tsconfig.json"),
              references: "auto"
            }
          });
          const result = resolver.sync(appDir, "src/components/Button");
          assert.strictEqual(
            result.path,
            path.join(appDir, "src", "components", "Button.ts")
          );
        }
      );

      it(
        "should resolve modules from referenced project's baseUrl",
        { todo: "${configDir} in tsconfig references" },
        () => {
          const appDir = path.join(referencesProjectDir, "packages", "app");
          const sharedSrcDir = path.join(
            referencesProjectDir,
            "packages",
            "shared",
            "src"
          );
          const resolver = new ResolverFactory({
            extensions: [".ts", ".tsx"],
            mainFields: ["browser", "main"],
            mainFiles: ["index"],
            tsconfig: {
              configFile: path.join(appDir, "tsconfig.json"),
              references: "auto"
            }
          });
          const result = resolver.sync(sharedSrcDir, "utils/helper");
          assert.strictEqual(
            result.path,
            path.join(sharedSrcDir, "utils", "helper.ts")
          );
        }
      );

      it(
        "should resolve components from referenced project's baseUrl",
        { todo: "${configDir} in tsconfig references" },
        () => {
          const appDir = path.join(referencesProjectDir, "packages", "app");
          const sharedSrcDir = path.join(
            referencesProjectDir,
            "packages",
            "shared",
            "src"
          );
          const resolver = new ResolverFactory({
            extensions: [".ts", ".tsx"],
            mainFields: ["browser", "main"],
            mainFiles: ["index"],
            tsconfig: {
              configFile: path.join(appDir, "tsconfig.json"),
              references: "auto"
            }
          });
          const result = resolver.sync(sharedSrcDir, "components/Input");
          assert.strictEqual(
            result.path,
            path.join(sharedSrcDir, "components", "Input.ts")
          );
        }
      );

      it(
        "should use correct baseUrl based on request context",
        { todo: "${configDir} in tsconfig references" },
        () => {
          const appDir = path.join(referencesProjectDir, "packages", "app");
          const sharedDir = path.join(
            referencesProjectDir,
            "packages",
            "shared"
          );
          const resolver = new ResolverFactory({
            extensions: [".ts", ".tsx"],
            mainFields: ["browser", "main"],
            mainFiles: ["index"],
            tsconfig: {
              configFile: path.join(appDir, "tsconfig.json"),
              references: "auto"
            }
          });
          const result1 = resolver.sync(appDir, "src/index");
          assert.strictEqual(
            result1.path,
            path.join(appDir, "src", "index.ts")
          );
        }
      );

      it(
        "should support explicit references with modules resolution",
        { todo: "${configDir} in tsconfig references" },
        () => {
          const appDir = path.join(referencesProjectDir, "packages", "app");
          const sharedSrcDir = path.join(
            referencesProjectDir,
            "packages",
            "shared",
            "src"
          );
          const resolver = new ResolverFactory({
            extensions: [".ts", ".tsx"],
            mainFields: ["browser", "main"],
            mainFiles: ["index"],
            tsconfig: {
              configFile: path.join(appDir, "tsconfig.json"),
              references: ["../shared"]
            }
          });
          const result = resolver.sync(sharedSrcDir, "utils/helper");
          assert.strictEqual(
            result.path,
            path.join(sharedSrcDir, "utils", "helper.ts")
          );
        }
      );
    });
  });

  describe("bug: baseUrl from deep extends chain", () => {
    const deepBaseUrlDir = path.resolve(
      fixtureDir,
      "tsconfig-paths",
      "extends-deep-baseurl"
    );

    it("should resolve paths whose baseUrl comes from a grandparent extends", () => {
      const appDir = path.join(deepBaseUrlDir, "packages", "app");
      const resolver = makeTsconfigResolver(path.join(appDir, "tsconfig.json"));
      const result = resolver.sync(appDir, "@base/utils/format");
      assert.strictEqual(
        result.path,
        path.join(deepBaseUrlDir, "tsconfig-base", "src", "utils", "format.ts")
      );
    });
  });

  describe("bug: scoped npm package in extends field", () => {
    const pkgEntryDir = path.resolve(
      fixtureDir,
      "tsconfig-paths",
      "extends-pkg-entry"
    );

    it("should resolve paths inherited from a scoped npm package tsconfig", () => {
      const resolver = makeTsconfigResolver(
        path.join(pkgEntryDir, "tsconfig.json")
      );
      const result = resolver.sync(pkgEntryDir, "@pkg/util");
      assert.strictEqual(
        result.path,
        path.join(
          pkgEntryDir,
          "node_modules",
          "@my-tsconfig",
          "base",
          "src",
          "util.ts"
        )
      );
    });
  });

  describe("JSONC support (comments in tsconfig.json)", () => {
    const jsoncExampleDir = path.resolve(
      fixtureDir,
      "tsconfig-paths",
      "jsonc-comments"
    );

    it("should parse tsconfig.json with line comments", () => {
      const resolver = makeTsconfigResolver(
        path.join(jsoncExampleDir, "tsconfig.json")
      );
      const result = resolver.sync(jsoncExampleDir, "@components/button");
      assert.strictEqual(
        result.path,
        path.join(jsoncExampleDir, "src", "components", "button.ts")
      );
    });

    it("should parse tsconfig.json with block comments", () => {
      const resolver = makeTsconfigResolver(
        path.join(jsoncExampleDir, "tsconfig.json")
      );
      const result = resolver.sync(jsoncExampleDir, "bar/index");
      assert.strictEqual(
        result.path,
        path.join(jsoncExampleDir, "src", "mapped", "bar", "index.ts")
      );
    });

    it("should parse tsconfig.json with mixed comments", () => {
      const resolver = makeTsconfigResolver(
        path.join(jsoncExampleDir, "tsconfig.json")
      );
      const result = resolver.sync(jsoncExampleDir, "foo");
      assert.strictEqual(
        result.path,
        path.join(jsoncExampleDir, "src", "mapped", "foo", "index.ts")
      );
    });
  });
});
