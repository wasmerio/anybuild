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

// Deserialize the value from the environment variable value
function env_var_deserialize(string $value) {
    $v = trim($value);
    if ($v === '') {
        return '';
    }
    $len = strlen($v);
    if ($len >= 2 && ($v[0] === '"' || $v[0] === "'") && $v[0] === $v[$len - 1]) {
        return substr($v, 1, -1);
    }
    $lower = strtolower($v);
    static $scalarMap = [
        'true'  => true,
        'yes'   => true,
        'on'    => true,
        'y'     => true,
        'false' => false,
        'no'    => false,
        'off'   => false,
        'n'     => false,
        'null'  => null,
    ];
    if (array_key_exists($lower, $scalarMap)) {
        return $scalarMap[$lower];
    }
    if (ctype_digit($v) || ($v[0] === '-' && ctype_digit(substr($v, 1)))) {
        return (int) $v;
    }
    if (is_numeric($v)) {
        return (float) $v;
    }
    if (strpos($v, '://') !== false && filter_var($v, FILTER_VALIDATE_URL)) {
        return $v;
    }
    if (strpos($v, ',') !== false) {
        if (strncmp($v, 'http://', 7) === 0 || strncmp($v, 'https://', 8) === 0) {
            return $v;
        }
        return array_map('trim', explode(',', $v));
    }
    return $v;
}


function get_env_var(string $name, string $default = '')
{
    $value = getenv($name);
    if ($value !== false) {
        return env_var_deserialize($value);
    }

    if ($default === '') {
        error_log("Warning: env var " . $name . " was not provided.");
    }

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


if ( ! isset( $_SERVER['HTTPS'] )
	&& isset( $_SERVER['HTTP_X_FORWARDED_PROTO'] )
	&& 'https' === strtolower( $_SERVER['HTTP_X_FORWARDED_PROTO'] )
) {
	// We need to actually set it so WP would know it's behind TLS.
	$_SERVER['HTTPS'] = '1';
}

$scheme = isset( $_SERVER['HTTPS'] ) && '1' === (string) $_SERVER['HTTPS'] ? "https://" : "http://";

if (!defined('WP_HOME')) {
    define( 'WP_HOME',  get_env_var('WP_HOME', isset($_SERVER['HTTP_HOST']) ? ($scheme . $_SERVER['HTTP_HOST'] ): "http://localhost"));
}

define( 'WP_SITEURL', get_env_var('WP_SITEURL', WP_HOME . '/') );

define( 'WP_MEMORY_LIMIT', get_env_var('WP_MEMORY_LIMIT', '256M') );
define( 'WP_MAX_MEMORY_LIMIT', get_env_var('WP_MAX_MEMORY_LIMIT', '256M') );
define( 'WP_POST_REVISIONS', get_env_var('WP_POST_REVISIONS', false));

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
define( 'WP_DEBUG', get_env_var('WP_DEBUG', false) );

/* Add any custom values between this line and the "stop editing" line. */

// Optionally include an additional wp-config.php file if defined
if ( getenv('WP_ADDITIONAL_CONFIG') ) {
    $extra_config_path = getenv('WP_ADDITIONAL_CONFIG');

    if ( file_exists( $extra_config_path ) ) {
        require_once $extra_config_path;
    } else {
        error_log( "WP_ADDITIONAL_CONFIG defined but file not found: {$extra_config_path}" );
    }
}

/** Absolute path to the WordPress directory. */
if ( ! defined( 'ABSPATH' ) ) {
    define( 'ABSPATH', __DIR__ . '/' );
}

function wpdefine_load_env_defines(string $prefix = 'WPDEFINE_'): void {
    $env = $_ENV + $_SERVER;
    $prefixLen = strlen($prefix);
    foreach ($env as $key => $rawValue) {
        if (!is_string($key) || strncmp($key, $prefix, $prefixLen) !== 0) {
            continue;
        }
        $const = substr($key, $prefixLen);
        if ($const === '' || defined($const)) {
            continue;
        }
        if (!preg_match('/^[A-Z_][A-Z0-9_]*$/', $const)) {
            continue;
        }
        define($const, env_var_deserialize((string) $rawValue));
    }
}

wpdefine_load_env_defines();

/* That's all, stop editing! Happy publishing. */

/** Sets up WordPress vars and included files. */
require_once ABSPATH . 'wp-settings.php';