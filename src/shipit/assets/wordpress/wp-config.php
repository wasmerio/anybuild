<?php

define( 'WP_AUTO_UPDATE_CORE', false); // Disable automatic aupdates and checks

/**
 * The base configuration for WordPress
 *
 * The wp-config.php creation script uses this file during the installation.
 * You don't have to use the web site, you can copy this file to "wp-config.php"
 * and fill in the values.
 *
 * This file contains the following configurations:
 *
 * * Database settings
 * * Secret keys
 * * Database table prefix
 * * ABSPATH
 *
 * @link https://wordpress.org/support/article/editing-wp-config-php/
 *
 * @package WordPress
 */


function get_env_var(string $name, string $default = ''): string
{
    if (isset($_ENV[$name])) {
	    return $_ENV[$name];
    }

    $stderr = fopen("php://stderr", "wb");
    fwrite($stderr, "Configuration error: environment variable " . $name . " not provided. Using default value: " . $default . PHP_EOL);
    fclose($stderr);

    return $default;
}

// ** Database settings - You can get this info from your web host ** //
/** The name of the database for WordPress */
define( 'DB_NAME', get_env_var('DB_NAME', 'wordpress') );

/** Database username */
define( 'DB_USER', get_env_var('DB_USERNAME', 'root') );

/** Database password */
define( 'DB_PASSWORD', get_env_var('DB_PASSWORD', '') );

/** Database hostname */
define( 'DB_HOST', get_env_var('DB_HOST', '127.0.0.1') . ":" . get_env_var('DB_PORT', '3306') );

define('MYSQL_CLIENT_FLAGS', MYSQLI_CLIENT_SSL);

/** Database charset to use in creating database tables. */
define( 'DB_CHARSET', 'utf8' );

/** The database collate type. Don't change this if in doubt. */
define( 'DB_COLLATE', 'utf8mb4_general_ci' );

// define('WP_ALLOW_REPAIR', true);


// define('DB_DIR', dirname(dirname(__FILE__)) . '/db/');

/**#@+
 * Authentication unique keys and salts.
 *
 * Change these to different unique phrases! You can generate these using
 * the {@link https://api.wordpress.org/secret-key/1.1/salt/ WordPress.org secret-key service}.
 *
 * You can change these at any point in time to invalidate all existing cookies.
 * This will force all users to have to log in again.
 *
 * @since 2.6.0
 */
define('AUTH_KEY', get_env_var('AUTH_KEY', 'no secret provided'));
define('SECURE_AUTH_KEY', get_env_var('SECURE_AUTH_KEY', 'no secret provided'));
define('LOGGED_IN_KEY', get_env_var('LOGGED_IN_KEY', 'no secret provided'));
define('NONCE_KEY', get_env_var('NONCE_KEY', 'no secret provided'));
define('AUTH_SALT', get_env_var('AUTH_SALT', 'no secret provided'));
define('SECURE_AUTH_SALT', get_env_var('SECURE_AUTH_SALT', 'no secret provided'));
define('LOGGED_IN_SALT', get_env_var('LOGGED_IN_SALT', 'no secret provided'));
define('NONCE_SALT', get_env_var('NONCE_SALT', 'no secret provided'));


$scheme = isset( $_SERVER['HTTPS'] ) && '1' === (string) $_SERVER['HTTPS'] ? "https://" : "http://";

if (!defined('WP_HOME')) {
    define( 'WP_HOME',  isset($_SERVER['HTTP_HOST']) ? ($scheme . $_SERVER['HTTP_HOST'] ): "http://localhost");
}

define( 'WP_SITEURL', WP_HOME . '/' );

define( 'WP_MEMORY_LIMIT', '256M' );
define( 'WP_MAX_MEMORY_LIMIT', '256M' );
define( 'WP_POST_REVISIONS', false );

/**#@-*/

/**
 * WordPress database table prefix.
 *
 * You can have multiple installations in one database if you give each
 * a unique prefix. Only numbers, letters, and underscores please!
 */
$table_prefix = 'wp_';

/**
 * For developers: WordPress debugging mode.
 *
 * Change this to true to enable the display of notices during development.
 * It is strongly recommended that plugin and theme developers use WP_DEBUG
 * in their development environments.
 *
 * For information on other constants that can be used for debugging,
 * visit the documentation.
 *
 * @link https://wordpress.org/support/article/debugging-in-wordpress/
 */
define( 'WP_DEBUG', false );

/* Add any custom values between this line and the "stop editing" line. */



/* That's all, stop editing! Happy publishing. */

/** Absolute path to the WordPress directory. */
if ( ! defined( 'ABSPATH' ) ) {
    define( 'ABSPATH', __DIR__ . '/' );
}

/** Sets up WordPress vars and included files. */
require_once ABSPATH . 'wp-settings.php';