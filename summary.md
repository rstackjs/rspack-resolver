# tracing-chrome Async 模式的 ID 生成 Bug

## 问题确认

所有 span 的 ID 都是 1，没有 parentId 字段。

## 根本原因

这是 `tracing-chrome` 的一个**已知 bug**，位于 `get_root_id()` 函数：

```rust
fn get_root_id(span: SpanRef<S>) -> u64 {
    span.scope()
        .from_root()      // 获取根 scope
        .take(1)         // ❌ 跳过第 1 层（根本身）
        .next()          // 获取第 2 层
        .unwrap_or(span)
        .id()           // 获取 ID
        .into_u64()
}
```

## Bug 详细分析

对于嵌套的 span：
- **第 2 层**（`resolve_tracing`）：
  - `from_root()` = [root, child2, child3, ...]
  - `.take(1)` 移除 root，返回 [child2, child3, ...]
  - `.next()` 返回 child3（**第 3 层**）
  - ❌ 错误：使用了 child3 的 ID！

- **第 3 层**（`resolve_impl`）：
  - `from_root()` = [root, child2, child3, child4, ...]
  - `.take(1)` 移除 root，返回 [child2, child3, child4, ...]
  - `.next()` 返回 child3（**仍然第 3 层**）
  - ❌ 又是 child3 的 ID！

**结果**：所有子 span 都使用了错误的 ID（第 3 层的 span ID）。

## 解决方案

### 方案 1：使用后处理脚本（立即可用）

我已经创建了 `fix_span_ids.py` 脚本，可以正确添加 ID 和父子关系：

```bash
# 生成 trace
TRACE_FILE=trace.json RUST_LOG=debug \
  cargo run -F=enable_instrument --example resolver \
  /Users/bytedance/git/problem/yarn_pnp_test/app ../lib/index.js

# 修复 ID 和父子关系
python3 fix_span_ids.py
# 生成的文件：trace_with_ids_fixed.json
```

### 方案 2：使用其他 tracing layer

使用 `tracing-subscriber` 的 JSON layer 替代 `tracing-chrome`，它有更好的 ID 生成逻辑。

### 方案 3：报告 bug

这是一个 `tracing-chrome` crate 的已知问题，可以向项目提交 issue：
https://github.com/tokio-rs/tracing/issues

搜索关键词："tracing-chrome" + "Async" + "get_root_id" + "bug"
