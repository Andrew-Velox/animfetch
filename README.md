# animfetch

An animated system fetch you can work inside. The fetch stays pinned at the top
of the screen and keeps animating; your prompt and everything you run scroll in
the region beneath it.

```
                                     ▒█▓░ ▒█░
                                    ░█████████▓       mohabbat@mohabbat
                                   ▒███████████▒      ─────────────────
                                  ▓████████████▒      OS: Arch Linux x86_64
                                 ░██████████████▓     Host: B650M K
                                ▓████████████████▒    Kernel: 7.0.14-arch1-1
                       ▓█████████████████████████▒    Uptime: 2 hours, 28 mins
   ░▒▒▓██████████████████████████████████████████     Packages: 1100 (pacman)
▒▓█████████████████████████████████████████████▓░     Shell: bash
█████████████████████████████████████████████▓        WM: Hyprland
 ░▒▒         ░▒████████████████████████████████▒      Terminal: kitty
          ░▒████████████████████████████████████▓     CPU: AMD Ryzen 7 7700 8-Core Processor (16)…
         ▓██████████████████████▓▒▒  ▒▓████████▓▒     Memory: 9.07GiB / 15GiB (61%)
       ▒████████████████▓▒▒▒▒▒▒        ░▒▒▒▒▒▒▒       Swap: 66MiB / 4.00GiB (1%)
       ▒██████████████▓▒                              Disk: 180GiB / 456GiB (39%)
        ▒▒▒▒▒▓██████▓                                 ████████████████████████
             ▒█████░
              ░▒▒

mohabbat@mohabbat ~/Projects/rust/animfetch $ █
─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─
mohabbat@archlinux ~/Projects/rust/animfetch $ ls
assets  Cargo.toml  config.example.toml  README.md  src  target
mohabbat@archlinux ~/Projects/rust/animfetch $ cd /etc
mohabbat@archlinux /etc $ █              <- scrolls freely below
```

## How it works

**The pinning is the terminal's own doing.** Setting a scroll region (DECSTBM)
tells the terminal never to scroll the top rows. Command output can then pour
into the lower part of the screen indefinitely without us redrawing anything —
we only repaint the top rows to advance the animation.

**The art scales to fit.** Frames are stored as an ink-coverage mask rather
than as characters, so they resample to any size. `width` and `height` define a
box, and the art is fitted inside it with its aspect ratio kept — so a tall
animation and a wide one still come out looking the same size.

**Raw mode with a non-blocking event poll** means the animation and the prompt
are not competing for the loop. Each pass draws a frame, then waits for input
*only until the next frame is due*, so keystrokes are handled the moment they
arrive and the frame clock never drifts.

Details that matter for how it feels:

- **Frame timing is absolute, not accumulated.** The next frame's deadline comes
  from a fixed schedule rather than from sleeping a fixed amount, so rendering
  cost cannot make the animation run slow.
- **Only changed lines are repainted**, and each frame is one atomic write
  wrapped in save/restore-cursor. The animation therefore cannot disturb the
  cursor you are typing at, and cannot tear.
- **Commands run in their own process group.** Ctrl-C reaches the command rather
  than animfetch, the same way it works under a real shell.

## Building

```sh
cargo build --release
install -Dm755 target/release/animfetch ~/.local/bin/animfetch
```

Linux only. All system data comes from `/proc`, `/sys`, and environment
variables — no subprocesses — which is what keeps it fast enough to sit in a
shell startup file. Porting means reimplementing `src/fetch.rs`; nothing else
knows where the numbers come from.

## Usage

There are three modes:

```
animfetch                    # interactive: fetch pinned above, prompt below
animfetch --play             # animate in place for a few seconds, then exit
animfetch --once             # print one static frame and exit
```

`--play` is the one for a shell startup file. It animates where the cursor
already is and then hands control back, so your own prompt follows underneath.

```
animfetch -a cat-tail        # different animation, this run only
animfetch --width 40         # smaller art (0 fills the screen)
animfetch --height 12        # both caps apply; aspect ratio is kept
animfetch --play -s 1.5      # shorter intro
```

### Choosing an animation

```sh
animfetch --list             # what is available (* marks the current default)
animfetch -a cat-tail        # use one for this run only
animfetch --set cat-tail     # make it the default, saved to config.toml
```

