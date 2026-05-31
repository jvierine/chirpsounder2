<?php
declare(strict_types=1);

header('Content-Type: application/json');

$archiveBaseDir = '/mnt/shovel/ionosonde';
$dashboardDir = '/var/www/html/iono';
$maxBytes = 25 * 1024 * 1024;

umask(0002);

function respond(int $code, array $data): void {
    http_response_code($code);
    echo json_encode($data, JSON_UNESCAPED_SLASHES);
    exit;
}

function ensure_writable_dir(string $dir): void {
    if (!is_dir($dir)) {
        if (!mkdir($dir, 2775, true) && !is_dir($dir)) {
            respond(500, ['ok' => false, 'error' => 'Failed to create target directory']);
        }
        chmod($dir, 2775);
    }
    if (!is_writable($dir)) {
        respond(500, ['ok' => false, 'error' => 'Target directory is not writable']);
    }
}

function save_upload(array $file, string $destination): void {
    $tmpDest = $destination . '.tmp';
    if (!move_uploaded_file($file['tmp_name'], $tmpDest)) {
        respond(500, ['ok' => false, 'error' => 'Save failed']);
    }
    chmod($tmpDest, 0664);
    if (!rename($tmpDest, $destination)) {
        @unlink($tmpDest);
        respond(500, ['ok' => false, 'error' => 'Replace failed']);
    }
    chmod($destination, 0664);
}

function utc_date_dir_from_unix(float $unix): string {
    if ($unix <= 0) {
        respond(400, ['ok' => false, 'error' => 'Bad unix timestamp']);
    }
    try {
        $dt = new DateTimeImmutable('@' . (int)floor($unix));
        $dt = $dt->setTimezone(new DateTimeZone('UTC'));
    } catch (Exception $e) {
        respond(400, ['ok' => false, 'error' => 'Invalid timestamp']);
    }
    return $dt->format('Y-m-d');
}

function handle_h5(array $file, string $name, string $archiveBaseDir): void {
    $stem = substr($name, 0, -3);
    $pattern = '/^(?<prefix>[A-Za-z0-9_]+(?:-[A-Za-z0-9_]+)*)-(?<ts>[0-9]{10,}(?:\.[0-9]+)?)$/';
    if (!preg_match($pattern, $stem, $m)) {
        respond(400, ['ok' => false, 'error' => 'Bad HDF5 filename']);
    }
    $dateDir = utc_date_dir_from_unix((float)$m['ts']);
    $targetDir = rtrim($archiveBaseDir, '/') . '/' . $dateDir;
    ensure_writable_dir($targetDir);
    $destination = $targetDir . '/' . $name;
    save_upload($file, $destination);
    respond(201, [
        'ok' => true,
        'type' => 'h5',
        'file' => $name,
        'prefix' => $m['prefix'],
        'unix' => $m['ts'],
        'date_dir' => $dateDir,
        'path' => $destination,
    ]);
}

function copy_file_atomic(string $source, string $destination): void {
    $tmpDest = $destination . '.tmp';
    if (!copy($source, $tmpDest)) {
        respond(500, ['ok' => false, 'error' => 'Cache copy failed']);
    }
    chmod($tmpDest, 0664);
    if (!rename($tmpDest, $destination)) {
        @unlink($tmpDest);
        respond(500, ['ok' => false, 'error' => 'Cache replace failed']);
    }
    chmod($destination, 0664);
}

function handle_status_json(array $file, string $name, string $archiveBaseDir, string $dashboardDir): void {
    if (!preg_match('/^station_status-([A-Z0-9]+)-([0-9]{10,}(?:\.[0-9]+)?)\.json$/i', $name, $m)) {
        respond(400, ['ok' => false, 'error' => 'Bad status filename']);
    }
    $raw = file_get_contents($file['tmp_name']);
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
    $stationFromJson = strtoupper((string)($payload['station'] ?? ''));
    if ($stationFromJson === '' || $stationFromJson !== $stationFromName) {
        respond(400, ['ok' => false, 'error' => 'Station mismatch']);
    }
    $generatedUnix = $payload['generated_unix'] ?? null;
    if (!is_numeric($generatedUnix) || abs((float)$generatedUnix - (float)$m[2]) > 1.0) {
        respond(400, ['ok' => false, 'error' => 'Timestamp mismatch']);
    }
    $dateDir = utc_date_dir_from_unix((float)$m[2]);
    $targetDir = rtrim($archiveBaseDir, '/') . '/' . $dateDir;
    ensure_writable_dir($targetDir);
    $destination = $targetDir . '/' . $name;
    save_upload($file, $destination);
    ensure_writable_dir($dashboardDir);
    $latestDestination = rtrim($dashboardDir, '/') . '/station_status_latest-' . $stationFromName . '.json';
    copy_file_atomic($destination, $latestDestination);
    respond(201, [
        'ok' => true,
        'type' => 'station_status',
        'file' => $name,
        'station' => $stationFromName,
        'unix' => $m[2],
        'date_dir' => $dateDir,
        'path' => $destination,
        'latest_path' => $latestDestination,
    ]);
}

function handle_dashboard_png(array $file, string $name, string $dashboardDir): void {
    $allowed = (
        preg_match('/^(latest|yesterday|previous)[A-Za-z0-9_.-]*\.png$/i', $name)
        || preg_match('/^map(_all|_scand|_us)?\.png$/i', $name)
    );
    if (!$allowed) {
        respond(400, ['ok' => false, 'error' => 'Bad PNG filename']);
    }
    $fh = fopen($file['tmp_name'], 'rb');
    $sig = $fh === false ? false : fread($fh, 8);
    if ($fh !== false) fclose($fh);
    if ($sig !== "\x89PNG\r\n\x1a\n") {
        respond(400, ['ok' => false, 'error' => 'Invalid PNG']);
    }
    ensure_writable_dir($dashboardDir);
    $destination = rtrim($dashboardDir, '/') . '/' . $name;
    save_upload($file, $destination);
    respond(201, [
        'ok' => true,
        'type' => 'dashboard_png',
        'file' => $name,
        'path' => $destination,
    ]);
}

if ($_SERVER['REQUEST_METHOD'] !== 'POST') {
    respond(405, ['ok' => false, 'error' => 'Use POST']);
}

if (!isset($_FILES['file'])) {
    respond(400, ['ok' => false, 'error' => 'Missing file field']);
}

$file = $_FILES['file'];
if (!is_uploaded_file($file['tmp_name'])) {
    respond(400, ['ok' => false, 'error' => 'Invalid upload']);
}

if ($file['error'] !== UPLOAD_ERR_OK) {
    respond(400, ['ok' => false, 'error' => 'Upload failed', 'code' => $file['error']]);
}

if ($file['size'] > $maxBytes) {
    respond(413, ['ok' => false, 'error' => 'Too large']);
}

$name = basename((string)$file['name']);
if ($name !== (string)$file['name'] || $name === '') {
    respond(400, ['ok' => false, 'error' => 'Bad filename']);
}

if (str_ends_with(strtolower($name), '.h5')) {
    handle_h5($file, $name, $archiveBaseDir);
}

if (str_ends_with(strtolower($name), '.json')) {
    handle_status_json($file, $name, $archiveBaseDir, $dashboardDir);
}

if (str_ends_with(strtolower($name), '.png')) {
    handle_dashboard_png($file, $name, $dashboardDir);
}

respond(400, ['ok' => false, 'error' => 'Unsupported file type']);
