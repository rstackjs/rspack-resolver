# Resolver Example - Tracing Support

This example demonstrates the module resolution capabilities of rspack-resolver with detailed tracing support.

## Features

- **Human-readable output**: Full span tracing with colored output
- **Chrome/Perfetto compatible**: Direct Chrome DevTools trace output
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

### Chrome Trace Output

Set the `TRACE_FILE` environment variable to output tracing data in Chrome-compatible format:

```bash
TRACE_FILE=trace.json cargo run -F=enable_instrument --example resolver /absolute/path/to/dir ./module
```

The file will be generated in Chrome trace format directly, ready for visualization.

### Visualize in Chrome DevTools

1. Open Chrome and navigate to `chrome://tracing`
2. Click "Load" and select `trace.json`
3. You'll see an interactive timeline showing:
   - Function call hierarchy (nested spans)
   - Execution time for each function
   - Parent-child relationships
   - Span arguments (path, specifier)
   - Result values (ret, err)
   - Duration breakdown

4. Use the timeline controls to:
   - Zoom in/out with mouse wheel
   - Click on spans to see details
   - Search for specific function names
   - Filter by thread ID

Alternatively, you can view the trace in [Perfetto UI](https://ui.perfetto.dev/):

1. Open https://ui.perfetto.dev/
2. Click "Open trace file" and select `trace.json`

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

The generated trace file contains Chrome-compatible trace events with:

- **Begin/End events** for each function call span
- **Metadata** including function name, timestamps, thread ID
- **Arguments** (path, specifier) included in the trace
- **Timing information** showing execution duration

Each event includes:

- `name` - Function name
- `ph` - Phase type (B=Begin, E=End, etc.)
- `ts` - Timestamp in microseconds
- `pid` - Process ID
- `tid` - Thread ID
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

# Chrome trace
TRACE_FILE=trace.json cargo run -F=enable_instrument --example resolver /tmp/project ./index.js
# Then open chrome://tracing and load trace.json
```

### With Custom Log Level

```bash
RUST_LOG=rspack_resolver=trace cargo run -F=enable_instrument --example resolver /tmp/project ./module.js
```

### Complex Resolve (with Chrome trace)

```bash
TRACE_FILE=full-trace.json RUST_LOG=trace cargo run -F=enable_instrument --example resolver /tmp/project ./module.js
# Open chrome://tracing or https://ui.perfetto.dev/ and load full-trace.json
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

### Chrome trace file is empty

Make sure the `TRACE_FILE` path is writable:

```bash
TRACE_FILE=/tmp/trace.json cargo run -F=enable_instrument --example resolver ...
ls -la /tmp/trace.json
```

### Chrome trace shows no events

1. Verify the trace file has content:
   ```bash
   wc -l trace.json
   ```
2. Check that the file is valid JSON:
   ```bash
   cat trace.json | python3 -m json.tool > /dev/null
   ```

## Performance Notes

- Tracing adds minimal overhead (< 5%)
- Chrome trace output is efficient and production-ready
- For production use, consider disabling tracing or using higher log levels
- The `enable_instrument` feature is off by default in release builds
