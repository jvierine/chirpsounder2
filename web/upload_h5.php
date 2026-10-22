<?php
declare(strict_types=1);

header('Content-Type: application/json');

$baseDir  = '/mnt/shovel/ionosonde';
$maxBytes = 25 * 1024 * 1024; // bounded scientific product size

umask(0002);

function h5_respond(int $code, array $data): void {
    global $stagedTmp;
    if (isset($stagedTmp) && is_string($stagedTmp)) @unlink($stagedTmp);
    http_response_code($code);
    echo json_encode($data, JSON_UNESCAPED_SLASHES);
    exit;
}

if ($_SERVER['REQUEST_METHOD'] !== 'POST') {
    h5_respond(405, ['ok' => false, 'error' => 'Use POST']);
}

if (!isset($_FILES['file'])) {
    h5_respond(400, ['ok' => false, 'error' => 'Missing file field']);
}

$f = $_FILES['file'];

if (!is_uploaded_file($f['tmp_name'])) {
    h5_respond(400, ['ok' => false, 'error' => 'Invalid upload']);
}

switch ($f['error']) {
    case UPLOAD_ERR_OK:
        break;
    case UPLOAD_ERR_INI_SIZE:
    case UPLOAD_ERR_FORM_SIZE:
        h5_respond(413, ['ok' => false, 'error' => 'File too large']);
    default:
        h5_respond(400, ['ok' => false, 'error' => 'Upload failed', 'code' => $f['error']]);
}

$name = $f['name'];

if ($f['size'] > $maxBytes) {
    h5_respond(413, ['ok' => false, 'error' => 'Too large']);
}

if (!str_ends_with($name, '.h5')) {
    h5_respond(400, ['ok' => false, 'error' => 'File must end with .h5']);
}

$stem = substr($name, 0, -3); // remove ".h5"

// Accept one or more dash-separated labels, then final unix-seconds field.
// Labels: letters, digits, underscore
// Timestamp: 10+ digits, optionally decimal fraction
$pattern = '/^(?<prefix>[A-Za-z0-9_]+(?:-[A-Za-z0-9_]+)*)-(?<ts>[0-9]{10,}(?:\.[0-9]+)?)$/';

if (!preg_match($pattern, $stem, $m)) {
    h5_respond(400, ['ok' => false, 'error' => 'Bad filename']);
}

$prefix = $m['prefix'];
$unixTsString = $m['ts'];

// Use integer seconds to derive directory date
$unixTsInt = (int) floor((float) $unixTsString);

if ($unixTsInt <= 0) {
    h5_respond(400, ['ok' => false, 'error' => 'Bad unix timestamp']);
}

try {
    $dt = new DateTimeImmutable('@' . $unixTsInt);
    $dt = $dt->setTimezone(new DateTimeZone('UTC'));
} catch (Exception $e) {
    h5_respond(400, ['ok' => false, 'error' => 'Invalid timestamp']);
}

$dateDir = $dt->format('Y-m-d');

$targetDir   = $baseDir . '/' . $dateDir;
$destination = $targetDir . '/' . $name;
$tmpDest     = $destination . '.' . bin2hex(random_bytes(8)) . '.tmp';

if (!is_dir($targetDir)) {
    if (!mkdir($targetDir, 2775, true) && !is_dir($targetDir)) {
        h5_respond(500, ['ok' => false, 'error' => 'Failed to create target directory']);
    }
    // The CIFS mount supplies ownership/mode; chmod is unsupported there.
}

if (!is_writable($targetDir)) {
    h5_respond(500, ['ok' => false, 'error' => 'Target directory is not writable']);
}

// Keep the local incoming copy for live catalogue ingestion. The archive is
// still committed first, but the worker need not reread it across SMB.
$stagedTmp = null;
$isBrowser = preg_match('/^browser_day-(TGO|DOB|DMB|KHO|W2NAF)$/', $prefix) === 1;
if ($isBrowser) {
    $stagedTmp = tempnam('/var/lib/iono2-browser/incoming', '.incoming-');
    if ($stagedTmp === false || !copy($f['tmp_name'], $stagedTmp)) {
        h5_respond(503, ['ok' => false, 'error' => 'Local staging unavailable']);
    }
    chmod($stagedTmp, 0664);
}

if (!move_uploaded_file($f['tmp_name'], $tmpDest)) {
    h5_respond(500, ['ok' => false, 'error' => 'Save failed']);
}


// Optional if you want to force group explicitly:
// chgrp($tmpDest, 'shovel');

if (!rename($tmpDest, $destination)) {
    @unlink($tmpDest);
    h5_respond(500, ['ok' => false, 'error' => 'Replace failed']);
}


// Only receiver-ready bundles enter the thin browser service. Raw observation
// uploads remain archived without triggering server-side scientific work.
if ($isBrowser) {
    $inbox = '/var/lib/iono2-browser/inbox';
    $staged = '/var/lib/iono2-browser/incoming/' . basename($name) . '.' . bin2hex(random_bytes(8));
    if (!rename($stagedTmp, $staged)) h5_respond(503, ['ok' => false, 'error' => 'Local publication failed']);
    $stagedTmp = null;
    $event = json_encode(['path' => $destination, 'staged_path' => $staged], JSON_UNESCAPED_SLASHES);
    $ticket = tempnam($inbox, '.upload-');
    if ($ticket === false || file_put_contents($ticket, $event) === false) {
        h5_respond(503, ['ok' => false, 'error' => 'Catalogue queue unavailable']);
    }
    chmod($ticket, 0664);
    if (!rename($ticket, $inbox . '/' . basename($name) . '.json')) {
        h5_respond(503, ['ok' => false, 'error' => 'Catalogue queue publication failed']);
    }
}

h5_respond(201, [
    'ok' => true,
    'file' => $name,
    'prefix' => $prefix,
    'unix' => $unixTsString,
    'date_dir' => $dateDir,
    'path' => $destination
]);
