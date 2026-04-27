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

if ($f['size'] > $maxBytes) {
    respond(413, ['ok' => false, 'error' => 'Too large']);
}

if (!str_ends_with($name, '.h5')) {
    respond(400, ['ok' => false, 'error' => 'File must end with .h5']);
}

$stem = substr($name, 0, -3); // remove ".h5"

// Accept one or more dash-separated labels, then final unix-seconds field.
// Labels: letters, digits, underscore
// Timestamp: 10+ digits, optionally decimal fraction
$pattern = '/^(?<prefix>[A-Za-z0-9_]+(?:-[A-Za-z0-9_]+)*)-(?<ts>[0-9]{10,}(?:\.[0-9]+)?)$/';

if (!preg_match($pattern, $stem, $m)) {
    respond(400, ['ok' => false, 'error' => 'Bad filename']);
}

$prefix = $m['prefix'];
$unixTsString = $m['ts'];

// Use integer seconds to derive directory date
$unixTsInt = (int) floor((float) $unixTsString);

if ($unixTsInt <= 0) {
    respond(400, ['ok' => false, 'error' => 'Bad unix timestamp']);
}

try {
    $dt = new DateTimeImmutable('@' . $unixTsInt);
    $dt = $dt->setTimezone(new DateTimeZone('UTC'));
} catch (Exception $e) {
    respond(400, ['ok' => false, 'error' => 'Invalid timestamp']);
}

$dateDir = $dt->format('Y-m-d');

$targetDir   = $baseDir . '/' . $dateDir;
$destination = $targetDir . '/' . $name;
$tmpDest     = $destination . '.tmp';

if (!is_dir($targetDir)) {
    if (!mkdir($targetDir, 2775, true) && !is_dir($targetDir)) {
        respond(500, ['ok' => false, 'error' => 'Failed to create target directory']);
    }
    chmod($targetDir, 2775);
}

if (!is_writable($targetDir)) {
    respond(500, ['ok' => false, 'error' => 'Target directory is not writable']);
}

if (!move_uploaded_file($f['tmp_name'], $tmpDest)) {
    respond(500, ['ok' => false, 'error' => 'Save failed']);
}

chmod($tmpDest, 0664);

// Optional if you want to force group explicitly:
// chgrp($tmpDest, 'shovel');

if (!rename($tmpDest, $destination)) {
    @unlink($tmpDest);
    respond(500, ['ok' => false, 'error' => 'Replace failed']);
}

chmod($destination, 0664);

respond(201, [
    'ok' => true,
    'file' => $name,
    'prefix' => $prefix,
    'unix' => $unixTsString,
    'date_dir' => $dateDir,
    'path' => $destination
]);

