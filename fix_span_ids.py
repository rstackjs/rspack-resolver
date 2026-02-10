import json

# 正确的实现：B 和 E 共享同一个 ID
def add_span_ids_correct(trace_data):
    # 首先按时间排序
    trace_sorted = sorted(trace_data, key=lambda x: x.get('ts', 0))
    
    # 追踪当前活动栈 [(id, event)]
    active_spans = []
    next_id = 1
    
    updated_trace = []
    
    for event in trace_sorted:
        ph = event.get('ph')
        
        if ph == 'B':
            # Begin event: 分配新的 ID
            current_id = next_id
            next_id += 1
            
            # 添加 ID
            event['id'] = current_id
            
            # 设置父 ID（如果栈中有其他 span）
            if active_spans:
                parent_id, _ = active_spans[-1]
                event['parentId'] = parent_id
            
            # 压入栈
            active_spans.append((current_id, event))
            
            updated_trace.append(event)
            
        elif ph == 'E':
            # End event: 从栈中弹出对应的 B
            if active_spans:
                parent_id, begin_event = active_spans.pop()
                # E 事件使用和 B 相同的 ID
                event['id'] = parent_id
                # E 事件不需要 parentId（B 事件已经有了）
                # Chrome 通过 id 匹配来连接 B 和 E
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
    output_file = "trace_with_ids_fixed.json"
    
    print(f"Reading {input_file}...")
    with open(input_file, 'r') as f:
        trace_data = json.load(f)
    
    print(f"Processing {len(trace_data)} events...")
    trace_with_ids = add_span_ids_correct(trace_data)
    
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
    
    # 检查一个完整的 span
    print("\nSample resolve_with_context span:")
    rwc_events = [e for e in new_trace if e.get('name') == 'resolve_with_context' and e.get('ph') in ['B', 'E']]
    if rwc_events:
        print(f"\nBegin: {json.dumps(rwc_events[0], indent=2)}")
        if len(rwc_events) > 1:
            print(f"\nEnd: {json.dumps(rwc_events[1], indent=2)}")
