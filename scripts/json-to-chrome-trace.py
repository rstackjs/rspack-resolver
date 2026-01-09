#!/usr/bin/env python3
"""Convert tracing JSON output to Chrome trace format."""
import json
import sys

def convert_to_chrome_trace(input_file, output_file):
    """Convert tracing JSON to Chrome trace format."""
    events = []
    
    with open(input_file, 'r') as f:
        for line in f:
            line = line.strip()
            if not line:
                continue
            
            try:
                data = json.loads(line)
                
                # Extract span info
                if 'span' in data:
                    span = data['span']
                    fields = span.get('fields', {})
                    
                    name = span.get('name', 'unknown')
                    span_id = span.get('id', 0)
                    parent_id = span.get('parent_id', 0)
                    
                    timestamp = data.get('timestamp', 0)
                    duration = data.get('duration_nanos', 0) // 1000  # Convert to microseconds
                    
                    # Get additional fields
                    path = fields.get('path', '')
                    specifier = fields.get('specifier', '')
                    
                    # Create begin event
                    begin_event = {
                        'name': name,
                        'ph': 'B',  # Begin event
                        'ts': timestamp,
                        'pid': 1,
                        'tid': 1,
                        'id': span_id,
                        'args': {
                            'path': str(path),
                            'specifier': str(specifier)
                        }
                    }
                    if parent_id:
                        begin_event['pid'] = parent_id
                    events.append(begin_event)
                    
                    # Create end event
                    end_event = {
                        'name': name,
                        'ph': 'E',  # End event
                        'ts': timestamp + duration,
                        'pid': 1,
                        'tid': 1,
                        'id': span_id
                    }
                    if parent_id:
                        end_event['pid'] = parent_id
                    events.append(end_event)
                    
            except json.JSONDecodeError:
                continue
    
    # Write Chrome trace format
    trace = {'traceEvents': events}
    with open(output_file, 'w') as f:
        json.dump(trace, f, indent=2)
    
    print(f"Converted {input_file} to Chrome trace format: {output_file}")

if __name__ == '__main__':
    if len(sys.argv) != 3:
        print(f"Usage: {sys.argv[0]} <input.json> <output.json>")
        sys.exit(1)
    
    convert_to_chrome_trace(sys.argv[1], sys.argv[2])
