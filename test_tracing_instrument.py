import subprocess
import os

# 测试 tracing::instrument 是否会生成 id

test_code = '''
use tracing::{info, instrument};
use tracing_chrome::{ChromeLayerBuilder, FlushGuard};
use tracing_subscriber::{Registry};

#[instrument]
async fn level1(x: i32) -> i32 {
    level2(x + 1).await
}

#[instrument]
async fn level2(x: i32) -> i32 {
    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
    x * 2
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (chrome_layer, _guard) = ChromeLayerBuilder::new()
        .file("test_instrument.json")
        .include_args(true)
        .build();
    
    Registry::default().with(chrome_layer).init();
    
    level1(42).await;
    
    Ok(())
}
'''

# 创建测试项目
os.makedirs("test_tracing", exist_ok=True)
os.chdir("test_tracing")

with open("src/lib.rs", "w") as f:
    f.write(test_code)

with open("Cargo.toml", "w") as f:
    f.write('''
[package]
name = "test_tracing"
version = "0.1.0"
edition = "2021"

[[bin]]
name = "test_tracing"
path = "src/lib.rs"

[dependencies]
tokio = { version = "1", features = ["full"] }
tracing = "0.1"
tracing-chrome = "0.7"
tracing-subscriber = "0.3"
''')

print("Running test...")
result = subprocess.run(
    ["cargo", "run", "--release"],
    capture_output=True,
    text=True
)

print(f"\nReturn code: {result.returncode}")

if os.path.exists("test_instrument.json"):
    import json
    with open("test_instrument.json", "r") as f:
        trace = json.load(f)
    
    print(f"\nTotal events: {len(trace)}")
    
    # 检查 id 字段
    events_with_id = [e for e in trace if 'id' in e]
    print(f"Events with 'id': {len(events_with_id)}")
    
    if events_with_id:
        print("\nSample events with id:")
        for e in events_with_id[:4]:
            print(json.dumps(e, indent=2))
else:
    print("\nError: test_instrument.json not generated")
    print(f"\nstdout:\n{result.stdout}")
    print(f"\nstderr:\n{result.stderr}")
