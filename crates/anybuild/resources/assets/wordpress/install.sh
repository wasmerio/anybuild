# Needed to get the WP-CLI commands to avoid asking for the TTY size
IFS=$'\n\t'
set -e

export COLUMNS=80  # Prevent WP-CLI from asking for TTY size
export PAGER="cat"

WP_ADMIN_EMAIL=${WP_ADMIN_EMAIL:-"admin@example.com"}
WP_ADMIN_USERNAME=${WP_ADMIN_USERNAME:-"admin"}
WP_ADMIN_PASSWORD=${WP_ADMIN_PASSWORD:-"admin"}
WP_DEFAULT_LOCALE=${WP_DEFAULT_LOCALE:-"en_US"}
WP_LOCALE=${WP_LOCALE:-"$WP_DEFAULT_LOCALE"}
WP_SITEURL=${WP_SITEURL:-"http://localhost"}
WP_SITE_TITLE=${WP_SITE_TITLE:-"WordPress"}
WP_CONTENT_DIR=${WP_CONTENT_DIR:-"wp-content"}

echo "📁 Initializing wp-content..."
mkdir -p "${WP_CONTENT_DIR}/plugins"
mkdir -p "${WP_CONTENT_DIR}/themes"
mkdir -p "${WP_CONTENT_DIR}/upgrade"
mkdir -p "${WP_CONTENT_DIR}/languages"

if [ -n "${WPCONTENT_BASE_PATH:-}" ] && [ -d "${WPCONTENT_BASE_PATH}" ]; then
  shopt -s dotglob nullglob
  cp -R "${WPCONTENT_BASE_PATH}"/plugins/* "${WP_CONTENT_DIR}/plugins" || true
  if [ -n "${WP_DEFAULT_THEME:-}" ]; then
    cp -R "${WPCONTENT_BASE_PATH}"/themes/* "${WP_CONTENT_DIR}/themes" || true
  else
    cp -R "${WPCONTENT_BASE_PATH}"/themes/twentytwenty* "${WP_CONTENT_DIR}/themes" || true
  fi
  shopt -u dotglob nullglob
fi

if wp core is-installed && [ "${WP_FORCE_SETUP:-false}" != "true" ]; then
  echo "🚀 Setting up WordPress from an existing installation"
  if [ "${WP_UPDATE_DB:-true}" = "true" ]; then
      echo "🛠️ Activating maintenance mode..."
      wp maintenance-mode activate || true
      echo "🔄 Updating database..."
      wp core update-db
      echo "🛠️ Deactivating maintenance mode..."
      wp maintenance-mode deactivate || true
  fi
else
  echo "🚀 Setting up WordPress from a fresh install"

  echo "⚙️ Installing WordPress core"

  wp core install \
    --url="$WP_SITEURL"  \
    --title="$WP_SITE_TITLE" \
    --admin_user="$WP_ADMIN_USERNAME" \
    --admin_password="$WP_ADMIN_PASSWORD" \
    --admin_email="$WP_ADMIN_EMAIL" \
    --locale="$WP_LOCALE"

  echo "🔄 Setting permalinks"
  # Avoid `wp rewrite structure`: it relaunches wp-cli in a subprocess to flush
  # rewrites, which fails under WASIX (PHP_BINARY is empty) and aborts this
  # script via `set -e`. Setting the option directly is equivalent, since we
  # flush rewrites with `wp rewrite flush --hard` at the end of this script.
  wp option update permalink_structure '/%year%/%monthnum%/%day%/%postname%/'
fi

# Install plugins from WP_PLUGINS environment variable
if [ -n "${WP_PLUGINS:-}" ]; then
  echo "🛠️ Installing plugins from WP_PLUGINS: $WP_PLUGINS"

  IFS=','  # Split by commas
  for PLUGIN_ENTRY in $WP_PLUGINS; do
    if [[ "$PLUGIN_ENTRY" =~ ^https?:// ]]; then
      echo "• Installing plugin from URL: $PLUGIN_ENTRY"
      wp plugin install "$PLUGIN_ENTRY" --force --activate
    else
      # Extract name and version using parameter expansion
      PLUGIN_NAME="${PLUGIN_ENTRY%%:*}"
      PLUGIN_VERSION="${PLUGIN_ENTRY#*:}"

      if [[ "$PLUGIN_NAME" == "$PLUGIN_VERSION" ]]; then
        echo "• Installing plugin '${PLUGIN_NAME}' (latest version)..."
        wp plugin install "$PLUGIN_NAME" --force --activate
      else
        echo "• Installing plugin '${PLUGIN_NAME}' (version: ${PLUGIN_VERSION})..."
        wp plugin install "$PLUGIN_NAME" --force --version="$PLUGIN_VERSION" --activate
      fi
    fi
  done
fi

if [ -n "${WP_PLUGINS_ACTIVATE:-}" ]; then
  echo "✨ Activating plugins: $WP_PLUGINS_ACTIVATE"
  IFS=',' # Split by commas

  for PLUGIN_ENTRY in $WP_PLUGINS_ACTIVATE; do
    echo "• Activating plugin: $PLUGIN_ENTRY"
    wp plugin activate "$PLUGIN_ENTRY"
  done
fi

# Install themes from WP_THEMES environment variable
if [ -n "${WP_THEMES:-}" ]; then
  echo "🎨 Installing themes from WP_THEMES: $WP_THEMES"
  IFS=','

  for THEME_ENTRY in $WP_THEMES; do
    if [[ "$THEME_ENTRY" =~ ^https?:// ]]; then
      echo "• Installing theme from URL: $THEME_ENTRY"
      wp theme install "$THEME_ENTRY" --force
    else
      THEME_NAME="${THEME_ENTRY%%:*}"
      THEME_VERSION="${THEME_ENTRY#*:}"

      if [[ "$THEME_NAME" == "$THEME_VERSION" ]]; then
        echo "• Installing theme '${THEME_NAME}' (latest version)..."
        wp theme install "$THEME_NAME" --force
      else
        echo "• Installing theme '${THEME_NAME}' (version: ${THEME_VERSION})..."
        wp theme install "$THEME_NAME" --force --version="$THEME_VERSION"
      fi
    fi
  done
fi

if [ -n "${WP_DEFAULT_THEME:-}" ]; then
  echo "✨ Activating default theme: $WP_DEFAULT_THEME"
  wp theme activate "$WP_DEFAULT_THEME"
fi

if [ -n "${WP_LOCALE:-}" ]; then
  echo "🌐 Setting locale: $WP_LOCALE"
  # wp language theme install --all "$WP_LOCALE"
  # wp language plugin install --all "$WP_LOCALE"
  # `wp core install --locale` only writes the WPLANG option; it never fetches a
  # language pack. Without the pack, `wp site switch-language` fails with
  # "Language not installed." and aborts this script via `set -e`. Both steps are
  # non-fatal so a network hiccup can't take down the whole setup.
  wp language core install "$WP_LOCALE" || true
  wp site switch-language "$WP_LOCALE" || true
fi

echo "✍️ Rewriting permalinks structure"
wp rewrite flush --hard || true

# if [ ! -f "/app/wp-content/wp-config.php" ]; then
#   cat > /app/wp-content/wp-config.php <<EOF
# <?php
# // If you need to set custom configuration, you can place it here.
# // This file will be included by the main wp-config.php after
# // loading environment variables.
# EOF
# fi

echo "✅ WordPress Setup complete"
