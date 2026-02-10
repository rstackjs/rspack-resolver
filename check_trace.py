import json

with open('trace.json', 'r') as f:
    trace = json.load(f)

print(f"Total events: {len(trace)}")

# 检查 B 事件
b_events = [e for e in trace if e.get('ph') == 'b']
print(f"\nBegin events: {len(b_events)}")

# 检查 ID 字段
b_with_id = [e for e in b_events if 'id' in e]
print(f"Events with 'id': {len(b_with_id)}")

# 检查 parentId 字段
b_with_parent = [e for e in b_events if 'parentId' in e]
print(f"Events with 'parentId': {len(b_with_parent)}")

if b_with_id:
    print(f"\nIDs in trace: {sorted(set(e['id'] for e in b_with_id))}")
