import { defineConfig } from "@rstest/core";
import path from "node:path";

export default defineConfig({
  testEnvironment: "node",
  include: ["napi/tests/**/*.test.mjs"],
  output: {
    externals: [
      ({ request }, callback) => {
        // Externalize the napi binding so native .node files load at runtime
        if (request === "../index.js") {
          callback(null, `node-commonjs ${path.resolve("napi/index.js")}`);
          return;
        }
        callback();
      }
    ]
  }
});
