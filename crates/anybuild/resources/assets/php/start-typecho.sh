#!/usr/bin/env bash
set -euo pipefail

# The stock CLI installer validates TYPECHO_DB_* even with an existing config.
port=3306
user=root
case "${TYPECHO_DB_ADAPTER-}" in
    *Pgsql) port=5432; user=postgres ;;
esac
export TYPECHO_DB_HOST="${TYPECHO_DB_HOST-${DB_HOST-127.0.0.1}}"
export TYPECHO_DB_PORT="${TYPECHO_DB_PORT-${DB_PORT-$port}}"
export TYPECHO_DB_USER="${TYPECHO_DB_USER-${DB_USERNAME-$user}}"
export TYPECHO_DB_PASSWORD="${TYPECHO_DB_PASSWORD-${DB_PASSWORD-}}"
export TYPECHO_DB_DATABASE="${TYPECHO_DB_DATABASE-${DB_NAME-typecho}}"

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
