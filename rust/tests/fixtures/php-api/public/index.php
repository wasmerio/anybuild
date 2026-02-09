<?php

require __DIR__ . '/../vendor/autoload.php';

header('Content-Type: application/json');

echo json_encode([
    'message' => 'Hello from PHP API',
    'version' => phpversion()
]);
