#!/usr/bin/env bash
set -euo pipefail

root="${1:-node_modules}"

if [[ ! -d "$root" ]]; then
  exit 0
fi

is_wasm() {
  local file="$1"
  local magic

  case "$file" in
    *.wasm | *.WASM)
      return 0
      ;;
  esac

  magic="$(dd if="$file" bs=4 count=1 2>/dev/null | od -An -tx1 | tr -d ' \n')"
  [[ "$magic" == "0061736d" ]]
}

while IFS= read -r -d "" file; do
  if is_wasm "$file"; then
    continue
  fi

  if ! grep -Iq . "$file"; then
    rm -f -- "$file"
  fi
done < <(
  find "$root" -type f \( -perm -100 -o -perm -010 -o -perm -001 \) -print0
)

# Removing native executables can leave dangling .bin links that make cp -RL fail.
while IFS= read -r -d "" link; do
  if [[ ! -e "$link" ]]; then
    rm -f -- "$link"
  fi
done < <(find "$root" -type l -print0)
