# Stdin Architecture in Command Executor

## Overview

The stdin support in command-executor provides flexible ways to send input to processes:

```
┌─────────────────┐
│     User Code   │
└────────┬────────┘
         │
         ├─── Direct Writing ──────┐
         │                         │
         └─── Channel-based ───┐   │
                              │   │
                              ▼   ▼
┌─────────────────────────────────────┐
│            Command Builder          │
│  ┌─────────────────────────────┐   │
│  │ stdin_channel: Option<Rx>   │   │
│  └─────────────────────────────┘   │
└─────────────────┬───────────────────┘
                  │
                  ▼
┌─────────────────────────────────────┐
│          LocalLauncher              │
│  • Always creates piped stdin       │
│  • Creates StdinHandle              │
└─────────────────┬───────────────────┘
                  │
                  ▼
┌─────────────────────────────────────┐
│          StdinHandle                │
│  ┌─────────────────────────────┐   │
│  │ stdin: Option<ChildStdin>   │   │
│  │ channel: Option<Receiver>   │   │
│  └─────────────────────────────┘   │
│                                     │
│  Methods:                           │
│  • write_line() - Write with \n    │
│  • write() - Write raw bytes       │
│  • close() - Close stdin (EOF)     │
│  • forward_channel() - Auto forward│
└─────────────────┬───────────────────┘
                  │
                  ▼
┌─────────────────────────────────────┐
│         Child Process               │
│  (cat, grep, bc, etc.)             │
└─────────────────────────────────────┘
```

## Usage Patterns

### 1. Direct Writing
Best for: Known input, simple scripts, testing

```rust
let mut cmd = Command::new("grep");
cmd.arg("pattern");

let (events, mut handle) = executor.launch(&target, cmd).await?;

if let Some(stdin) = handle.stdin_mut() {
    stdin.write_line("line 1").await?;
    stdin.write_line("line 2").await?;
    stdin.close(); // Send EOF
}
```

### 2. Channel-based Input
Best for: Streaming data, async producers, unknown amount of input

```rust
let (tx, rx) = async_channel::unbounded();

let mut cmd = Command::new("wc");
cmd.stdin_channel(rx);

let (events, handle) = executor.launch(&target, cmd).await?;

// Can send from multiple tasks
tx.send("data".to_string()).await?;
// Drop tx to close channel and send EOF
drop(tx);
```

### 3. Interactive Communication
Best for: REPL-like processes, calculators, interactive tools

```rust
let cmd = Command::new("python3");
let (mut events, mut handle) = executor.launch(&target, cmd).await?;

// Send Python commands
if let Some(stdin) = handle.stdin_mut() {
    stdin.write_line("print(2 + 2)").await?;
    stdin.write_line("import math").await?;
    stdin.write_line("print(math.pi)").await?;
    stdin.write_line("exit()").await?;
}
```

## How It Works

1. **Command Creation**: When you create a Command, you can optionally attach a channel for stdin input:
   ```rust
   let mut cmd = Command::new("cat");
   cmd.stdin_channel(receiver);
   ```

2. **Process Launch**: LocalLauncher always configures stdin as piped (not null):
   ```rust
   async_cmd.stdin(Stdio::piped());
   ```

3. **Handle Creation**: A StdinHandle is created wrapping the ChildStdin:
   ```rust
   let stdin_handle = stdin.map(|s| StdinHandle::new(s, stdin_channel));
   ```

4. **Writing Data**: 
   - Direct: `stdin.write_line("data")` or `stdin.write(b"bytes")`
   - Channel: Data sent through channel could be forwarded automatically
   - Close: `stdin.close()` drops the writer, sending EOF

5. **Process Receives**: The child process reads from its stdin as normal

## Key Design Decisions

1. **Always Piped**: We always create stdin as piped, even without a channel, to allow direct writing.

2. **Optional Channel**: The channel is optional - you can use direct writing, channel-based, or both.

3. **Explicit Close**: You must explicitly close stdin to send EOF. This prevents hanging processes.

4. **Line-oriented API**: `write_line()` automatically adds newlines, making it easy to work with line-oriented tools.

5. **Runtime Agnostic**: Uses async-process, not tokio-specific APIs.

## Future Enhancements

1. **Auto-forwarding**: Currently, channel forwarding needs to be manually implemented. Could add automatic forwarding task.

2. **Layer Support**: Stdin needs to work through SSH and Docker layers:
   ```rust
   // Future: SSH layer preserves stdin
   executor.with_layer(SshLayer::new("host"))
           .launch(target, cmd_with_stdin).await?;
   ```

3. **Binary Support**: Better support for binary stdin (not just text).

4. **Buffering Control**: Options for buffering behavior.

## Testing

Tests verify:
- Stdin handle is available when process launches
- Direct writing works correctly
- Processes receive and process stdin data
- EOF is properly signaled with close()

See `src/stdin_test.rs` for test examples.