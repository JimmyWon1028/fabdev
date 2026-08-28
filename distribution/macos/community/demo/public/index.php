<?php

header('Content-Type: application/json');

echo json_encode([
  'app' => 'fabDev Community Demo',
  'host' => $_SERVER['HTTP_HOST'] ?? null,
  'phpVersion' => PHP_VERSION,
  'documentRoot' => $_SERVER['DOCUMENT_ROOT'] ?? null,
], JSON_PRETTY_PRINT | JSON_UNESCAPED_SLASHES);
