<?php

declare(strict_types=1);

$root = getenv('TYPECHO_APP_PATH') ?: '/app';
$config = $root . '/config.inc.php';
$persistentConfig = $root . '/usr/config.inc.php';

if ((is_file($config) || is_link($config)) && !unlink($config)) {
    throw new \RuntimeException('Unable to replace the Typecho config link');
}
// The volume may still be empty until after_deploy seeds its configuration.
if (!symlink($persistentConfig, $config)) {
    throw new \RuntimeException('Unable to link the persistent Typecho config');
}
clearstatcache();
