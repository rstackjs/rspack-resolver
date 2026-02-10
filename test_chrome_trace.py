import json

# 读取 trace
with open('trace.json', 'r') as f:
    trace = json.load(f)

print(f"Total events: {len(trace)}")
print(f"\nSpan types: {set(e.get('ph') for e in trace)}")

print(f"\nSample events:")
for i, event in enumerate(trace[:5]):
    print(f"{i+1}. {event.get('ph')} - {event.get('name', 'N/A')}")
    print(f"   Keys: {sorted(event.keys())}")

print(f"\nEvents with 'id' field: {len([e for e in trace if 'id' in e])}")
print(f"Events with 'ph=X' (cross-flow): {len([e for e in trace if e.get('ph') == 'X'])}")

# 检查是否有嵌套的 span
span_names = [e.get('name') for e in trace if e.get('ph') == 'B']
print(f"\nSpan names: {set(span_names)}")
