import json
import sys
from collections import defaultdict

def merge_tracing_spans(trace_data):
    # 按函数名、参数、线程 ID 分组
    span_groups = defaultdict(list)
    
    for event in trace_data:
        if event.get("ph") in ["B", "E"]:
            # 生成唯一 key: name + args + pid + tid
            args = event.get("args", {})
            args_str = json.dumps(args, sort_keys=True)
            key = f"{event.get('name')}|{args_str}|{event.get('pid')}|{event.get('tid')}"
            span_groups[key].append(event)
    
    # 找到需要保留的事件
    events_to_remove = set()
    merged_spans = []
    
    for key, spans in span_groups.items():
        if len(spans) <= 2:  # 只有一对 B/E，不需要合并
            print(f"Skipping {len(spans)} spans for '{spans[0].get('name')}' (key: {key[:50]}...)")
            continue
        
        # 排序
        spans.sort(key=lambda x: x["ts"])
        
        # 保留第一个 B 和最后一个 E
        first_b = spans[0]
        last_e = spans[-1]
        
        # 检查是否确实是一组交替的 B/E
        is_alternating = True
        for i, span in enumerate(spans):
            expected_ph = "B" if i % 2 == 0 else "E"
            if span["ph"] != expected_ph:
                is_alternating = False
                break
        
        if is_alternating:
            # 创建合并后的 span
            merged_span = {
                **first_b,
                "ts": last_e["ts"]  # 使用最后一个 E 的结束时间作为 duration
            }
            if "dur" in last_e:
                merged_span["dur"] = last_e["dur"]
            
            merged_spans.append(merged_span)
            
            # 标记中间的所有 span 为需要删除
            for span in spans:
                events_to_remove.add(id(span))
            
            print(f"Merged {len(spans)} spans for '{first_b.get('name')}' (key: {key[:50]}...) {first_b}")
    
    # 重建 trace：删除中间的 span，添加合并后的
    filtered_trace = [
        event for event in trace_data 
        if id(event) not in events_to_remove or event.get("ph") not in ["B", "E"]
    ]
    
    # 添加合并后的 spans
    filtered_trace.extend(merged_spans)
    
    # 按时间戳重新排序
    filtered_trace.sort(key=lambda x: x.get("ts", 0))
    
    return filtered_trace

if __name__ == "__main__":
    input_file = sys.argv[1] if len(sys.argv) > 1 else "trace.json"
    output_file = sys.argv[2] if len(sys.argv) > 2 else "trace_merged.json"
    
    print(f"Reading {input_file}...")
    with open(input_file, 'r') as f:
        trace_data = json.load(f)
    
    print(f"Processing {len(trace_data)} events...")
    merged_trace = merge_tracing_spans(trace_data)
    
    print(f"Writing {len(merged_trace)} events to {output_file}...")
    with open(output_file, 'w') as f:
        json.dump(merged_trace, f, indent=2)
    
    print("Done!")
