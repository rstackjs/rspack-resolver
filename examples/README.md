# Resolver Example - Tracing Support

This example demonstrates the module resolution capabilities of rspack-resolver with detailed tracing support.

## Features

- **Human-readable output**: Full span tracing with colored output
- **JSON trace output**: Generate machine-readable traces for visualization
- **Chrome/Perfetto compatible**: Convert traces to Chrome DevTools format
- **Call hierarchy visualization**: See the complete function call stack with timing

## Usage

### Basic Usage (Human-readable)

```bash
cargo run -F=enable_instrument --example resolver /absolute/path/to/dir ./module
```

Example:
```bash
cargo run -F=enable_instrument --example resolver /Users/bytedance/project/app ./index.js
```

### JSON Trace Output

Set the `TRACE_FILE` environment variable to output tracing data in JSON format:

```bash
TRACE_FILE=trace.json cargo run -F=enable_instrument --example resolver /absolute/path/to/dir ./module
```

The JSON file will contain one JSON object per line with span events including:
- `timestamp` - ISO 8601 timestamp
- `level` - Log level (DEBUG, INFO, etc.)
- `fields.message` - Event type (new, enter, exit, close)
- `fields.time.busy`, `fields.time.idle` - Timing information
- `fields.ret` - Return value on success
- `fields.err` - Error value on failure
- `span` - Span metadata (name, path, specifier)
- `spans` - Parent span information for hierarchy

### Convert JSON Trace to Chrome Format

Convert the JSON trace to Chrome/Perfetto trace format:

```bash
python3 scripts/convert-trace.py trace.json chrome-trace.json
```

The script will:
- Parse all JSON lines
- Build span hierarchy from parent-child relationships
- Convert timestamps to microseconds since epoch
- Generate Chrome-compatible trace events
- Output the number of converted events

### Visualize in Chrome DevTools

1. Open Chrome and navigate to `chrome://tracing`
2. Click "Load" and select `chrome-trace.json`
3. You'll see an interactive timeline showing:
   - Function call hierarchy (nested spans)
   - Execution time for each function
   - Parent-child relationships
   - Span arguments (path, specifier)
   - Result values (ret, err)
   - Duration breakdown (busy vs idle time)

4. Use the timeline controls to:
   - Zoom in/out with mouse wheel
   - Click on spans to see details
   - Search for specific function names
   - Filter by thread ID (represents call depth)

## Log Levels

Control verbosity using `RUST_LOG`:

```bash
# Only ERROR and WARN
RUST_LOG=warn cargo run -F=enable_instrument --example resolver /path ./module

# INFO and above
RUST_LOG=info cargo run -F=enable_instrument --example resolver /path ./module

# DEBUG and above (default)
RUST_LOG=debug cargo run -F=enable_instrument --example resolver /path ./module

# TRACE and above (most verbose)
RUST_LOG=trace cargo run -F=enable_instrument --example resolver /path ./module

# Specific module only
RUST_LOG=rspack_resolver=debug cargo run -F=enable_instrument --example resolver /path ./module

# Exclude certain modules
RUST_LOG=debug,rspack_resolver=trace cargo run -F=enable_instrument --example resolver /path ./module
```

## Output Format

### Human-readable Format

The default output shows colored span events:

- `new` - Span creation
- `enter` - Function entry
- `exit` - Function exit (returns a value)
- `close` - Span closure with timing info (`time.busy`, `time.idle`)

Example output:
```
DEBUG resolve_tracing{path=/path specifier="./module"}: new
DEBUG resolve_tracing{path=/path specifier="./module"}: enter
DEBUG resolve_tracing{path=/path specifier="./module"}: resolve{path="/path" specifier="./module"}: new
...
DEBUG resolve_tracing{path=/path specifier="./module"}: options=... ret="/path/module.js"
DEBUG resolve_tracing{path=/path specifier="./module"}: exit
DEBUG resolve_tracing{path=/path specifier="./module"}: close time.busy=12.0ms time.idle=31.9µs
```

