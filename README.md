# harness-picker

`hpick` — one fzf pick to start a new Claude Code, Codex or pi chat, or resume one you started in the current folder.

Made to be run from [Zed](https://zed.dev)'s terminal as a quick agent picker, but it works in any terminal.

![hpick](assets/screenshot.png)

Needs [fzf](https://github.com/junegunn/fzf) 0.42+ (the package managers below install it) and whichever of `claude`, `codex`, `pi` you use.

## Install

```sh
brew install devnull03/tap/harness-picker        # macOS (Apple Silicon), Linux
```

```powershell
scoop bucket add devnull03 https://github.com/devnull03/scoop-bucket
scoop install harness-picker                      # Windows
winget install devnull03.HarnessPicker            # Windows
```

Or download a binary from [Releases](https://github.com/devnull03/harness-picker/releases/latest) and put it on your `PATH`, or `cargo install --git https://github.com/devnull03/harness-picker`.

## Use

Run `hpick` in a project folder. Type to filter, Enter to open, Esc to cancel.

Chats are read from `~/.claude/history.jsonl`, `~/.codex/history.jsonl` + `~/.codex/sessions`, and `~/.pi/agent/sessions`, and are listed only if they were started in the current folder.
