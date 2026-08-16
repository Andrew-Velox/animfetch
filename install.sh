#!/bin/sh
# Install animfetch from a GitHub Release.
#
# Downloads a prebuilt binary, so this needs no Rust toolchain and no
# compiler. Linux and macOS.
#
#   curl -fsSL https://raw.githubusercontent.com/Andrew-Velox/animfetch/main/install.sh | sh
#
# Environment:
#   ANIMFETCH_VERSION   tag to install, e.g. v0.1.0 (default: latest release)
#   ANIMFETCH_BINDIR    where to put the binary (default: ~/.local/bin, or
#                       /usr/local/bin when running as root)

set -eu

REPO=Andrew-Velox/animfetch

die() {
	printf 'animfetch: %s\n' "$*" >&2
	exit 1
}

note() {
	printf '%s\n' "$*" >&2
}

# ---------------------------------------------------------------------------
# What are we installing, and where
# ---------------------------------------------------------------------------

case "$(uname -s)" in
	Linux)
		case "$(uname -m)" in
			x86_64 | amd64) target=x86_64-unknown-linux-musl ;;
			aarch64 | arm64) target=aarch64-unknown-linux-musl ;;
			*) die "no prebuilt Linux binary for $(uname -m); install from source instead:
  cargo install --locked --git https://github.com/$REPO" ;;
		esac
		;;
	Darwin)
		case "$(uname -m)" in
			arm64) target=aarch64-apple-darwin ;;
			x86_64) target=x86_64-apple-darwin ;;
			*) die "no prebuilt macOS binary for $(uname -m)" ;;
		esac
		;;
	*) die "unsupported platform $(uname -s); animfetch runs on Linux and macOS" ;;
esac

if [ -n "${ANIMFETCH_BINDIR:-}" ]; then
	bindir=$ANIMFETCH_BINDIR
elif [ "$(id -u)" = 0 ]; then
	bindir=/usr/local/bin
else
	bindir=$HOME/.local/bin
fi

# One of the two, whichever exists. Both are asked to fail on HTTP errors rather
# than save an error page as if it were a tarball.
if command -v curl >/dev/null 2>&1; then
	fetch() { curl -fsSL "$1" -o "$2"; }
elif command -v wget >/dev/null 2>&1; then
	fetch() { wget -qO "$2" "$1"; }
else
	die 'need curl or wget'
fi

version=${ANIMFETCH_VERSION:-}
if [ -z "$version" ]; then
	note 'Resolving the latest release...'
	# The redirect target of /releases/latest ends in the tag, which avoids
	# depending on a JSON parser being installed.
	if command -v curl >/dev/null 2>&1; then
		url=$(curl -fsSLo /dev/null -w '%{url_effective}' \
			"https://github.com/$REPO/releases/latest") || url=
	else
		url=$(wget -qS --max-redirect=10 -O /dev/null \
			"https://github.com/$REPO/releases/latest" 2>&1 |
			sed -n 's|.*[Ll]ocation: *||p' | tail -1) || url=
	fi
	version=$(printf '%s' "$url" | sed -n 's|.*/tag/||p')
	[ -n "$version" ] || die "could not find the latest release; set ANIMFETCH_VERSION, or see
  https://github.com/$REPO/releases"
fi

name=animfetch-$version-$target
base=https://github.com/$REPO/releases/download/$version

# ---------------------------------------------------------------------------
# Download, verify, install
# ---------------------------------------------------------------------------

tmp=$(mktemp -d)
# Runs on every exit path, so a failed download leaves nothing behind.
trap 'rm -rf "$tmp"' EXIT INT TERM

note "Downloading $name..."
fetch "$base/$name.tar.gz" "$tmp/$name.tar.gz" ||
	die "no release asset $name.tar.gz
  Check that $version exists and has binaries attached:
  https://github.com/$REPO/releases"

# Not fatal if the checksum file is missing, since an early release may not
# have one, but a checksum that is present and wrong stops the install.
# sha256sum on Linux, shasum on macOS; same output format either way.
if command -v sha256sum >/dev/null 2>&1; then
	sha256() { sha256sum "$1"; }
elif command -v shasum >/dev/null 2>&1; then
	sha256() { shasum -a 256 "$1"; }
else
	sha256() { return 1; }
fi

if fetch "$base/SHA256SUMS" "$tmp/SHA256SUMS" 2>/dev/null &&
	[ -s "$tmp/SHA256SUMS" ] &&
	sha256 /dev/null >/dev/null 2>&1; then
	expected=$(sed -n "s|  *\**$name\.tar\.gz\$||p" "$tmp/SHA256SUMS" | head -1)
	if [ -n "$expected" ]; then
		actual=$(sha256 "$tmp/$name.tar.gz" | cut -d' ' -f1)
		[ "$expected" = "$actual" ] || die "checksum mismatch for $name.tar.gz
  expected $expected
  got      $actual"
		note 'Checksum verified.'
	fi
else
	note 'Warning: could not verify a checksum for this download.'
fi

tar -xzf "$tmp/$name.tar.gz" -C "$tmp" --strip-components=1 "$name/animfetch" ||
	die 'the downloaded archive does not contain an animfetch binary'

mkdir -p "$bindir"
# Via a temporary name in the destination directory, so replacing a binary that
# is currently running (a pinned instance, say) cannot leave a half-written one.
install -m755 "$tmp/animfetch" "$bindir/.animfetch.new" ||
	die "cannot write to $bindir; set ANIMFETCH_BINDIR to somewhere you own"
mv -f "$bindir/.animfetch.new" "$bindir/animfetch"

note "Installed $("$bindir/animfetch" --version) to $bindir/animfetch"

# ---------------------------------------------------------------------------
# What to do next
# ---------------------------------------------------------------------------

case ":$PATH:" in
	*":$bindir:"*) ;;
	*)
		note ''
		note "$bindir is not on your PATH. Add this to your shell startup file:"
		note "  export PATH=\"$bindir:\$PATH\""
		;;
esac

note ''
note 'Try it:            animfetch --once'
note 'Pin it to a shell: add this line to ~/.bashrc or ~/.zshrc'
note '  [[ $- == *i* ]] && command -v animfetch >/dev/null && animfetch --pin'
