<img src="docs/logo.png" width="64" align="left">

# reach

Tiny, fast desktop control for Windows, made for AI agents.
<br clear="left">

## Install

Grab `reach.exe` from [Releases](../../releases), put it wherever you like, and run it once. It adds its folder to your PATH (open a new terminal after) and starts a small tray server. Don't want the PATH bit? Use `--no-path` or `reach path remove`.

## Use it

```
reach screenshot                 saves a JPEG, prints its size and the cursor
reach click 640 360              coordinates are screenshot pixels
reach type hello world
reach key ctrl+shift+t
reach open notepad
reach ui find Save               e12 button "Save" 913,40
reach ui invoke e12
```

`reach help` lists everything else.

## MCP

Claude Code:

```powershell
$t = Get-Content "$env:USERPROFILE\.reach\token"
claude mcp add --transport http reach http://127.0.0.1:7420/mcp --header "Authorization: Bearer $t"
```

Cursor (`~/.cursor/mcp.json`, with `REACH_TOKEN` set to that token):

```json
{
  "mcpServers": {
    "reach": {
      "url": "http://127.0.0.1:7420/mcp",
      "headers": { "Authorization": "Bearer ${env:REACH_TOKEN}" }
    }
  }
}
```

## What you'll notice

- **Agent pill:** a little pill next to your cursor shows which agent is driving and what it's doing.
- **Pause on user:** touch the mouse or keyboard and reach waits until you've been idle for 2 seconds.
- **Motion:** the mouse glides and typing is paced like a person. Use `reach motion fast` or `--fast` when you want speed.

## Security

It only listens on localhost, and every request needs the bearer token in `%USERPROFILE%\.reach\token`. Keep that token private.

## License

MIT
