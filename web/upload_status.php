<?php
declare(strict_types=1);

header('Content-Type: application/json');

$baseDir = '/mnt/shovel/ionosonde';
$maxBytes = 256 * 1024;

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

if ($f['error'] !== UPLOAD_ERR_OK) {
    respond(400, ['ok' => false, 'error' => 'Upload failed', 'code' => $f['error']]);
}

if ($f['size'] > $maxBytes) {
    respond(413, ['ok' => false, 'error' => 'Too large']);
}

$name = $f['name'];
if (!preg_match('/^station_status-([A-Z0-9]+)-([0-9]{10,}(?:\.[0-9]+)?)\.json$/i', $name, $m)) {
    respond(400, ['ok' => false, 'error' => 'Bad status filename']);
}

$raw = file_get_contents($f['tmp_name']);
if ($raw === false) {
    respond(400, ['ok' => false, 'error' => 'Could not read upload']);
}

$payload = json_decode($raw, true);
if (!is_array($payload)) {
    respond(400, ['ok' => false, 'error' => 'Invalid JSON']);
}

if (($payload['schema'] ?? '') !== 'chirpsounder2.station_status.v1') {
    respond(400, ['ok' => false, 'error' => 'Bad status schema']);
}

$stationFromName = strtoupper($m[1]);
$unixTsString = $m[2];
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

$stationFromJson = strtoupper((string)($payload['station'] ?? ''));
if ($stationFromJson === '' || $stationFromJson !== $stationFromName) {
    respond(400, ['ok' => false, 'error' => 'Station mismatch']);
}

$generatedUnix = $payload['generated_unix'] ?? null;
if (!is_numeric($generatedUnix) || abs((float)$generatedUnix - (float)$unixTsString) > 1.0) {
    respond(400, ['ok' => false, 'error' => 'Timestamp mismatch']);
}

$dateDir = $dt->format('Y-m-d');
$targetDir = $baseDir . '/' . $dateDir;

if (!is_dir($targetDir)) {
    if (!mkdir($targetDir, 2775, true) && !is_dir($targetDir)) {
        respond(500, ['ok' => false, 'error' => 'Failed to create target directory']);
    }
    chmod($targetDir, 2775);
}

if (!is_writable($targetDir)) {
    respond(500, ['ok' => false, 'error' => 'Target directory is not writable']);
}

$destination = $targetDir . '/' . $name;
$tmpDest = $destination . '.tmp';

if (!move_uploaded_file($f['tmp_name'], $tmpDest)) {
    respond(500, ['ok' => false, 'error' => 'Save failed']);
}

chmod($tmpDest, 0664);

if (!rename($tmpDest, $destination)) {
    @unlink($tmpDest);
    respond(500, ['ok' => false, 'error' => 'Replace failed']);
}

chmod($destination, 0664);

respond(201, [
    'ok' => true,
    'file' => $name,
    'station' => $stationFromName,
    'unix' => $unixTsString,
    'date_dir' => $dateDir,
    'path' => $destination,
]);
