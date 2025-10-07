# Needed to get the WP-CLI commands to avoid asking for the TTY size, which
# doesn't work because we don't have the stty command it uses.
export COLUMNS=80

echo "Creating required directories..."

mkdir -p wp-content/plugins
echo "" > wp-content/plugins/.keep

mkdir -p wp-content/upgrade
echo "" > wp-content/upgrade/.keep

echo "Installing WordPress core..."

wp core install \
  --url="$WASMER_APP_URL"  \
  --title="$WP_SITE_TITLE" \
  --admin_user="$WP_ADMIN_USERNAME" \
  --admin_password="$WP_ADMIN_PASSWORD" \
  --admin_email="$WP_ADMIN_EMAIL" \
  --locale="$WP_LOCALE"


if [ -z "$WP_UPDATE_DB" ]; then
    wp core update-db
fi

echo "Installation complete"
