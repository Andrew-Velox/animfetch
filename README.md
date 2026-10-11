<h1 align="center">animfetch 🧊</h1>
<div align="center">

An animated system fetch you can work inside. The art stays pinned at the top of
the screen while your prompt and command output scroll below it. Linux and
macOS.


[![version](https://img.shields.io/github/v/release/Andrew-Velox/animfetch?color=5cd5fb&style=flat-square)](https://github.com/Andrew-Velox/animfetch/releases)
[![stars](https://img.shields.io/github/stars/Andrew-Velox/animfetch?label=stars&color=5cd5fb&style=flat-square)](https://github.com/Andrew-Velox/animfetch)
[![license](https://img.shields.io/github/license/Andrew-Velox/animfetch?color=5cd5fb&style=flat-square)](LICENSE)
[![platform](https://img.shields.io/badge/platform-Linux%20%7C%20macOS-5cd5fb?style=flat-square)](https://github.com/Andrew-Velox/animfetch)
[![issues](https://img.shields.io/github/issues/Andrew-Velox/animfetch?color=5cd5fb&style=flat-square)](https://github.com/Andrew-Velox/animfetch/issues)<!-- loc-start -->
<!-- ![Lines of Rust](https://img.shields.io/badge/lines_of_Rust-4307-orange?style=flat-square) -->

<!-- loc-end -->

</div>

![animfetch running with a pinned fetch above a working shell](.github/assets/animfetch-demo.gif)

## Animations


### Non-colored
|   |   |   |   |   |
|:---:|:---:|:---:|:---:|:---:|
|cat-run|cat-tail|fox-run|dolphin-swim|blackhole|
| ![](.github/assets/cat-run.gif) | ![](.github/assets/cat-tail.gif) | ![](.github/assets/fox-run.gif) | ![](.github/assets/dolphin-swim.gif) | ![](.github/assets/blackhole.gif) |
|butterfly|icosahedron|rabbit-run|mew|yin-yang|
| ![](.github/assets/butterfly.gif) | ![](.github/assets/icosahedron.gif) | ![](.github/assets/rabbit-run.gif) | ![](.github/assets/mew.gif) | ![](.github/assets/yin-yang.gif) |
|tree|boxing|chicken-run|deer-run|horse-run|
| ![](.github/assets/tree.gif) | ![](.github/assets/boxing.gif) | ![](.github/assets/chicken-run.gif) | ![](.github/assets/deer-run.gif) | ![](.github/assets/horse-run.gif) |
|squirrel-run|earth|saturn|||
| ![](.github/assets/squirrel-run.gif) | ![](.github/assets/earth.gif) | ![](.github/assets/saturn.gif) | ![](.github/assets/rocket.gif) | ![](.github/assets/fire.gif) |
| ![](.github/assets/morty-dance.gif) | ![](.github/assets/bird.gif) |

### Coloured
<details>
<summary>Click to expand</summary>

|   |   |   |   |   |
|:---:|:---:|:---:|:---:|:---:|
|dragon|mew|fire|rocket|rabbit-sit|
| ![](.github/assets/coloured/dragon.gif) | ![](.github/assets/coloured/mew.gif) | ![](.github/assets/coloured/fire.gif) | ![](.github/assets/coloured/rocket.gif) | ![](.github/assets/coloured/rabbit-sit.gif) |
|gojo-dance|gojo-dance2|morty-dance|mew|conjito|
| ![](.github/assets/coloured/gojo-dance.gif) | ![](.github/assets/coloured/gojo-dance2.gif) | ![](.github/assets/coloured/morty-dance.gif) | ![](.github/assets/coloured/mew.gif) | ![](.github/assets/coloured/conjito.gif) |
|boxing|coelho|
| ![](.github/assets/coloured/boxing.gif) | ![](.github/assets/coloured/coelho.gif) |

</details>


Pick one with `-a <name>`, or set the default with `--set <name>`.

## Install

Arch Linux:

```sh
paru -S animfetch-bin      # or animfetch-git to build from source
```
Nix / NixOS:

Run without installing:

```sh
nix run github:Andrew-Velox/animfetch
```

Or add animfetch to your own `flake.nix`:

```nix
{
  inputs.animfetch.url = "github:Andrew-Velox/animfetch";
}
```

Then add it to your NixOS packages:

```nix
environment.systemPackages = [
  inputs.animfetch.packages.${pkgs.stdenv.hostPlatform.system}.default
];
```

or with Home Manager:

```nix
home.packages = [
  inputs.animfetch.packages.${pkgs.stdenv.hostPlatform.system}.default
];
```

Any Linux or macOS, prebuilt binary, no Rust needed:

```sh
curl -fsSL https://raw.githubusercontent.com/Andrew-Velox/animfetch/main/install.sh | sh
```

Goes to `~/.local/bin`, or `/usr/local/bin` as root; `ANIMFETCH_BINDIR`
overrides. Tarballs are on the [releases page][releases]. macOS support is
new; if something looks off there, please open an issue.

From source, with Rust 1.88 or newer:

```sh
cargo install --locked --git https://github.com/Andrew-Velox/animfetch
```

[releases]: https://github.com/Andrew-Velox/animfetch/releases/latest

## Usage

```sh
animfetch --pin      # pin above your own shell, keep animating in background
animfetch --play     # animate in place for a few seconds, then exit
animfetch --once     # print one static frame and exit
animfetch            # interactive: animfetch owns the prompt
```

`--pin` is the one you want. It sets a scroll region so the top rows never
scroll, then detaches and paints them while your own shell runs underneath with
its history, completion and aliases intact. Undo with `animfetch --unpin`.

A terminal holds one pin. A bare `animfetch --pin`, which is what a shell's
startup file runs, leaves a pin that is already there alone, so nested shells
stay quiet. `--pin` with settings (`animfetch --pin --no-color`) replaces it,
so a line you type takes effect at once. A run with `--save` does not: it
writes the settings, says so, and leaves this terminal as it is; terminals you
open after that start with them.

```sh
animfetch -a cat-tail        # different animation, this run only
animfetch --style quad       # half (default), quad, ramp, or raw
animfetch --width 40         # cap the art width (0 fills the screen)
animfetch --height 12        # both caps apply, aspect ratio is kept
animfetch --fps 20
animfetch --play -s 1.5      # shorter intro
animfetch --no-color         # everything in shades of grey
animfetch --def-color        # the default colours, from your theme: undoes --no-color
```

Piping the output, or `NO_COLOR=1`, drops to the static path automatically.

`--style` picks how the art is drawn. `half` and `quad` render it as solid
blocks and rescale to any terminal. `ramp` maps coverage onto the characters in
`ramp`. `raw` prints the art's own characters untouched, which suits hand-drawn
and pre-coloured ASCII like `earth`; frames may carry their own truecolor
escapes, and `--art-color own|theme` picks between those and the theme gradient.

### In your shell startup

`~/.bashrc` or `~/.zshrc`:

```sh
[[ $- == *i* ]] && command -v animfetch >/dev/null && animfetch --pin
```

`~/.config/fish/config.fish`:

```fish
status is-interactive; and type -q animfetch; and animfetch --pin
```

The guards keep it out of scripts and `scp`, which break on unexpected output.
Never use bare `animfetch` here, since it waits for input.

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
No aliases, no history, no completion: a launcher with a prompt, not a shell.


## Borders

An ornament can run down the left of the data (between the art and the info)
and another along its bottom, drawn from PNG line art:

```sh
animfetch --pin --border-left 00 --border-bottom 03
animfetch --pin --lb 00 --db 03   # the same, shorter: --lb left, --db down
animfetch --list-borders          # what is built in, and your own
animfetch --pin --no-border       # for this run only
```
```toml
border_left   = "00"      # a built-in name, one of your own, or a path to a PNG
border_bottom = "03"
border_render = "auto"    # auto | image | cells
border_bottom_lift = 2.15 # raise the bottom ornament toward the info, in rows
border_bottom_shift = 12.0 # move the bottom ornament to the right, in columns
```

The images are drawn as the pictures they are, using the kitty graphics
protocol, which kitty, WezTerm and Ghostty speak: over the cells and under the
text, scaled to a whole number of cells so nothing is stretched. Terminals that
do not speak it (VS Code's included) get **no border**; nothing is converted to
text unless you ask. `animfetch --list-borders` begins by saying which kind of
terminal you are in.

`border_render = "cells"` is that request: it redraws the image as braille
characters, which is coarse and works anywhere with colour. `"image"` forces
pictures even where they are not detected.

A bottom image is stored upright, hilt at the top, and is turned a quarter
clockwise so it lies along the bottom. The art and the info give up the room a
border takes, and it is left out on a terminal under 60 columns and when the
output is not a terminal. `--no-color` draws the pictures in grey, like the
rest. `NO_COLOR` and `color = false` leave the pictures as they are, which have
no colour to turn off, and remove the `"cells"` drawing, which is coloured
text.

Put your own PNGs in `~/.config/animfetch/border/left_border/` and
`.../bottom_border/`; a file of yours shadows a built-in of the same name. They
should be mostly transparent line art in light strokes, drawn for a dark
terminal, as the built-in ones are. The built-in images add about 950 KiB to the
binary; `cargo build --no-default-features` leaves them out, and borders still
work from your own files.

The protocol is not passed through by tmux or screen, so inside them there is
no border either. To check a terminal, `kitten icat assets/border/00.png` in
kitty should show the image, and `animfetch --list-borders` should say it shows
pictures.

### The look with a border

`minimal` is made to sit beside a border ornament. For the whole look, turn the
animation off so the ornament and the list are the pane:

```sh
animfetch --pin --info minimal --no-art --border-left 00 --border-bottom 00
```
```toml
info_style    = "minimal"
art           = false
border_left   = "00"
border_bottom = "00"
```

Its values are the fuller ones (`zsh 5.9.2`, `kitty 0.48.2`, `mango (Wayland)`),
which cost the same few milliseconds as `boxed`. A terminal's version comes from
its environment where it sets one, and from `--version` for kitty, alacritty and
foot, which are the ones known to answer.

## Configuration

Optional. Copy [`config.example.toml`][example] to
`~/.config/animfetch/config.toml`. Every key is optional, and a malformed file
falls back to defaults.

[example]: https://github.com/Andrew-Velox/animfetch/blob/main/config.example.toml

### Info styles

Eight layouts for the info pane, picked with `info_style` or `--info`:

| Style | Looks like |
| --- | --- |
| `classic` (default) | `user@host`, a rule, then `Label: value` rows |
| `boxed` | icons and short labels in a rounded box, values beside it |
| `tree` | a user box over OS and PC groups: a tree of icons beside `: value` lines |
| `sections` | Hardware, Software and Uptime / Age blocks under titled rules, then the colours as dots |
| `chips` | SYSTEM, SOFTWARE and HARDWARE as filled, rounded pills with two-letter keys |
| `iconic` | a bare coloured icon per row, no labels, under pill headings, then outline shapes |
| `card` | one rounded box, labels at the left edge and values at the right, then your own line of text |
| `minimal` | an icon and a value per row, nothing else: the plain list that sits beside a border |

`boxed` also shows fuller values (kernel name, terminal, shell and window
manager versions, the filesystem under `disk`), colours usage percentages by how
full they are, and adds the `gpu` and `os_age` rows. `tree` draws a fixed set of
rows (os, shell, terminal, host, gpu, memory, display) and leaves other modules
out. `sections` draws its own fixed three blocks, with the CPU and GPU as bare
model names, every local disk on its own line (`disks`), and a line per battery
(`battery`, none on a desktop). Its section keys take the far end of the
gradient as their colour, so they follow a palette too.

`chips` and `iconic` read nothing extra: their values are short and bare
(`Arch Linux`, `1h 40m`, `AMD Ryzen 5 5600X`), so they cost no more than
`classic`. `iconic` colours its icons along your `gradient` and fills its
headings from it, shows only the user's name (no host), and says uptime to the
second. Its `temperature` row needs a hwmon chip that reports the CPU
(`k10temp`, `coretemp`); without one the row is skipped.

`card` lists rows in the order of `modules`, so it is the style you shape
yourself. Memory is `Ram` and each disk is named for where it is mounted. Two
config keys feed it: `custom` (rows of your own text, placed where `custom`
stands in `modules`) and `footer` (a line under the rows). Both are plain
config, so they cost nothing:

```toml
info_style = "card"
footer = "we are free!"
custom = [{ label = "Owner", value = "POKKITA" }]
```

`chips` fills its pills with your `value` colour darkened and its headings with
your `accent`, so it follows a palette like everything else. The rounded ends
are Powerline glyphs and need both `icons` and colour; without either the
chips are square. Its `font` row comes from GTK's `settings.ini`, so a machine
with no GTK settings file shows none.

The `display` row reads each screen's current mode from the kernel, so it needs
access to `/dev/dri`, which a plain SSH session lacks; the row is skipped there.

Icons are Nerd Font glyphs; set `icons = false` if you see empty boxes. The
versions of the shell and window manager come from running them, about 7 ms on a
typical machine, which `classic` never spends. Leave `modules` unset to get each
style's own rows.

Set `live = true` (or pass `--live`) and the uptime, memory, swap and disk
rows keep updating while the fetch is pinned, every `live_interval` seconds
(default 2). It is off by default, only touches those four rows, and pauses
while a full-screen program like `vim` has the terminal.

Set `color_source = "palette"` and the fetch follows your desktop theme, reading
a matugen, pywal or wallust palette. Under `--pin` it re-reads on change, so
retheming recolours the running animation. Try it with `animfetch --once
--palette`.

### Renaming and replacing rows

Any built-in row can be given a name of your own, or text or a command in place
of what the machine says, under `[overrides.<module>]`. The names are the ones
`modules` takes:

```toml
modules = ["os", "kernel", "uptime", "custom", "shell"]

[overrides.os]
label = "Distro"                  # the name shown for the row

[overrides.kernel]
value = "my-own-kernel 1.0"       # shown instead of what the machine reports

[overrides.uptime]
label = "Up"
command = "uptime -p"             # the first line this prints is the value

[[custom]]                        # a row of your own, from text or a command
label = "Public IP"
command = "curl -s --max-time 1 ifconfig.me"

[[custom]]
label = "Editor"
value = "helix"
```

- `label`, `value` and `command` are each optional, but an override has to say
  at least one, and `value` and `command` are alternatives.
- A rename is only the name. The row keeps its icon, its place and its live
  updates, and the styles that print no names (`iconic`, `tree`, `sections`
  headings) are unchanged by it.
- A `value` or `command` is shown as a plain line in every style, so the usage
  bars and figures a style would draw for a memory or disk row are replaced by
  your text. The row is not refreshed by `live`, since it is not the machine's
  reading any more. A module the machine cannot read (a `battery` on a desktop)
  still shows your text, since you wrote the row.
- A command runs with `sh -c`, with your privileges, like anything in your
  shell's startup files; nothing in animfetch's own files runs one. Only the
  first non-empty line is used, with control characters removed, and its
  exit status is not asked about. Commands all start together, so the wait is
  the slowest, and one that prints nothing or takes longer than
  `command_timeout` seconds (default 1, at most 10) leaves its row out. Each
  runs once, at startup.
- A mistake (an unknown module, a `value` with a `command`, an empty table) is
  reported with its line number and the config is not used, like any other
  config error.


## Stargazers ⭐

![Stargazers](.github/assets/stargazers.svg)


## Credits

Some animations are redrawn as ASCII from [RunCat][runcat] by
[kyome22][kyome], used under the Apache License 2.0.

[runcat]: https://github.com/kyome22/menubar_runcat
[kyome]: https://github.com/kyome22


<hr>

