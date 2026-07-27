#!/usr/bin/env bash

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
installer="$repo_root/site/public/install"
test_root="$(mktemp -d "${TMPDIR:-/tmp}/anybuild-installer-tests.XXXXXX")"
trap 'rm -rf "$test_root"' EXIT
path_source_line=". \"\$HOME/.anybuild/env\""
fish_path_line="fish_add_path -- \"\$HOME/.anybuild/bin\""

fail() {
  echo "installer test failed: $*" >&2
  exit 1
}

assert_file_contains() {
  local file=$1
  local text=$2
  grep -Fq "$text" "$file" ||
    fail "expected $file to contain: $text"
}

sha256_file() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | awk '{ print $1 }'
  else
    shasum -a 256 "$1" | awk '{ print $1 }'
  fi
}

make_downloads() {
  local directory=$1
  local version=$2
  local payload="$test_root/payload-$version"
  local targets=(
    aarch64-apple-darwin
    x86_64-apple-darwin
    aarch64-unknown-linux-musl
    x86_64-unknown-linux-musl
  )

  rm -rf "$directory" "$payload"
  mkdir -p "$directory" "$payload"
  cat > "$payload/anybuild" <<EOF
#!/bin/sh
printf '%s\n' '$version'
EOF
  chmod +x "$payload/anybuild"

  : > "$directory/SHA256SUMS"
  for target in "${targets[@]}"; do
    local asset="anybuild-$target.tar.gz"
    tar -C "$payload" -czf "$directory/$asset" anybuild
    printf '%s  %s\n' "$(sha256_file "$directory/$asset")" "$asset" \
      >> "$directory/SHA256SUMS"
  done
}

fake_bin="$test_root/bin"
downloads="$test_root/downloads"
mkdir -p "$fake_bin"

cat > "$fake_bin/uname" <<'EOF'
#!/bin/sh
case "$1" in
  -s) printf '%s\n' "$FAKE_UNAME_S" ;;
  -m) printf '%s\n' "$FAKE_UNAME_M" ;;
  *) exit 1 ;;
esac
EOF

cat > "$fake_bin/curl" <<'EOF'
#!/bin/sh
set -eu
url=
output=
while [ "$#" -gt 0 ]; do
  case "$1" in
    --proto)
      shift 2
      ;;
    --tlsv1.2 | -fLsS)
      shift
      ;;
    -o)
      output=$2
      shift 2
      ;;
    https://*)
      url=$1
      shift
      ;;
    *)
      shift
      ;;
  esac
done
[ -n "$url" ] && [ -n "$output" ]
printf '%s\n' "$url" >> "$FAKE_CURL_LOG"
cp "$FAKE_DOWNLOAD_DIR/${url##*/}" "$output"
EOF
chmod +x "$fake_bin/uname" "$fake_bin/curl"

run_installer() {
  local home=$1
  local system=$2
  local machine=$3
  shift 3

  env \
    HOME="$home" \
    PATH="$fake_bin:$PATH" \
    SHELL="${TEST_SHELL:-/bin/zsh}" \
    FAKE_UNAME_S="$system" \
    FAKE_UNAME_M="$machine" \
    FAKE_CURL_LOG="$test_root/curl.log" \
    FAKE_DOWNLOAD_DIR="$downloads" \
    "$@" \
    sh "$installer"
}

sh -n "$installer"
make_downloads "$downloads" "0.24.0"

platform_cases=(
  "Darwin:x86_64:anybuild-x86_64-apple-darwin.tar.gz"
  "Darwin:arm64:anybuild-aarch64-apple-darwin.tar.gz"
  "Linux:amd64:anybuild-x86_64-unknown-linux-musl.tar.gz"
  "Linux:aarch64:anybuild-aarch64-unknown-linux-musl.tar.gz"
)

for platform_case in "${platform_cases[@]}"; do
  IFS=: read -r system machine asset <<<"$platform_case"
  home="$test_root/platform-$system-$machine"
  mkdir -p "$home"
  : > "$test_root/curl.log"
  run_installer "$home" "$system" "$machine" ANYBUILD_NO_PATH_UPDATE=1 \
    >/dev/null
  assert_file_contains "$test_root/curl.log" \
    "https://github.com/wasmerio/anybuild/releases/latest/download/$asset"
  [ "$("$home/.anybuild/bin/anybuild" --version)" = "0.24.0" ] ||
    fail "wrong installed version for $system/$machine"
done

