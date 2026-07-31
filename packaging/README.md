# Packaging

How animfetch gets to people who do not have a Rust toolchain. Nothing in here
is needed to build or run it; see the main README for that.

The three pieces fit together in one direction:

```
git tag v0.1.0  ->  release.yml  ->  GitHub Release with static binaries
                                          |                    |
                                     install.sh          animfetch-bin (AUR)
```

Everything downstream reads from the Release, so cutting one is always the first
step.

## Before the first release

Two things are missing, and both block the AUR:

- **A LICENSE file.** A repository without one is "all rights reserved" by
  default, which means nobody can legally redistribute it, and Arch requires
  the licence text to be installed with the package. Pick a licence, commit it
  as `LICENSE`, set `license=()` in both PKGBUILDs to match, and uncomment the
  `install -Dm644 LICENSE` line in each `package()`.
- **Checksums in `animfetch-bin`.** `sha256sums_*` are `SKIP` because there is
  nothing to hash yet. Run `updpkgsums` once the release exists.

## Cutting a release

```sh
# 1. Bump the version in Cargo.toml, then refresh the lockfile.
cargo build --release

# 2. Commit, tag, push. The tag is what triggers release.yml.
git commit -am 'release v0.1.0'
git tag v0.1.0
git push origin main v0.1.0
```

`release.yml` then builds `x86_64` and `aarch64` static binaries on native
runners, checks each is really static, smoke-tests it, and attaches the
tarballs plus `SHA256SUMS` to a GitHub Release.

Check it worked:

```sh
curl -fsSL https://raw.githubusercontent.com/Andrew-Velox/animfetch/main/install.sh | sh
```

## Publishing to the AUR

Only you can do this, since it needs your AUR account and SSH key. Once per
package:

```sh
# Register the SSH key you use for the AUR at https://aur.archlinux.org/
git clone ssh://aur@aur.archlinux.org/animfetch-bin.git
cd animfetch-bin
cp /path/to/animfetch/packaging/aur/animfetch-bin/PKGBUILD .

updpkgsums                          # pin the real checksums
makepkg -si                         # confirm it builds and installs
namcap PKGBUILD *.pkg.tar.zst       # must be clean before pushing

makepkg --printsrcinfo > .SRCINFO   # the AUR reads this, not the PKGBUILD
git add PKGBUILD .SRCINFO
git commit -m 'initial release'
git push
```

`animfetch-git` is the same flow with no `updpkgsums` step, and it works without
a release existing, so it is worth putting up first if you want something
installable today.

For each later version: bump `pkgver`, `updpkgsums`, regenerate `.SRCINFO`,
commit, push.

## Known namcap output

- `Reference to x86_64 should be changed to $CARCH` on `animfetch-bin` is a
  false positive. `source_x86_64` arrays have to name the architecture
  literally; that is how makepkg selects between them.
- `Dependency included, but may not be needed ('gcc-libs')` on `animfetch-git`
  contradicts the `libgcc` line above it. `gcc-libs` is what provides `libgcc`,
  and listing it is what Arch's own Rust packages do.
