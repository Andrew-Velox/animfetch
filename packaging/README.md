# Releasing a new version

The whole pipeline in one direction:

```
git tag vX.Y.Z  ->  release.yml  ->  GitHub Release, four tarballs + SHA256SUMS
                                          |                    |
                                     install.sh          animfetch-bin (AUR)
```

Everything downstream reads from the Release, so nothing on the AUR can be
updated until the Release exists. `animfetch-git` builds from HEAD and never
needs any of this.

## 1. Bump the version

In the animfetch repo, with a clean tree:

```sh
git pull        # the mural and LOC workflows commit to main on their own
```

Edit `Cargo.toml`: `version = "X.Y.Z"`. Then in
`packaging/aur/animfetch-bin/PKGBUILD`: set `pkgver=X.Y.Z` and reset both
`sha256sums_*` lines to `'SKIP'`, because the old sums describe the previous
release's tarballs.

```sh
cargo build --release       # refreshes Cargo.lock to the new version
cargo test --release
git add -A
git commit -m 'release: vX.Y.Z'
git push
```

## 2. Wait for CI, then tag

Do not tag in the same breath as the push. Wait for CI to go green first,
especially the macos job, so a broken commit never becomes a release. Then:

```sh
git pull        # the bots may have moved main again while CI ran
git tag vX.Y.Z && git push origin vX.Y.Z
```

Pulling before tagging matters: a tag pointing at a commit that is not on
origin makes the next `git push` fail with non-fast-forward, and untangling
that after the tag is public means merging rather than rebasing.

The tag push triggers `release.yml`: two Linux musl builds, two macOS builds,
smoke tests, tarballs, `SHA256SUMS`, and a changelog generated from the
conventional-commit prefixes since the previous tag. Commits without a
`feat:`/`fix:`/`docs:` style prefix are left out of the changelog.

## 3. Check the release

```sh
curl -fsSL https://github.com/Andrew-Velox/animfetch/releases/download/vX.Y.Z/SHA256SUMS
```

Four lines means all four targets published. The real end-to-end check:

```sh
curl -fsSL https://raw.githubusercontent.com/Andrew-Velox/animfetch/main/install.sh | sh
animfetch --version
```

## 4. Update animfetch-bin on the AUR

Needs `pacman-contrib` installed (for `updpkgsums`). In the AUR clone
(`ssh://aur@aur.archlinux.org/animfetch-bin.git`):

```sh
cd ~/Projects/rust/animfetch-bin
git pull
cp ~/Projects/rust/animfetch/packaging/aur/animfetch-bin/PKGBUILD .
updpkgsums
makepkg --printsrcinfo > .SRCINFO
makepkg -f                          # proves it builds from the real release
namcap PKGBUILD *.pkg.tar.zst       # see known output below
rm -rf src pkg *.pkg.tar.zst animfetch-*.tar.gz
git add PKGBUILD .SRCINFO
git commit -m 'update to X.Y.Z'
git push
```

Rules learned the hard way:

- **`.SRCINFO` must be in the commit.** The AUR reads only `.SRCINFO`; a push
  without it succeeds but the site keeps showing the old version, with a
  `warning: .SRCINFO unchanged` you can easily miss in the push output.
- **Run the commands one at a time, not as one `&&` chain.** A missing
  `updpkgsums` once broke the chain silently and a PKGBUILD with `SKIP`
  checksums went out.
- **Never push `SKIP` checksums.** They disable download verification for
  everyone who installs the package.
- **Delete the downloaded tarballs before committing.** `makepkg` leaves them
  in the clone and they must not go into the AUR repo.

If `updpkgsums` is unavailable, pin by hand: download `SHA256SUMS` from the
release, copy the two `*-unknown-linux-musl` hashes into `sha256sums_x86_64`
and `sha256sums_aarch64`, and verify with `makepkg -f` before pushing.

## 5. Sync back and verify

Copy the pinned PKGBUILD back so the repo copy matches what the AUR serves:

```sh
cd ~/Projects/rust/animfetch
cp ~/Projects/rust/animfetch-bin/PKGBUILD packaging/aur/animfetch-bin/
git commit -am 'packaging: pin vX.Y.Z checksums'
git push
```

The AUR web page and RPC index lag a few minutes behind a push. Check what is
actually served before assuming a problem:

```sh
curl -s 'https://aur.archlinux.org/cgit/aur.git/plain/.SRCINFO?h=animfetch-bin' | grep pkgver
```

Installing through paru inside that lag window fetches the previous version;
`paru -Sy animfetch-bin` after ten minutes or so gets the new one.

## animfetch-git

Nothing to do per release: it builds whatever HEAD is. Its `pkgver` in
`.SRCINFO` is a placeholder (`0.1.0.r0.g0000000`) that `pkgver()` replaces at
build time. That is normal for VCS packages, but AUR helpers use the placeholder
to detect updates, so it never looks updated on the site. Refreshing it
occasionally is cosmetic: run `makepkg` in the clone, regenerate `.SRCINFO`,
push.

## Known namcap output

- `Reference to x86_64 should be changed to $CARCH` on `animfetch-bin` is a
  false positive. `source_x86_64` arrays have to name the architecture
  literally; that is how makepkg selects between them.
- `Dependency included, but may not be needed ('gcc-libs')` on `animfetch-git`
  contradicts the `libgcc` line above it. `gcc-libs` is what provides `libgcc`,
  and listing it is what Arch's own Rust packages do.
