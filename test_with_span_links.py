import json
import subprocess
import os

# 创建一个测试程序来验证 tracing-chrome 的配置
test_code = '''
use std::time::Duration;
use tracing::{info, instrument};
use tracing_chrome::{ChromeLayerBuilder, FlushGuard};
use tracing_subscriber::{registry, Registry};

#[instrument]
async fn inner_function(x: i32) -> i32 {
    tokio::time::sleep(Duration::from_millis(10)).await;
    inner_inner(x + 1).await
}

#[instrument]
async fn inner_inner(x: i32) -> i32 {
    tokio::time::sleep(Duration::from_millis(5)).await;
    x * 2
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (chrome_layer, _guard) = ChromeLayerBuilder::new()
        .file("test_trace.json")
        .include_args(true)
        .span_events(true)  // 尝试启用 span events
        .build();
    
    registry().with(chrome_layer).init();
    
    inner_function(42).await;
    
    Ok(())
}
'''

# 写入测试文件
with open("test_trace.rs", "w") as f:
    f.write(test_code)

print("Creating Cargo.toml for test...")
cargo_toml = '''
[package]
name = "test_trace"
version = "0.1.0"
edition = "2021"

[dependencies]
tokio = { version = "1", features = ["full"] }
tracing = "0.1"
tracing-chrome = "0.7"
tracing-subscriber = "0.3"
'''

with open("Cargo.toml", "w") as f:
    f.write(cargo_toml)

print("Running test...")
result = subprocess.run(
    ["cargo", "run", "--quiet"],
    capture_output=True,
    text=True
)

print(f"\nChecking output...")

if os.path.exists("test_trace.json"):
    with open("test_trace.json", "r") as f:
        trace = json.load(f)
    
    print(f"Total events: {len(trace)}")
    print(f"\nSpan types: {set(e.get('ph') for e in trace)}")
    
    # 检查是否有 id 字段
    events_with_id = [e for e in trace if 'id' in e]
    print(f"Events with 'id': {len(events_with_id)}")
    
    # 检查是否有 X 事件（cross-flow）
    events_with_x = [e for e in trace if e.get('ph') == 'X']
    print(f"Events with 'ph=X': {len(events_with_x)}")
    
    if events_with_id:
        print(f"\nSample event with id: {events_with_id[0]}")
    
    # 显示 span 结构
    begin_events = [e for e in trace if e.get('ph') == 'B']
    print(f"\nSample begin event keys: {sorted(begin_events[0].keys()) if begin_events else 'N/A'}")
else:
    print("Error: test_trace.json not generated")
    print(f"\nstdout: {result.stdout}")
    print(f"stderr: {result.stderr}")