### Chrome Trace Format

The converted trace contains:

- **Complete events** (`ph: "B"`, `ph: "E"`): Begin/end pairs for each function
- **Duration events** (`ph: "i"`, `name: "duration"`): Timing information
- **Result events** (`ph: "i"`, `name: "result"`): Return values or errors

Each event includes:
- `name` - Function name
- `ph` - Phase type (B=Begin, E=End, i=Instant)
- `ts` - Timestamp in microseconds
- `pid` - Process ID (always 1)
- `tid` - Thread ID (represents call depth)
- `args` - Additional arguments (path, specifier, return value, etc.)

## Traced Functions

The following resolution functions are instrumented with `tracing::instrument`:

- `resolve_tracing` - Top-level resolution wrapper
- `resolve` - Main resolve function
- `resolve_impl` - Core implementation
- `require_without_parse` - Module requirement without parsing
- `require_relative` - Relative path resolution
- `require_absolute` - Absolute path resolution
- `load_as_file_or_directory` - Load file or directory
- `load_as_file` - Load file
- `load_as_directory` - Load directory
- `load_extension_alias` - Extension alias handling
- `load_alias_or_file` - Alias or file resolution
- `find_package_json` - Package.json discovery
- And many more...

## Examples

### Simple Resolve

```bash
# Human-readable output
cargo run -F=enable_instrument --example resolver /tmp/project ./index.js

# JSON trace
TRACE_FILE=trace.json cargo run -F=enable_instrument --example resolver /tmp/project ./index.js
python3 scripts/convert-trace.py trace.json chrome-trace.json
```

### With Custom Log Level

```bash
RUST_LOG=rspack_resolver=trace cargo run -F=enable_instrument --example resolver /tmp/project ./module.js
```

### Complex Resolve (with all traces)

```bash
TRACE_FILE=full-trace.json RUST_LOG=trace cargo run -F=enable_instrument --example resolver /tmp/project ./module.js
python3 scripts/convert-trace.py full-trace.json chrome-trace.json
# Open chrome://tracing and load chrome-trace.json
```

## Troubleshooting

### No tracing output

Ensure you're using the `enable_instrument` feature:
```bash
cargo run -F=enable_instrument --example resolver ...
```

### Too much output

Reduce log level:
```bash
RUST_LOG=info cargo run -F=enable_instrument --example resolver ...
```

### JSON trace file is empty

Make sure the `TRACE_FILE` path is writable:
```bash
TRACE_FILE=/tmp/trace.json cargo run -F=enable_instrument --example resolver ...
ls -la /tmp/trace.json
```

### Chrome trace shows no events

1. Verify the JSON trace has content:
   ```bash
   wc -l trace.json
   ```
2. Check for parsing errors during conversion
3. Ensure the Chrome trace file is valid JSON:
   ```bash
   cat chrome-trace.json | python3 -m json.tool > /dev/null
   ```

### Timeline shows wrong timing

- Timestamps are converted from ISO 8601 to microseconds since Unix epoch
- This conversion is done by `datetime.fromisoformat()` and `.timestamp()`
- If you see incorrect timing, check your system clock

## Advanced Usage

### Filtering Specific Functions

You can modify the `convert-trace.py` script to filter events:

```python
# Add this after events are created
filtered_events = [e for e in events if 'resolve' in e['name']]
events = filtered_events
```

### Custom Visualization

The JSON trace format can be imported into other tools:
- **Perfetto** - https://ui.perfetto.dev/
- **FlameGraph** - After converting to flamegraph format
- **Jaeger** - With appropriate adapter

## Performance Notes

- Tracing adds minimal overhead (< 5%)
- JSON output is slightly slower due to serialization
- For production use, consider disabling tracing or using higher log levels
- The `enable_instrument` feature is off by default in release builds
