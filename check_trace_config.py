import json

# 分析当前 trace 的结构
with open('trace.json', 'r') as f:
    trace = json.load(f)

print("=== Chrome Trace Event Format Analysis ===\n")

# Chrome trace event format 包含的字段
print("\n1. Events with 'ph' field:")
ph_types = set(e.get('ph') for e in trace)
for ph in sorted(ph_types):
    count = len([e for e in trace if e.get('ph') == ph])
    print(f"   {ph}: {count} events")

print("\n2. Checking for span ID fields:")
for field in ['id', 'scope', 'parentId', 'bind_id']:
    count = len([e for e in trace if field in e])
    if count > 0:
        print(f"   {field}: {count} events have this field")
    else:
        print(f"   {field}: 0 events have this field")

print("\n3. Sample B (begin) event:")
begin_events = [e for e in trace if e.get('ph') == 'B']
if begin_events:
    print(json.dumps(begin_events[0], indent=2))

print("\n4. Chrome Trace Format specification:")
print("   According to Chrome DevTools Protocol:")
print("   - B/E events should have 'id' field for parent-child relationships")
print("   - X events represent cross-flow links (async continuation)")
print("   - 'id' + 'ph' uniquely identifies a span")
print("   - Parent-child is determined by timeline nesting OR explicit 'id' field")
