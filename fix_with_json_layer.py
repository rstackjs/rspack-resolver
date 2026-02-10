# 使用 tracing-subscriber 的 JSON layer 替代 tracing-chrome
# 这个 layer 会正确生成 span ID 和父子关系

import json

# 验证当前 trace
print("分析当前 trace...")
with open("trace.json", "r") as f:
    trace_data = json.load(f)

# 统计
b_events = [e for e in trace_data if e.get("ph") == "b" or e.get("ph") == "B"]
e_events = [e for e in trace_data if e.get("ph") == "e" or e.get("ph") == "E"]

print(f"  Begin events: {len(b_events)}")
print(f"  End events: {len(ne_events)}")

# 检查 ID 字段
with_id = [e for e in trace_data if 'id' in e and e['id']]
print(f"  Events with 'id': {len(with_id)}")

# 检查 parentId 字段  
with_parent = [e for e in trace_data if 'parentId' in e]
print(f"  Events with 'parentId': {len(with_parent)}")

# 分析 ID 分布
if with_id:
    ids = [e['id'] for e in with_id]
    unique_ids = list(sorted(set(ids)))
    print(f"\n  Unique IDs: {unique_ids}")
    if len(unique_ids) == 1:
        print("  ❌ 所有 span 使用同一个 ID！")
    else:
        print(f"  ✅ 有 {len(unique_ids)} 个不同的 ID")
