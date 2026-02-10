import json
import subprocess
import tempfile
import os

# 测试 tracing 本身是否生成 id

test_code = '''
use tracing::{info, instrument, Level, Subscriber};
use tracing_subscriber::Registry;

// 自定义 subscriber 来捕获 trace 数据
struct CustomSubscriber {
    events: std::sync::Mutex<Vec<CustomEvent>>,
}

#[derive(Debug, Clone)]
struct CustomEvent {
    fields: Vec<(String, String)>,
}

impl<S> Subscriber for CustomSubscriber
where
    S: Subscriber + for<'a> tracing::Subscriber::lookup_span<'a>,
{
    fn new_span(&self, attrs: &tracing::span::Attributes) -> tracing::span::Id {
        let id = tracing::span::Id::from_u64(1);
        println!("New span: {:?}", attrs.values());
        println!("  ID: {:?}", id);
        id
    }
    
    fn record(&self, _: &tracing::span::Id, _: &tracing::span::Record) {}
    
    fn record_follows_from(&self, _: &tracing::span::Id, _: &tracing::span::Id) {
        println!("Span follows from another span");
    }
    
    fn event(&self, _: &tracing::Event) {}
    
    fn enter(&self, _: &tracing::span::Id) {}
    
    fn exit(&self, _: &tracing::span::Id) {}
}

#[instrument]
async fn inner(x: i32) -> i32 {
    x * 2
}

#[tokio::main]
async fn main() {
    let subscriber = CustomSubscriber {
        events: std::sync::Mutex::new(Vec::new()),
    };
    Registry::default().with(subscriber).init();
    
    inner(42).await;
}
'''

with open("src/main.rs", "w") as f:
    f.write(test_code)

print("Compiling test...")
result = subprocess.run(
    ["cargo", "run", "--quiet", "2>&1"],
    capture_output=True,
    text=True
)

print(f"\n{result.stdout}")
