# Zed Presence WebSocket

An extension for [Zed](https://zed.dev) that sends presence data to a WebSocket server instead of Discord.

## Overview

This is a modified version of the [zed-discord-presence](https://github.com/xhyrom/zed-discord-presence) extension that sends presence data via WebSocket instead of Discord Rich Presence. It monitors your coding activity in Zed and sends real-time updates to a WebSocket server.

## Requirements

[Rust](https://rust-lang.org) is required for installing this extension. The easiest way to get Rust is by using [rustup](https://rustup.rs).

## Installation

Since [zed-industries/extensions#1217](https://github.com/zed-industries/extensions/pull/1217) has been merged, you can simply download the extension in `zed: extensions`.

### Dev Installation

1. Clone this repository
2. `Ctrl+Shift+P` and select `zed: install dev extension`
3. Choose the directory where you cloned this repository
4. Enjoy :)

## Configuration

Configure the WebSocket server URL by setting the `WEBSOCKET_URL` environment variable:

```bash
export WEBSOCKET_URL="ws://localhost:3000/ws"
```

Or in Zed settings:

```jsonc
{
  "lsp": {
    "presence_websocket": {
      "initialization_options": {
        "websocket_url": "ws://localhost:3000/ws"
      }
    }
  }
}
```

## Data Format

The extension sends presence data in this JSON format:

```json
{
  "event": "presence",
  "presence": {
    "fileName": "App.tsx",
    "extension": "tsx",
    "timeSpent": 300,
    "languageIcon": "https://example.com/tsx.png",
    "ideIcon": "https://example.com/zed.png",
    "workingDirectory": "/home/user/project",
    "gitUrl": "https://github.com/user/repo"
  }
}
```

## WebSocket Server

You'll need a WebSocket server that can receive these presence updates. The server should:

1. Accept WebSocket connections on the configured URL
2. Listen for messages with `event: "presence"`
3. Handle the presence data appropriately (display in UI, store, etc.)

Example acknowledgment response:
```json
{
  "event": "acknowledged",
  "message": "Presence data received"
}
```

## Configuration Options

Most of the original Discord configuration options still work for customizing what data is sent:

- `state` - Custom state message
- `details` - Custom details message
- `idle` settings - Behavior when inactive
- `rules` - Workspaces to exclude/include
- `git_integration` - Include Git repository URLs
- `languages` - Per-language overrides

## Differences from Discord Version

- **No Discord dependency**: No need for Discord app or IPC
- **WebSocket transport**: Uses standard WebSocket protocol
- **Server flexibility**: Connect to any WebSocket server
- **No rich presence**: Just sends structured presence data

## Development

To modify the extension:

1. Edit the Rust code in `lsp/src/`
2. Run `cargo build` to compile
3. Reload the extension in Zed

## License

This project is licensed under the GNU General Public License v3.0.</content>
<parameter name="filePath">/home/studio/Projects/fintech-academy/zed-discord-presence/README.md
