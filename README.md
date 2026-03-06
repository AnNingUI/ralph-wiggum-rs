# Ralph Wiggum (Rust)

## What is this?

This project is a Rust implementation of [open-ralph-wiggum](https://github.com/Th0rgal/open-ralph-wiggum).
Just to use it without getting confused by errors caused by different versions of `js runtime`

## How to Build

```bash
git clone https://github.com/AnNingUI/ralph-wiggum-rs
cd ralph-wiggum-rs
cargo build --release
```

### Installation

**Windows:**
```bash
move .\target\release\ralph-rs.exe <YOUR_SOFTWARE_ENV_DIR>\ralph-rs.exe
```

**Linux/macOS:**
```bash
sudo mv ./target/release/ralph-rs /usr/local/bin/ralph-rs
```

## Usage

```bash
# Basic usage with opencode (recommended)
ralph-rs "Your task" --agent opencode --model claude-sonnet-4 -n 10

# Using codex (reads from ~/.codex/config.toml)
ralph-rs "Your task" --agent codex -n 10

# Check status
ralph-rs status

# Stop loop
ralph-rs stop
```

## Important Notes

### Codex Configuration

If you're using codex with a third-party API provider, ralph-rs might not be able to read your API key from `auth.json` due to permission issues.

**Solution:** Use environment variables instead.

Add this to your `~/.codex/config.toml`:

```toml
[model_providers.<YOUR_PROVIDER>]
env_key = "YOUR_API_KEY_ENV_NAME"
```

Then set the environment variable:

**Linux/macOS:**
```bash
export YOUR_API_KEY_ENV_NAME="sk-your-key-here"
```

**Windows CMD:**
```cmd
setx YOUR_API_KEY_ENV_NAME "sk-your-key-here"
```

**Windows PowerShell:**
```powershell
$env:YOUR_API_KEY_ENV_NAME = "sk-your-key-here"
# Or permanently:
[System.Environment]::SetEnvironmentVariable('YOUR_API_KEY_ENV_NAME', 'sk-your-key-here', 'User')
```

### Recommended: Use opencode

To avoid configuration issues, we recommend using **opencode** instead of codex:

```bash
ralph-rs "Your task" --agent opencode --model claude-sonnet-4 -n 10
```

## Thanks To

- [Original Open Ralph Wiggum project](https://github.com/Th0rgal/open-ralph-wiggum)
- [Original Ralph Wiggum technique by Geoffrey Huntley](https://ghuntley.com/ralph/)
- [Ralph Orchestrator](https://github.com/mikeyobrien/ralph-orchestrator)

## License

MIT
