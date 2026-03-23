import { defineConfig } from "@rstest/core";
import path from "node:path";

export default defineConfig({
  testEnvironment: "node",
  include: ["napi/__test__/**/*.test.mjs"],
  output: {
    externals: [
      ({ request }, callback) => {
        if (request === "../index.js" || request === "../resolver.wasi.cjs") {
          callback(
            null,
            `node-commonjs ${path.resolve("napi", request.slice(3))}`
          );
          return;
        }
        callback();
      }
    ]
  }
});
