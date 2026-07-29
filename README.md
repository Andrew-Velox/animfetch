# animfetch

An animated system fetch you can work inside. The fetch stays pinned at the top
of the screen and keeps animating while your prompt and command output scroll
below it.

<video
  src=".github/assets/animfetch-video.mp4"
  controls muted loop playsinline width="900">
</video>

Linux only — all system data comes from `/proc`, `/sys`, and environment
variables.

## Install

### Arch Linux

```sh
paru -S animfetch-bin      # or: yay -S animfetch-bin
```

### Any Linux

```sh
curl -fsSL https://raw.githubusercontent.com/Andrew-Velox/animfetch/main/install.sh | sh
```

A static binary, so no Rust and no dependencies. Goes to `~/.local/bin`, or
`/usr/local/bin` as root; set `ANIMFETCH_BINDIR` to override. To read it before
running it, download it first — it is short.

### From source

Needs Rust 1.88 or newer.

```sh
cargo install --locked --git https://github.com/Andrew-Velox/animfetch
```

Installs to `~/.cargo/bin`. Update with `--force`, remove with
`cargo uninstall animfetch`.

## Usage

```sh
animfetch --pin      # pin above your own shell, keep animating in background
animfetch --play     # animate in place for a few seconds, then exit
animfetch --once     # print one static frame and exit
animfetch            # interactive: animfetch owns the prompt
```

`--pin` is the one you probably want. It sets a scroll region so the terminal
never scrolls the top rows, then detaches into the background and paints them
while your own shell — your prompt, history, completions, aliases — runs
underneath. Undo it with `animfetch --unpin`.


Common options:

```sh
animfetch -a cat-tail        # different animation, this run only
animfetch --style quad       # half (default), quad, or ramp
animfetch --width 40         # cap the art width (0 fills the screen)
animfetch --height 12        # both caps apply; aspect ratio is kept
animfetch --fps 20
animfetch --play -s 1.5      # shorter intro
animfetch --no-color
```

Piping the output drops to the static path automatically, as does `NO_COLOR=1`
for colour.

### In your shell startup

Use `--pin`, `--play`, or `--once`. Never the bare `animfetch` — it waits for
input and replaces your shell for as long as it runs.

Add one line to `~/.bashrc` or `~/.zshrc`:

```sh
[[ $- == *i* ]] && command -v animfetch >/dev/null && animfetch --pin
```

or to `~/.config/fish/config.fish`:

```fish
status is-interactive; and type -q animfetch; and animfetch --pin
```

The guards keep it out of scripts and `scp`, which read your startup file too
and break on unexpected output, and out of shells that cannot find the binary.

`--pin` is safe to run for every shell: a second one in a nested shell notices
the first and exits silently.

## Interactive mode keys

| Key | Action |
| --- | --- |
| `<text>` `Enter` | Run the command, output appears below the fetch |
| `Ctrl-C` | Interrupt a running command; at an idle prompt, quit |
| `Esc` | Quit |
| `Ctrl-D` | Quit, when the line is empty |
| `Ctrl-U` | Clear the line |
| `Ctrl-W` | Delete the last word |

`cd` and `exit` are handled internally; everything else runs under `$SHELL -c`.
That means no aliases, no history, and no completion — it is a launcher with a
prompt, not a shell.

## Animations

```sh
animfetch --list             # what is available (* marks the default)
animfetch -a blackhole       # use one for this run only
animfetch --set blackhole    # make it the default, saved to config.toml
```

```
$ animfetch --list
  blackhole    10 frames  built in
  cat-run       5 frames  built in
* cat-tail      8 frames  built in
  dolphin-run   9 frames  built in
  fox-run       8 frames  built in
  dog-run      12 frames  /home/you/.config/animfetch/anim/dog-run
```

### Adding your own

Drop plain-text frames into `~/.config/animfetch/anim/<name>/`, one file per
frame, named so they sort in order (`01.txt`, `02.txt`, …). No rebuild needed —
they are picked up at runtime, and a directory shadows a built-in of the same
name.

Two rules for the art:

- Any non-whitespace character counts as ink. Frames are read as a coverage
  mask, not as characters, so what you draw with does not matter.
- Author every frame on one canvas. Padding is kept as written, and that is what
  keeps poses registered against each other.

To compile one into the binary instead, put it under `assets/anim/`, add it to
the `BUNDLED` table in `src/anim.rs`, then **rebuild and reinstall**:

```sh
cargo build --release && install -Dm755 target/release/animfetch ~/.local/bin/animfetch
```

Frames are embedded at compile time, so a new animation will not appear in
`--list` until you reinstall.

## Configuration

Optional. Every key is optional too, and a malformed file falls back to defaults
rather than refusing to start.

Copy [`config.example.toml`][example] to `~/.config/animfetch/config.toml`.

[example]: https://github.com/Andrew-Velox/animfetch/blob/main/config.example.toml

### Colours from your desktop theme

Instead of fixed colours, animfetch can read whatever file themes the rest of
your desktop — matugen, pywal, wallust — so the fetch follows your wallpaper.

```sh
animfetch --once --palette      # try it without editing anything
```

To make it permanent:

```toml
color_source = "palette"
```

It looks for a matugen, pywal or wallust palette in that order, and reads the
`primary`, `secondary`, `tertiary` and `on_surface` roles by default. Any role
the file does not define keeps the fixed colour from your config, so this can
only add colour, never take it away — and a missing palette file changes
nothing at all.

Under `--pin` the file is re-read when it changes, so retheming your desktop
recolours the running animation without restarting it.

See `palette_files`, `palette_accent`, `palette_value` and `palette_gradient`
in `config.example.toml` to point it somewhere else or pick different roles.

## A note on the prompt

Interactive mode draws something that looks like a shell prompt, collects what
you type, and hands it to `$SHELL`. That is a keylogger you happen to trust —
fine on your own machine, worth being deliberate about anywhere else. The whole
path is in `src/prompt.rs` and is short enough to read end to end.
