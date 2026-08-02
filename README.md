# animfetch

An animated system fetch you can work inside. The fetch stays pinned at the top
of the screen and keeps animating while your prompt and command output scroll
below it.

![animfetch running with a pinned fetch above a working shell](.github/assets/animfetch-demo.gif)

Linux only. All system data comes from `/proc`, `/sys`, and environment
variables.

## animations

|   |   |   |   |   |
|:---:|:---:|:---:|:---:|:---:|
|cat-run|cat-tail|fox-run|dolphin-run|blackhole|
| ![](.github/assets/cat-run.gif) | ![](.github/assets/cat-tail.gif) | ![](.github/assets/fox-run.gif) | ![](.github/assets/dolphin-run.gif) | ![](.github/assets/blackhole.gif) |
|butterfly|icosahedron|rabbit-run|mew|yin-yang|
| ![](.github/assets/butterfly.gif) | ![](.github/assets/icosahedron.gif) | ![](.github/assets/rabbit-run.gif) | ![](.github/assets/mew.gif) | ![](.github/assets/yin-yang.gif) |

Pick one with `-a <name>`, or make it the default with `--set <name>`.

## Install

### Arch Linux

```sh
paru -S animfetch-bin      # or: yay -S animfetch-bin
```

The prebuilt binary. `animfetch-git` builds the latest commit from source instead.

### Any Linux

```sh
curl -fsSL https://raw.githubusercontent.com/Andrew-Velox/animfetch/main/install.sh | sh
```

Static binary, so no Rust and no dependencies. It goes to `~/.local/bin`, or
`/usr/local/bin` as root. Set `ANIMFETCH_BINDIR` to override. If you'd rather
read the script before running it, download it first. It's short.

You can also grab a tarball from the [latest release][releases] and put the
binary wherever you like.

### From source

Needs Rust 1.88 or newer.

```sh
cargo install --locked --git https://github.com/Andrew-Velox/animfetch
```

Installs to `~/.cargo/bin`. Update with `--force`, remove with
`cargo uninstall animfetch`.

[releases]: https://github.com/Andrew-Velox/animfetch/releases/latest

## Usage

```sh
animfetch --pin      # pin above your own shell, keep animating in background
animfetch --play     # animate in place for a few seconds, then exit
animfetch --once     # print one static frame and exit
animfetch            # interactive: animfetch owns the prompt
```

`--pin` is the one you probably want. It sets a scroll region so the terminal
never scrolls the top rows, then detaches into the background and paints them.
Your own shell runs underneath, with its prompt, history, completions and
aliases all intact. Undo it with `animfetch --unpin`.

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

Use `--pin`, `--play`, or `--once`. Never the bare `animfetch`, which waits for
input and replaces your shell for as long as it runs.

Add one line to `~/.bashrc` or `~/.zshrc`:

```sh
[[ $- == *i* ]] && command -v animfetch >/dev/null && animfetch --pin
```

or to `~/.config/fish/config.fish`:

```fish
status is-interactive; and type -q animfetch; and animfetch --pin
```

Both guards are worth keeping. Scripts and `scp` read your startup file too, and
they break on unexpected output. The second one stops a shell that can't find
the binary from printing an error on every new terminal.

`--pin` is safe to run for every shell. A second one in a nested shell notices
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

`cd` and `exit` are handled internally, everything else runs under `$SHELL -c`.
So no aliases, no history, no completion. It's a launcher with a prompt, not a
shell.

## Animations

```sh
animfetch --list             # what is available (* marks the default)
animfetch -a blackhole       # use one for this run only
animfetch --set blackhole    # make it the default, saved to config.toml
```

```
$ animfetch --list
  blackhole     9 frames  built in
  butterfly    16 frames  built in
* cat-run       5 frames  built in
  cat-tail      8 frames  built in
  dolphin-run   9 frames  built in
  fox-run       8 frames  built in
  icosahedron  12 frames  built in
  mew           8 frames  built in
  rabbit-run    5 frames  built in
  yin-yang      9 frames  built in
  dog-run      12 frames  /home/you/.config/animfetch/anim/dog-run
```

### Adding your own

Drop plain-text frames into `~/.config/animfetch/anim/<name>/`, one file per
frame, named so they sort in order (`01.txt`, `02.txt`, `03.txt`). No rebuild
needed. They're picked up at runtime, and a directory shadows a built-in of the
same name.

Three rules for the art:

- Any non-whitespace character counts as ink. Frames are read as a coverage
  mask, not as characters, so what you draw with doesn't matter.
- Author every frame on one canvas. Padding is kept as written, and that's what
  keeps poses registered against each other.
- Draw silhouettes, not shading. Art converted from an image uses characters as
  a brightness ramp, and since every one of them is ink, the whole thing comes
  out as a solid blob. Detail has to be whitespace: an enclosed gap of two cells
  or more survives scaling, which is how eyes stay eyes.

To compile one into the binary instead, put it under `assets/anim/`, add it to
the `BUNDLED` table in `src/anim.rs`, then **rebuild and reinstall**:

```sh
cargo build --release && install -Dm755 target/release/animfetch ~/.local/bin/animfetch
```

Frames are embedded at compile time, so a new animation won't show up in
`--list` until you reinstall.

## Configuration

Optional, and every key inside it is optional too. A malformed file falls back
to defaults instead of refusing to start.

Copy [`config.example.toml`][example] to `~/.config/animfetch/config.toml`.

[example]: https://github.com/Andrew-Velox/animfetch/blob/main/config.example.toml

### Colours from your desktop theme

Instead of fixed colours, animfetch can read whatever file themes the rest of
your desktop (matugen, pywal, wallust) so the fetch follows your wallpaper.

```sh
animfetch --once --palette      # try it without editing anything
```

To make it permanent:

```toml
color_source = "palette"
```

It looks for a matugen, pywal or wallust palette in that order, and reads the
`primary`, `secondary`, `tertiary` and `on_surface` roles by default. Any role
the file doesn't define keeps the fixed colour from your config, so this can
only add colour, never take it away. A missing palette file changes nothing at
all.

Under `--pin` the file is re-read when it changes, so retheming your desktop
recolours the running animation without restarting it.

See `palette_files`, `palette_accent`, `palette_value` and `palette_gradient`
in `config.example.toml` to point it somewhere else or pick different roles.

## A note on the prompt

Interactive mode draws something that looks like a shell prompt, collects what
you type, and hands it to `$SHELL`. That's a keylogger you happen to trust.
Fine on your own machine, worth thinking about anywhere else. The whole path is
in `src/prompt.rs`, short enough to read end to end.
