#!/usr/bin/env bash

ROOT_HTACCESS="/app/wp-content/root-htaccess"
APP_HTACCESS="/app/.htaccess"

if [[ -e "$APP_HTACCESS" ]]; then
  if [[ ! -f "$ROOT_HTACCESS" ]]; then
    echo "Coppying app's .htaccess file into wp-content"
    # Note: doing cp fails, so we resort to this workaround.
    cat "$APP_HTACCESS" > "$ROOT_HTACCESS"
  fi
  rm "$APP_HTACCESS"
fi

# If no base htaccess is found, then we create a symlink to the root htaccess
if [[ ! -e "$APP_HTACCESS" ]]; then
  if [[ -f "$ROOT_HTACCESS" ]]; then
    echo "Using wp-content .htaccess file as app's htaccess"
    ln -s "$ROOT_HTACCESS" "$APP_HTACCESS"
  fi
fi

phpix "$@"