home="$test_root/default-install"
mkdir -p "$home"
: > "$test_root/curl.log"
run_installer "$home" Darwin arm64 ANYBUILD_VERSION=0.24.0 >/dev/null
run_installer "$home" Darwin arm64 ANYBUILD_VERSION=v0.24.0 >/dev/null
assert_file_contains "$test_root/curl.log" \
  "https://github.com/wasmerio/anybuild/releases/download/v0.24.0/"
[ "$(grep -Fxc "$path_source_line" "$home/.zshrc")" -eq 1 ] ||
  fail "zsh profile update was not idempotent"
assert_file_contains "$home/.anybuild/env" "$home/.anybuild/bin"

make_downloads "$downloads" "0.25.0"
run_installer "$home" Darwin arm64 ANYBUILD_VERSION=0.25.0 >/dev/null
[ "$("$home/.anybuild/bin/anybuild" --version)" = "0.25.0" ] ||
  fail "upgrade did not replace the installed binary"

home="$test_root/no-path-update"
mkdir -p "$home"
run_installer "$home" Linux x86_64 ANYBUILD_NO_PATH_UPDATE=1 >/dev/null
[ ! -e "$home/.bashrc" ] || fail "path opt-out modified the shell profile"
[ -f "$home/.anybuild/env" ] || fail "path opt-out did not create the env file"

for shell_case in "Linux:bash:.bashrc" "Darwin:bash:.bash_profile" \
  "Linux:sh:.profile"; do
  IFS=: read -r system shell_name profile <<<"$shell_case"
  home="$test_root/profile-$system-$shell_name"
  mkdir -p "$home"
  TEST_SHELL="/bin/$shell_name" run_installer "$home" "$system" x86_64 \
    >/dev/null
  [ "$(grep -Fxc "$path_source_line" "$home/$profile")" -eq 1 ] ||
    fail "$shell_name profile was not configured"
done
unset TEST_SHELL

home="$test_root/fish"
mkdir -p "$home"
TEST_SHELL=/usr/bin/fish run_installer "$home" Linux x86_64 >/dev/null
assert_file_contains "$home/.config/fish/conf.d/anybuild.fish" \
  "$fish_path_line"
unset TEST_SHELL

home="$test_root/custom-install"
custom_install="$home/custom bin"
mkdir -p "$home"
run_installer "$home" Linux x86_64 \
  "ANYBUILD_INSTALL_DIR=$custom_install" ANYBUILD_NO_PATH_UPDATE=1 >/dev/null
[ "$("$custom_install/anybuild" --version)" = "0.25.0" ] ||
  fail "custom install directory was not honored"
resolved=$(
  HOME="$home" PATH=/usr/bin:/bin sh -c \
    '. "$HOME/.anybuild/env"; command -v anybuild'
)
[ "$resolved" = "$custom_install/anybuild" ] ||
  fail "custom install directory was not written to the env file"

home="$test_root/checksum"
mkdir -p "$home/.anybuild/bin"
cat > "$home/.anybuild/bin/anybuild" <<'EOF'
#!/bin/sh
printf '%s\n' old
EOF
chmod +x "$home/.anybuild/bin/anybuild"
printf '%064d  %s\n' 0 "anybuild-x86_64-unknown-linux-musl.tar.gz" \
  > "$downloads/SHA256SUMS"
if run_installer "$home" Linux x86_64 ANYBUILD_NO_PATH_UPDATE=1 \
  >/dev/null 2>&1; then
  fail "installer accepted an invalid checksum"
fi
[ "$("$home/.anybuild/bin/anybuild" --version)" = "old" ] ||
  fail "checksum failure replaced the existing installation"

make_downloads "$downloads" "0.25.0"
home="$test_root/unsupported"
mkdir -p "$home"
if run_installer "$home" Plan9 x86_64 ANYBUILD_NO_PATH_UPDATE=1 \
  >"$test_root/unsupported.out" 2>&1; then
  fail "installer accepted an unsupported operating system"
fi
assert_file_contains "$test_root/unsupported.out" \
  "unsupported operating system: Plan9"

if run_installer "$home" Linux riscv64 ANYBUILD_NO_PATH_UPDATE=1 \
  >"$test_root/unsupported-arch.out" 2>&1; then
  fail "installer accepted an unsupported architecture"
fi
assert_file_contains "$test_root/unsupported-arch.out" \
  "unsupported Linux architecture: riscv64"

if run_installer "$home" Linux x86_64 ANYBUILD_VERSION=not-a-version \
  >"$test_root/invalid-version.out" 2>&1; then
  fail "installer accepted an invalid version"
fi
assert_file_contains "$test_root/invalid-version.out" \
  "invalid version: not-a-version"

echo "installer tests passed"
