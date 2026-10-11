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

**[Documentation](https://andrew-velox.github.io/animfetch-cli/)** ·
[Animations](https://andrew-velox.github.io/animfetch-cli/animations/) ·
[Releases](https://github.com/Andrew-Velox/animfetch/releases/latest)

</div>

![animfetch running with a pinned fetch above a working shell](.github/assets/animfetch-demo.gif)

## Install

```sh
# Any Linux or macOS, prebuilt binary, no Rust needed
curl -fsSL https://raw.githubusercontent.com/Andrew-Velox/animfetch/main/install.sh | sh

# Arch Linux
paru -S animfetch-bin

# Nix: run it without installing
nix run github:Andrew-Velox/animfetch

# From source, with Rust 1.88 or newer
cargo install --locked --git https://github.com/Andrew-Velox/animfetch
```

NixOS and Home Manager setups, install paths and the other options are in
[Installation](https://andrew-velox.github.io/animfetch-cli/docs/installation/).

## Use it

```sh
animfetch --pin          # pin above your own shell and keep animating
animfetch --unpin        # put the terminal back
animfetch -a dragon      # another animation, this run only (-l lists them)
animfetch --no-color     # everything in shades of grey
```

To have it in every new terminal, put this in `~/.bashrc` or `~/.zshrc`:

```sh
[[ $- == *i* ]] && command -v animfetch >/dev/null && animfetch --pin
```

Add `--save` to keep options for every later run: `animfetch --no-color --save`
saves and says so, and with `--pin` it saves and pins. A config file is
optional: copy [`config.example.toml`](config.example.toml) to
`~/.config/animfetch/config.toml`.

## Documentation

Everything else is on the site.

| | |
| --- | --- |
| [Quick start](https://andrew-velox.github.io/animfetch-cli/docs/quick-start/) | the first five minutes |
| [Modes](https://andrew-velox.github.io/animfetch-cli/docs/modes/) | pinned, played, once and interactive |
| [Info styles](https://andrew-velox.github.io/animfetch-cli/docs/info-styles/) | the eight layouts for the info pane |
| [Animations](https://andrew-velox.github.io/animfetch-cli/docs/animations/) | the gallery and how to add your own |
| [Borders](https://andrew-velox.github.io/animfetch-cli/docs/borders/) | picture ornaments beside the info |
| [Theming](https://andrew-velox.github.io/animfetch-cli/docs/theming/) | colours, gradients, grey, and following your desktop theme |
| [Configuration](https://andrew-velox.github.io/animfetch-cli/docs/configuration/) | every key |
| [Overrides](https://andrew-velox.github.io/animfetch-cli/docs/overrides/) | rename a row, or fill one from a command |
| [CLI reference](https://andrew-velox.github.io/animfetch-cli/docs/cli-reference/) | every flag and environment variable |
| [Troubleshooting](https://andrew-velox.github.io/animfetch-cli/docs/troubleshooting/) | empty boxes, no borders, odd colours |

## Stargazers ⭐

![Stargazers](.github/assets/stargazers.svg)

## Credits

Some animations are redrawn as ASCII from [RunCat][runcat] by
[kyome22][kyome], used under the Apache License 2.0.

[runcat]: https://github.com/kyome22/menubar_runcat
[kyome]: https://github.com/kyome22
