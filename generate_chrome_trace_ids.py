import json

# 为现有的 trace 添加 ID 字段，建立父子关系
def add_span_ids(trace_data):
    # 首先按时间排序
    trace_sorted = sorted(trace_data, key=lambda x: x.get('ts', 0))
    
    # 追踪当前活动栈
    active_spans = []  # [(id, depth)]
    next_id = 1
    
    updated_trace = []
    
    for event in trace_sorted:
        ph = event.get('ph')
        
        if ph == 'B':
            # Begin event: 分配新的 ID
            current_id = next_id
            next_id += 1
            
            # 计算深度（基于当前活动 span 数量）
            depth = len(active_spans)
            
            # 添加 ID 和 parentId
            event['id'] = current_id
            if active_spans:
                # 设置父 ID 为栈顶的 span
                parent_id, _ = active_spans[-1]
                event['parentId'] = parent_id
            
            # 压入栈
            active_spans.append((current_id, depth))
            
            updated_trace.append(event)
            
        elif ph == 'E':
            # End event: 从栈中弹出对应的 B
            if active_spans:
                parent_id, depth = active_spans.pop()
                event['id'] = parent_id
                updated_trace.append(event)
            else:
                # 没有对应的 B，直接添加
                updated_trace.append(event)
        else:
            # 其他事件（M, i 等），直接添加
            updated_trace.append(event)
    
    return updated_trace

if __name__ == "__main__":
    input_file = "trace.json"
    output_file = "trace_with_ids.json"
    
    print(f"Reading {input_file}...")
    with open(input_file, 'r') as f:
        trace_data = json.load(f)
    
    print(f"Processing {len(trace_data)} events...")
    trace_with_ids = add_span_ids(trace_data)
    
    print(f"Writing to {output_file}...")
    with open(output_file, 'w') as f:
        json.dump(trace_with_ids, f, indent=2)
    
    # 验证
    with open(output_file, 'r') as f:
        new_trace = json.load(f)
    
    events_with_id = len([e for e in new_trace if 'id' in e])
    events_with_parentid = len([e for e in new_trace if 'parentId' in e])
    
    print(f"\nDone!")
    print(f"Events with 'id': {events_with_id}")
    print(f"Events with 'parentId': {events_with_parentid}")
