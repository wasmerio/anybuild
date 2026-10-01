<?php

declare(strict_types=1);

(static function (): void {
    $app = getenv('TYPECHO_APP_PATH');
    $base = getenv('TYPECHO_USR_BASE_PATH');
    if (!$app || !$base) {
        throw new RuntimeException('Typecho runtime paths are missing');
    }

    $usr = $app . '/usr';
    $storedConfig = $usr . '/.anybuild/config.inc.php';
    $privateDirectory = $usr . '/.anybuild/';
    if (strncmp(
        $_SERVER['SCRIPT_FILENAME'] ?? '',
        $privateDirectory,
        strlen($privateDirectory)
    ) === 0) {
        http_response_code(404);
        exit;
    }

    // A fresh volume hides the packaged themes and plugins. Seed missing files
    // once, preserving anything the site owner has already customized.
    if (!is_file($usr . '/.anybuild/seeded')) {
        $seed = static function (string $source, string $target) use (&$seed): void {
            if (!is_dir($target) && !mkdir($target, 0755, true)
                && !is_dir($target)) {
                throw new RuntimeException('Cannot create Typecho directory: ' . $target);
            }
            foreach (new DirectoryIterator($source) as $entry) {
                if ($entry->isDot()) {
                    continue;
                }
                $destination = $target . '/' . $entry->getFilename();
                if ($entry->isDir()) {
                    $seed($entry->getPathname(), $destination);
                } elseif (!file_exists($destination)
                    && !copy($entry->getPathname(), $destination)) {
                    throw new RuntimeException('Cannot seed Typecho file: ' . $destination);
                }
            }
        };
        $seed($base, $usr);
        foreach ([$usr . '/uploads', $usr . '/.anybuild'] as $directory) {
            if (!is_dir($directory) && !mkdir($directory, 0755, true)
                && !is_dir($directory)) {
                throw new RuntimeException('Cannot create Typecho directory: ' . $directory);
            }
        }
        if (file_put_contents($usr . '/.anybuild/seeded', '1') === false) {
            throw new RuntimeException('Cannot mark Typecho volume as seeded');
        }
    }

    // Keep the generated config at its original path when executing it:
    // Typecho uses dirname(__FILE__) to locate its application root.
    $writeConfig = static function (string $target, string $contents): void {
        $temporary = dirname($target) . '/.typecho-'
            . bin2hex(random_bytes(8)) . '.php';
        if (file_put_contents($temporary, $contents) === false
            || !rename($temporary, $target)) {
            @unlink($temporary);
            throw new RuntimeException('Cannot save Typecho configuration: ' . $target);
        }
    };
    $config = $app . '/config.inc.php';
    if (is_file($storedConfig)) {
        $contents = file_get_contents($storedConfig);
        if (!is_file($config) || file_get_contents($config) !== $contents) {
            $writeConfig($config, $contents);
        }
    }
    register_shutdown_function(static function () use (
        $config, $storedConfig, $writeConfig
    ): void {
        if (is_file($config)) {
            $contents = file_get_contents($config);
            if (!is_file($storedConfig)
                || file_get_contents($storedConfig) !== $contents) {
                $writeConfig($storedConfig, $contents);
            }
        }
    });
})();
