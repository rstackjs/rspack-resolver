# 检查 ChromeLayerBuilder 的可用方法

# 在 Rust 中，我们可以通过 IDE 或文档来查看
# 但让我们尝试运行 cargo doc

import subprocess
import os

print("Checking tracing-chrome API...")

# 尝试获取文档
result = subprocess.run(
    ["cargo", "doc", "--package", "tracing-chrome", "--no-deps", "--open"],
    capture_output=True,
    text=True
)

if result.returncode == 0:
    print("Documentation opened in browser")
    print("\nPlease check ChromeLayerBuilder for methods related to:")
    print("  - span_events()")
    print("  - id()")
    print("  - parent_id()")
else:
    print(f"Failed to open docs: {result.stderr}")

# 检查是否有环境变量可以配置
print("\nChecking tracing-chrome source code for config options...")
print("Common environment variables for tracing-chrome:")
print("  - TRACING_CHROME_SPAN_EVENTS")
print("  - TRACING_CHROME_ID")
