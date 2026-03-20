<?php
declare(strict_types=1);

header('Content-Type: application/json');

$baseDir  = '/mnt/shovel/ionosonde';
$maxBytes = 1024 * 1024; // 1 MiB

umask(0002);

function respond(int $code, array $data): void {
    http_response_code($code);
    echo json_encode($data, JSON_UNESCAPED_SLASHES);
    exit;
}

if ($_SERVER['REQUEST_METHOD'] !== 'POST') {
    respond(405, ['ok' => false, 'error' => 'Use POST']);
}

if (!isset($_FILES['file'])) {
    respond(400, ['ok' => false, 'error' => 'Missing file field']);
}

$f = $_FILES['file'];

if (!is_uploaded_file($f['tmp_name'])) {
    respond(400, ['ok' => false, 'error' => 'Invalid upload']);
}

switch ($f['error']) {
    case UPLOAD_ERR_OK:
        break;
    case UPLOAD_ERR_INI_SIZE:
    case UPLOAD_ERR_FORM_SIZE:
        respond(413, ['ok' => false, 'error' => 'File too large']);
    default:
        respond(400, ['ok' => false, 'error' => 'Upload failed', 'code' => $f['error']]);
}

$name = $f['name'];

if (!preg_match('/^cdetections-([A-Za-z0-9_-]+)-([0-9]{10,})\.h5$/', $name, $m)) {
    respond(400, ['ok' => false, 'error' => 'Bad filename']);
}

$station = $m[1];
$unixTs  = $m[2];

if ($f['size'] > $maxBytes) {
    respond(413, ['ok' => false, 'error' => 'Too large']);
}

$dt = new DateTimeImmutable('@' . $unixTs);
$dt = $dt->setTimezone(new DateTimeZone('UTC'));
$dateDir = $dt->format('Y-m-d');

$targetDir = $baseDir . '/' . $dateDir;
$destination = $targetDir . '/' . $name;
$tmpDest = $destination . '.tmp';

if (!is_dir($targetDir)) {
    if (!mkdir($targetDir, 2775, true) && !is_dir($targetDir)) {
        respond(500, ['ok' => false, 'error' => 'Failed to create target directory']);
    }
}

if (!is_writable($targetDir)) {
    respond(500, ['ok' => false, 'error' => 'Target directory is not writable']);
}

if (!move_uploaded_file($f['tmp_name'], $tmpDest)) {
    respond(500, ['ok' => false, 'error' => 'Save failed']);
}

chmod($tmpDest, 0664);

// Optional, only if you want to force group after upload:
// chgrp($tmpDest, 'shovel');

if (!rename($tmpDest, $destination)) {
    @unlink($tmpDest);
    respond(500, ['ok' => false, 'error' => 'Replace failed']);
}

chmod($destination, 0664);

respond(201, [
    'ok' => true,
    'file' => $name,
    'station' => $station,
    'unix' => $unixTs,
    'date_dir' => $dateDir,
    'path' => $destination
]);

