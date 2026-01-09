#!/usr/bin/env python3
"""Convert tracing JSON output to Chrome trace format."""
import json
import sys
from datetime import datetime
from typing import List, Dict, Any

def parse_timestamp(ts_str: str) -> int:
    """Convert ISO 8601 timestamp to microseconds since epoch."""
    try:
        dt = datetime.fromisoformat(ts_str.replace('Z', '+00:00'))
        return int(dt.timestamp() * 1_000_000)
    except (ValueError, AttributeError):
        return 0

def convert_to_chrome_trace(input_file: str, output_file: str):
    """Convert tracing JSON to Chrome trace format."""
    events: List[Dict[str, Any]] = []

    # Track span state: span_key -> {start_time, name, parent_id, fields, depth}
    span_state: Dict[str, Dict[str, Any]] = {}

    # First pass: collect all timestamps and find minimum
    all_timestamps = []
    raw_events = []

    with open(input_file, 'r') as f:
        for line in f:
            line = line.strip()
            if not line:
                continue

            try:
                data = json.loads(line)
                raw_events.append(data)

                if 'timestamp' in data:
                    ts = parse_timestamp(data['timestamp'])
                    all_timestamps.append(ts)

            except json.JSONDecodeError as e:
                print(f"JSON decode error: {e}", file=sys.stderr)
                continue
            except Exception as e:
                print(f"Error processing line: {e}", file=sys.stderr)
                continue

    # Calculate offset from first event
    min_timestamp = min(all_timestamps) if all_timestamps else 0
    max_timestamp = max(all_timestamps) if all_timestamps else 0

    print(f"Time range: {len(all_timestamps)} events, from {min_timestamp} to {max_timestamp}")
    print(f"Time span: {(max_timestamp - min_timestamp) / 1000:.2f} ms")

    # Process events
    for data in raw_events:
        if 'span' not in data:
            continue

        span = data['span']
        fields = data.get('fields', {})
        message = fields.get('message', '')
        name = span.get('name', 'unknown')

        # Adjust timestamp
        timestamp_us = parse_timestamp(data.get('timestamp', '')) - min_timestamp

        # Extract span info from spans array for depth calculation
        spans = data.get('spans', [])

        # Create unique key for this span based on name and its own fields
        # Use only the current span's fields, not parent's
        span_path = span.get('path', '')
        span_specifier = span.get('specifier', '')

        # Create a more specific key to distinguish different calls
        # Include parent name to distinguish nested calls
        parent_name = spans[-1].get('name', '') if spans else ''
        span_key = f"{name}:{span_path}:{span_specifier}:{parent_name}"

        # Calculate depth based on spans array length
        depth = len(spans)

        if message == 'new' or (message == 'enter' and span_key not in span_state):
            # Span creation or first enter - record start time
            if span_key not in span_state:
                span_state[span_key] = {
                    'start': timestamp_us,
                    'name': name,
                    'path': span_path,
                    'specifier': span_specifier,
                    'depth': depth
                }

        elif message == 'close':
            # Span close - create Chrome events only once
            if span_key in span_state:
                span_data = span_state[span_key]

                # Build args object - only include fields that actually exist
                args = {}
                if span_data['path']:
                    args['path'] = span_data['path']
                if span_data['specifier']:
                    args['specifier'] = span_data['specifier']

                # Create begin event
                begin_event = {
                    'name': span_data['name'],
                    'ph': 'B',
                    'ts': span_data['start'],
                    'pid': 1,
                    'tid': depth,
                }
                if args:
                    begin_event['args'] = args

                events.append(begin_event)

                # Create end event
                end_event = {
                    'name': span_data['name'],
                    'ph': 'E',
                    'ts': timestamp_us,
                    'pid': 1,
                    'tid': depth
                }
                events.append(end_event)

            # Note: Skip duration events - they're redundant since time info
            # is already captured in the B/E events' ts fields

            # Clean up span state
            span_state.pop(span_key, None)

        # Capture field data like 'ret' for results
        elif 'ret' in fields or 'err' in fields:
            args = {}
            if span_path:
                args['path'] = span_path
            if span_specifier:
                args['specifier'] = span_specifier

            result_event = {
                'name': 'result',
                'ph': 'i',
                'ts': timestamp_us,
                'pid': 1,
                'tid': depth,
                's': 'p',
            }
            if args:
                result_event['args'] = args

            if 'ret' in fields:
                result_event.setdefault('args', {})['return'] = fields['ret']
            if 'err' in fields:
                result_event.setdefault('args', {})['error'] = fields['err']
            events.append(result_event)

    # Sort events by timestamp
    events.sort(key=lambda x: x.get('ts', 0))

    # Write Chrome trace format
    trace = {'traceEvents': events}
    with open(output_file, 'w') as f:
        json.dump(trace, f, indent=2)

    print(f"Converted {len(events)} events from {input_file} to Chrome trace format: {output_file}")

if __name__ == '__main__':
    if len(sys.argv) != 3:
        print(f"Usage: {sys.argv[0]} <input.json> <output.json>")
        print(f"Example: {sys.argv[0]} trace.json chrome-trace.json")
        sys.exit(1)

    convert_to_chrome_trace(sys.argv[1], sys.argv[2])
