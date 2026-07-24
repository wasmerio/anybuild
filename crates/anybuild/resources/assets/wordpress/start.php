<?php

declare(strict_types=1);

$rootHtaccess = '/app/wp-content/root-htaccess';
$appHtaccess = '/app/.htaccess';

if (file_exists($appHtaccess)) {
    if (!is_file($rootHtaccess)) {
        echo "Copying app's .htaccess file into wp-content\n";
        $contents = file_get_contents($appHtaccess);
        if ($contents !== false) {
            file_put_contents($rootHtaccess, $contents);
        }
    }
    unlink($appHtaccess);
}

if (!file_exists($appHtaccess) && is_file($rootHtaccess)) {
    echo "Using wp-content .htaccess file as app's htaccess\n";
    symlink($rootHtaccess, $appHtaccess);
}