```
$ animfetch --list
* cat-run    5 frames  built in
  cat-tail   8 frames  built in
  fox-run   12 frames  /home/you/.config/animfetch/anim/fox-run
```

`--set` rewrites only the `animation` line in your config, leaving comments and
every other key alone, and refuses a name that would not load. A name that
resolves to nothing is always an error rather than a silent fall back to the
built-in art — with several animations installed, a typo is otherwise easy to
miss.

| Key | Action |
| --- | --- |
| `<text>` `Enter` | Run the command, output appears below the fetch |
| `Ctrl-C` | Interrupt a running command; at an idle prompt, quit |
| `Esc` | Quit |
| `Ctrl-D` | Quit, when the line is empty |
| `Ctrl-U` | Clear the line |
| `Ctrl-W` | Delete the last word |

`cd` and `exit` are handled internally. `cd` has to be: a child process that
changes directory and exits accomplishes nothing, so every shell wrapper
implements it itself. Everything else goes to `$SHELL -c`.

Piping the output (`animfetch | less`) drops into the static path automatically,
as does `NO_COLOR=1` for colour.

### In your shell startup

Use `--play` (or `--once` for no animation). The default mode waits for input,
which is not what you want on every new terminal:

```sh
animfetch --play
```

`--play` deliberately never enters raw mode, so anything you type during the
intro stays in the terminal's line buffer and arrives at your real prompt
rather than being swallowed. It also never clears the screen, so scrollback
above it survives.

Note that it does block the shell for its duration — `--seconds` or the
`play_seconds` config key controls that, and `--once` skips it entirely.

## Configuration

Copy `config.example.toml` to `~/.config/animfetch/config.toml`. Every key is
optional; a malformed file falls back to defaults and reports the problem
afterwards rather than refusing to start.

### Custom animations

Two animations ship in the binary: `cat-run` and `cat-tail`.

For your own, drop plain-text frames into `~/.config/animfetch/anim/<name>/`,
one file per frame, named so they sort in order (`01.txt`, `02.txt`, …). Then:

```sh
animfetch --list
animfetch -a <name>
```

A directory shadows a built-in animation of the same name, so `cat-run` can be
replaced without touching the binary. To compile a new one in instead, put it
under `assets/anim/` and add it to the `BUNDLED` table in `src/anim.rs`.

Two things to know:

- **Any non-whitespace character counts as ink.** Frames are read as a coverage
  mask, not as characters, so the glyphs you draw with do not matter — the
  `ramp` setting decides what gets printed. This is what lets a frame scale to
  any terminal size without losing thin strokes.
- **Author every frame on one canvas.** Padding is preserved as written, so
  leading blank lines are what keep poses registered against each other. A
  directory named `cat-run` overrides the built-in animation, and any other
  name must exist on disk.

## Limitations

It is a launcher with a prompt, not a shell. Specifically:

- **No aliases.** Commands run under `$SHELL -c`, which does not read your
  interactive startup files. Using `-i` would fix aliases and reintroduce the
  problem those files cause — every command would re-run whatever your `.bashrc`
  prints. Functions and `PATH` are unaffected, since the environment is
  inherited.
- **No history, completion, or line editing** beyond backspace and the two
  control keys above.
- **Full-screen programs** (`vim`, `less`, `htop`) work, because they use the
  alternate screen and restore it on exit. A program that clears the screen
  *without* the alternate screen will wipe the fetch until the next repaint.

## A note on the prompt

This tool draws something that looks like a shell prompt, collects what you
type, and hands it to `$SHELL`. That is a keylogger you happen to trust, which
is fine on your own machine and worth being deliberate about anywhere else.

The entire path lives in `src/prompt.rs` and is short enough to read end to end.
No config value and no animation file reaches the exec call; the only input is
your own keystrokes. Keep it that way if you extend this.

## Layout

```
src/
  main.rs      wiring, argument parsing, the shell loop and screen split
  term.rs      raw mode and scroll region ownership, and their restoration
  anim.rs      frame loading and coverage-based scaling
  render.rs    composition and the diffing, cursor-preserving painter
  fetch.rs     system information (Linux)
  prompt.rs    input handling, builtins, and shell execution
  config.rs    config file and defaults
  color.rs     truecolor and gradients
assets/anim/*/            built-in animations, embedded at compile time
```
