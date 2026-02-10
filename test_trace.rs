
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
