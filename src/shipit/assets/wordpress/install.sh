# Needed to get the WP-CLI commands to avoid asking for the TTY size
export COLUMNS=80

echo "Creating required directories..."

mkdir -p wp-content/plugins
mkdir -p wp-content/upgrade

echo "Installing WordPress core..."

wp core install \
  --url="$WP_SITE_URL"  \
  --title="$WP_SITE_TITLE" \
  --admin_user="$WP_ADMIN_USERNAME" \
  --admin_password="$WP_ADMIN_PASSWORD" \
  --admin_email="$WP_ADMIN_EMAIL" \
  --locale="$WP_LOCALE"


if [ -z "$WP_UPDATE_DB" ]; then
    echo "Updating database..."
    wp core update-db
fi

echo "Installation complete"
