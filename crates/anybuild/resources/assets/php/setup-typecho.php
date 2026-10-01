<?php

declare(strict_types=1);

try {
    $root = getenv('TYPECHO_APP_PATH');
    $source = getenv('TYPECHO_USR_BASE_PATH');
    $usr = $root . '/usr';
    if (!is_dir($usr . '/uploads') && !mkdir($usr . '/uploads', 0777, true)) {
        throw new \RuntimeException('Unable to create Typecho upload directory');
    }

    $files = new \RecursiveIteratorIterator(
        new \RecursiveDirectoryIterator($source, \FilesystemIterator::SKIP_DOTS),
        \RecursiveIteratorIterator::SELF_FIRST
    );
    foreach ($files as $file) {
        $target = $usr . '/' . substr($file->getPathname(), strlen($source) + 1);
        if (file_exists($target) || is_link($target)) {
            continue;
        }
        // Copy contents without the permission changes unsupported by volumes.
        if ($file->isLink()) {
            $copied = symlink(readlink($file->getPathname()), $target);
        } elseif ($file->isDir()) {
            $copied = mkdir($target, 0777, true);
        } else {
            $copied = copy($file->getPathname(), $target);
        }
        if (!$copied) {
            throw new \RuntimeException('Unable to seed Typecho file: ' . $target);
        }
    }

    $env = static function (string $name, string $default): string {
        $value = getenv($name);
        return $value === false || $value === '' ? $default : $value;
    };
    $defaults = [
        'TYPECHO_DB_PREFIX' => 'typecho_',
        'TYPECHO_SITE_URL' => $env('WP_SITEURL', 'http://localhost'),
        'TYPECHO_USER_NAME' => 'admin',
        'TYPECHO_USER_MAIL' => 'admin@example.com',
        'TYPECHO_USER_PASSWORD' => 'admin',
    ];
    foreach ($defaults as $name => $default) {
        $value = $env($name, $default);
        putenv($name . '=' . $value);
        $_SERVER[$name] = $value;
    }

    if (in_array('--phpix', $argv, true)) {
        require getenv('TYPECHO_STARTUP_SCRIPT');
    }
    require_once $root . '/config.inc.php';
    $db = \Typecho\Db::get();

    // Typecho's installer exits with status 1 for an already installed site.
    try {
        $installed = $db->fetchRow($db->select()->from('table.options')
            ->where('user = 0 AND name = ?', 'installed'));
        if (!empty($installed['value'])) {
            echo 'Typecho already installed' . PHP_EOL;
            exit(0);
        }
    } catch (\Typecho\Db\Adapter\SQLException $e) {
        // A fresh database has no options table yet; let the installer create it.
    }

    require $root . '/install.php';
} catch (\Throwable $e) {
    fwrite(STDERR, $e->getMessage() . PHP_EOL);
    exit(1);
}
