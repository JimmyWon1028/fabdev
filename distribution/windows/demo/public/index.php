<?php

header('Content-Type: application/json; charset=utf-8');

echo json_encode([
  'app' => 'fabDev',
  'site' => 'demo.test',
  'php' => PHP_VERSION,
  'status' => 'ok',
], JSON_UNESCAPED_SLASHES | JSON_UNESCAPED_UNICODE);
