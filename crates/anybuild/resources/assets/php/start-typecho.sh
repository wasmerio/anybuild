#!/usr/bin/env bash
set -euo pipefail

mkdir -p "$TYPECHO_APP_PATH/usr/uploads"
cp -Rn "$TYPECHO_USR_BASE_PATH/." "$TYPECHO_APP_PATH/usr/"

config="$TYPECHO_APP_PATH/config.inc.php"
if [ ! -L "$config" ]; then
    if [ -f "$config" ] && [ ! -e "$TYPECHO_APP_PATH/usr/config.inc.php" ]; then
        mv "$config" "$TYPECHO_APP_PATH/usr/config.inc.php"
    else
        rm -f "$config"
    fi
    ln -s usr/config.inc.php "$config"
fi

exec "$@"
