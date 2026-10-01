<?php

$root = getenv('TYPECHO_APP_PATH') ?: dirname(__DIR__);

define('__TYPECHO_ROOT_DIR__', $root);
define('__TYPECHO_PLUGIN_DIR__', '/usr/plugins');
define('__TYPECHO_THEME_DIR__', '/usr/themes');
define('__TYPECHO_ADMIN_DIR__', '/admin/');

require_once __TYPECHO_ROOT_DIR__ . '/var/Typecho/Common.php';
\Typecho\Common::init();

$env = static function (string $name, string $default = ''): string {
    $value = getenv($name);
    return $value === false ? $default : $value;
};
$adapter = $env('TYPECHO_DB_ADAPTER', 'Pdo_SQLite');
$postgres = strpos($adapter, 'Pgsql') !== false;
$db = new \Typecho\Db($adapter, $env('TYPECHO_DB_PREFIX', 'typecho_'));
$connection = [
    'file' => $env('TYPECHO_DB_FILE', __TYPECHO_ROOT_DIR__ . '/usr/typecho.db'),
    'host' => $env('TYPECHO_DB_HOST', $env('DB_HOST', '127.0.0.1')),
    'port' => (int) $env('TYPECHO_DB_PORT', $env('DB_PORT', $postgres ? '5432' : '3306')),
    'user' => $env('TYPECHO_DB_USER', $env('DB_USERNAME', $postgres ? '' : 'root')),
    'password' => $env('TYPECHO_DB_PASSWORD', $env('DB_PASSWORD')),
    'database' => $env('TYPECHO_DB_DATABASE', $env('DB_NAME', 'typecho')),
    'charset' => $env('TYPECHO_DB_CHARSET', $postgres ? 'utf8' : 'utf8mb4'),
    'dsn' => $env('TYPECHO_DB_DSN'),
    'engine' => $env('TYPECHO_DB_ENGINE', 'InnoDB'),
    'sslCa' => $env('TYPECHO_DB_SSL_CA'),
    'sslVerify' => in_array(
        strtolower($env('TYPECHO_DB_SSL_VERIFY', 'off')),
        ['1', 'true', 'on', 'yes'],
        true
    ),
];
// The CLI installer validates TYPECHO_DB_* independently of the loaded config.
if (PHP_SAPI === 'cli') {
    foreach (['host', 'port', 'user', 'password', 'database', 'file', 'charset', 'dsn', 'engine'] as $key) {
        $_SERVER['TYPECHO_DB_' . strtoupper($key)] = (string) $connection[$key];
    }
}
$db->addServer($connection, \Typecho\Db::READ | \Typecho\Db::WRITE);
\Typecho\Db::set($db);
