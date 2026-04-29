<?php
$dashboardTitle = 'Live TGO Oblique Sounding Dashboard';
$imageGlob = '/var/www/html/iono/*.png';
$imageBaseUrl = '/iono';
$maxAgeHours = 48;
$refreshSeconds = 60;

$receiverStations = ['TGO', 'DOB'];

$plotTypeOrder = [
    'ionogram' => '/^latest-(digisonde|lfm)-/i',
    'rti' => '/^(latest|yesterday)-rti-/i',
    'summary' => '/^(latest-)?rothr_jorn_|^latest_/i',
    'map' => '/^map(_all|_scand)?\.png$/i',
    'pc status' => '/-pc\.png$/i',
    'other' => '/.*/',
];

$stationLabels = [
    'DB049' => 'Dourbes',
    'TH76' => 'Thule',
    'THJ76' => 'Thule',
    'TGO' => 'TGO',
    'DOB' => 'DOB',
];

function detect_plot_type(string $filename, array $plotTypeOrder): string
{
    foreach ($plotTypeOrder as $label => $pattern) {
        if ($label === 'other') continue;
        if (preg_match($pattern, $filename)) return $label;
    }
    return 'other';
}

function detect_receiver_station(string $filename, array $receiverStations): ?string
{
    foreach ($receiverStations as $receiver) {
        $r = preg_quote($receiver, '/');

        if (preg_match('/^latest-(?:digisonde|lfm)-[^-]+-' . $r . '\.png$/i', $filename)) {
            return $receiver;
        }

        if (preg_match('/^(?:latest|yesterday)-rti-[^-]+-' . $r . '\.png$/i', $filename)) {
            return $receiver;
        }

        if (preg_match('/^(?:latest-)?rothr_jorn_(?:today|yesterday)-' . $r . '\.png$/i', $filename)) {
            return $receiver;
        }

        if (preg_match('/-' . $r . '-pc\.png$/i', $filename)) {
            return $receiver;
        }
    }

    return null;
}

function plot_type_rank(string $plotType, array $plotTypeOrder): int
{
    $labels = array_keys($plotTypeOrder);
    $index = array_search($plotType, $labels, true);
    return $index === false ? count($labels) : $index;
}

function station_label(string $station, array $stationLabels): string
{
    return $stationLabels[$station] ?? $station;
}

function label_from_filename(string $filename, array $stationLabels): string
{
    if (preg_match('/^latest-digisonde-([^-]+)-([^.]+)\.png$/i', $filename, $m)) {
        return 'Digisonde ' . station_label($m[1], $stationLabels) . ' -> ' . $m[2];
    }

    if (preg_match('/^latest-lfm-([^-]+)-([^.]+)\.png$/i', $filename, $m)) {
        return 'LFM ' . station_label($m[1], $stationLabels) . ' -> ' . $m[2];
    }

    if (preg_match('/^(latest|yesterday)-rti-([^-]+)-([^.]+)\.png$/i', $filename, $m)) {
        $day = strtolower($m[1]) === 'yesterday' ? 'Yesterday' : 'Latest';
        return $day . ' RTI ' . station_label($m[2], $stationLabels) . ' -> ' . $m[3];
    }

    if (preg_match('/^(?:latest-)?rothr_jorn_(today|yesterday)(?:-([^.]+))?\.png$/i', $filename, $m)) {
        $day = strtolower($m[1]) === 'today' ? 'Today' : 'Yesterday';
        $receiver = isset($m[2]) && $m[2] !== '' ? ' -> ' . $m[2] : '';
        return 'ROTHR/JORN Overview ' . $day . $receiver;
    }

    $stem = preg_replace('/\.png$/i', '', $filename);
    $stem = preg_replace('/^latest-/', '', $stem);
    $label = str_replace(['_', '-'], ' ', $stem);
    $label = preg_replace('/\s+/', ' ', $label ?? '');
    return ucwords(trim((string)$label));
}

function station_sort_key(string $filename, string $plotType, array $stationLabels): string
{
    if ($plotType === 'ionogram') {
        if (preg_match('/^latest-(?:digisonde|lfm)-([^-]+)-([^.]+)\.png$/i', $filename, $m)) {
            return strtolower(station_label($m[1], $stationLabels) . ' ' . $m[2]);
        }
    }

    if ($plotType === 'summary') {
        if (preg_match('/^(?:latest-)?rothr_jorn_(today|yesterday)(?:-([^.]+))?\.png$/i', $filename, $m)) {
            $dayRank = strtolower($m[1]) === 'today' ? '0' : '1';
            $receiver = isset($m[2]) ? strtolower($m[2]) : '';
            return 'rothr_jorn ' . $receiver . ' ' . $dayRank;
        }
    }

    return strtolower($filename);
}

$cutoff = time() - ($maxAgeHours * 3600);
$cardsByReceiver = array_fill_keys($receiverStations, []);

foreach (glob($imageGlob) ?: [] as $path) {
    if (!is_file($path)) continue;

    $mtime = filemtime($path);
    $filename = basename($path);
    $plotType = detect_plot_type($filename, $plotTypeOrder);
    $receiver = detect_receiver_station($filename, $receiverStations);

    if ($receiver === null) continue;
    if ($mtime === false || $mtime < $cutoff) continue;

    $cardsByReceiver[$receiver][] = [
        'filename' => $filename,
        'path' => $path,
        'plotType' => $plotType,
        'plotTypeRank' => plot_type_rank($plotType, $plotTypeOrder),
        'sortKey' => station_sort_key($filename, $plotType, $stationLabels),
        'title' => label_from_filename($filename, $stationLabels),
        'mtime' => $mtime,
        'url' => rtrim($imageBaseUrl, '/') . '/' . rawurlencode($filename),
    ];
}

foreach ($cardsByReceiver as &$cards) {
    usort($cards, static function (array $a, array $b): int {
        if ($a['plotTypeRank'] !== $b['plotTypeRank']) {
            return $a['plotTypeRank'] <=> $b['plotTypeRank'];
        }
        if ($a['sortKey'] !== $b['sortKey']) {
            return strcmp($a['sortKey'], $b['sortKey']);
        }
        return strcmp($a['filename'], $b['filename']);
    });
}
unset($cards);

$hasCards = false;
foreach ($cardsByReceiver as $cards) {
    if ($cards) {
        $hasCards = true;
        break;
    }
}
?>
<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<title><?php echo htmlspecialchars($dashboardTitle, ENT_QUOTES, 'UTF-8'); ?></title>
<meta http-equiv="refresh" content="<?php echo (int)$refreshSeconds; ?>">
<style>
    body {
        margin: 0;
        font-family: Arial, sans-serif;
        color: #0f172a;
        background: white;
    }

    .header {
        display: flex;
        align-items: center;
        justify-content: center;
        padding: 20px 20px 8px 20px;
    }

    .logo {
        margin-right: 12px;
        width: 100px;
    }

    .headertext {
        font-size: 24px;
        font-weight: bold;
    }

    .utc-time {
        font-size: 1.6rem;
        font-weight: 600;
        letter-spacing: 0.06em;
        font-family: monospace;
        display: flex;
        justify-content: center;
        align-items: center;
    }

    .tabs {
        display: flex;
        justify-content: center;
        gap: 8px;
        margin: 18px 16px 6px 16px;
        flex-wrap: wrap;
    }

    .tab-button {
        border: 1px solid #cbd5e1;
        background: #f8fafc;
        color: #0f172a;
        padding: 10px 22px;
        border-radius: 999px;
        cursor: pointer;
        font-size: 16px;
        font-weight: 600;
    }

    .tab-button.active {
        background: #0f172a;
        color: white;
        border-color: #0f172a;
    }

    .tab-panel {
        display: none;
    }

    .tab-panel.active {
        display: block;
    }

    .dashboard {
        display: grid;
        grid-template-columns: repeat(auto-fit, minmax(400px, 1fr));
        gap: 16px;
        padding: 16px;
    }

    .card {
        background: white;
        border-radius: 12px;
        padding: 10px;
        box-shadow: 0 4px 15px rgba(0,0,0,0.4);
        position: relative;
    }

    .card h2 {
        font-size: 16px;
        margin: 8px 0 10px 0;
        text-align: center;
    }

    .card-meta {
        text-align: center;
        font-size: 13px;
        color: #475569;
        margin-bottom: 8px;
    }

    img.dashboard-image {
        width: 100%;
        height: auto;
        object-fit: contain;
        border-radius: 8px;
        cursor: pointer;
    }

    .overlay {
        position: fixed;
        inset: 0;
        width: 100vw;
        height: 100vh;
        background: rgba(0,0,0,0.3);
        display: flex;
        justify-content: center;
        align-items: center;
        z-index: 9999;
        opacity: 0;
        pointer-events: none;
        transition: opacity 0.2s ease;
    }

    .overlay img {
        max-width: 95vw;
        max-height: 95vh;
        width: auto;
        height: auto;
        border-radius: 10px;
        object-fit: contain;
        cursor: pointer;
    }

    .overlay.active {
        opacity: 1;
        pointer-events: all;
    }

    body.overlay-open {
        overflow: hidden;
    }

    .empty-state {
        max-width: 700px;
        margin: 32px auto;
        padding: 18px;
        border-radius: 12px;
        box-shadow: 0 4px 15px rgba(0,0,0,0.15);
        text-align: center;
        color: #334155;
    }
</style>
</head>
<body>

<center>
    <div class="header">
        <img class="logo" src="https://www.tgo.uit.no/UitLogo.png" alt="UiT logo">
        <div class="headertext"><?php echo htmlspecialchars($dashboardTitle, ENT_QUOTES, 'UTF-8'); ?></div>
    </div>
</center>

<div id="utc-time" class="utc-time">---------- --:--:-- UTC</div>
<center><a href="https://github.com/jvierine/chirpsounder2">github.com/jvierine/chirpsounder2</a></center>

<script>
const utcEl = document.getElementById("utc-time");

function updateUtcTime() {
  const now = new Date();
  const year = now.getUTCFullYear();
  const month = String(now.getUTCMonth() + 1).padStart(2, "0");
  const day = String(now.getUTCDate()).padStart(2, "0");
  const hours = String(now.getUTCHours()).padStart(2, "0");
  const minutes = String(now.getUTCMinutes()).padStart(2, "0");
  const seconds = String(now.getUTCSeconds()).padStart(2, "0");
  utcEl.textContent = `${year}-${month}-${day} ${hours}:${minutes}:${seconds} UTC`;
}
updateUtcTime();
setInterval(updateUtcTime, 1000);
</script>

<?php if (!$hasCards): ?>
<div class="empty-state">
    No receiver-station <code>*.png</code> files newer than <?php echo (int)$maxAgeHours; ?> hours were found in
    <code><?php echo htmlspecialchars($imageGlob, ENT_QUOTES, 'UTF-8'); ?></code>.
</div>
<?php else: ?>

<div class="tabs">
<?php foreach ($receiverStations as $i => $receiver): ?>
    <button class="tab-button <?php echo $i === 0 ? 'active' : ''; ?>" data-tab="<?php echo htmlspecialchars($receiver, ENT_QUOTES, 'UTF-8'); ?>">
        <?php echo htmlspecialchars($receiver, ENT_QUOTES, 'UTF-8'); ?>
    </button>
<?php endforeach; ?>
</div>

<?php foreach ($receiverStations as $i => $receiver): ?>
<section class="tab-panel <?php echo $i === 0 ? 'active' : ''; ?>" id="tab-<?php echo htmlspecialchars($receiver, ENT_QUOTES, 'UTF-8'); ?>">
    <?php if (!$cardsByReceiver[$receiver]): ?>
        <div class="empty-state">
            No plots found for receiver station <strong><?php echo htmlspecialchars($receiver, ENT_QUOTES, 'UTF-8'); ?></strong>.
        </div>
    <?php else: ?>
        <div class="dashboard">
        <?php foreach ($cardsByReceiver[$receiver] as $card): ?>
            <div class="card" data-receiver="<?php echo htmlspecialchars($receiver, ENT_QUOTES, 'UTF-8'); ?>" data-plot-type="<?php echo htmlspecialchars($card['plotType'], ENT_QUOTES, 'UTF-8'); ?>">
                <h2><?php echo htmlspecialchars($card['title'], ENT_QUOTES, 'UTF-8'); ?></h2>
                <div class="card-meta">
                    <?php echo htmlspecialchars($card['plotType'], ENT_QUOTES, 'UTF-8'); ?>
                    | updated <?php echo gmdate('Y-m-d H:i:s', (int)$card['mtime']); ?> UTC
                </div>
                <img class="dashboard-image" src="<?php echo htmlspecialchars($card['url'], ENT_QUOTES, 'UTF-8'); ?>" alt="<?php echo htmlspecialchars($card['title'], ENT_QUOTES, 'UTF-8'); ?>">
            </div>
        <?php endforeach; ?>
        </div>
    <?php endif; ?>
</section>
<?php endforeach; ?>

<?php endif; ?>

<div class="overlay" id="overlay">
    <img id="overlayImg" src="" alt="Expanded plot">
</div>

<script>
document.querySelectorAll('.tab-button').forEach(button => {
    button.addEventListener('click', () => {
        const tab = button.dataset.tab;

        document.querySelectorAll('.tab-button').forEach(b => b.classList.remove('active'));
        document.querySelectorAll('.tab-panel').forEach(panel => panel.classList.remove('active'));

        button.classList.add('active');
        document.getElementById('tab-' + tab).classList.add('active');
    });
});

const overlay = document.getElementById('overlay');
const overlayImg = document.getElementById('overlayImg');

function openOverlay(src, alt) {
    overlayImg.src = src;
    overlayImg.alt = alt || 'Expanded plot';
    overlay.classList.add('active');
    document.body.classList.add('overlay-open');
}

function closeOverlay() {
    overlay.classList.remove('active');
    document.body.classList.remove('overlay-open');
    overlayImg.src = '';
}

document.querySelectorAll('img.dashboard-image').forEach(img => {
    img.addEventListener('click', event => {
        event.preventDefault();
        openOverlay(img.src, img.alt);
    });
});

overlay.addEventListener('click', event => {
    if (event.target === overlay || event.target === overlayImg) {
        closeOverlay();
    }
});

document.addEventListener('keydown', event => {
    if (event.key === 'Escape') {
        closeOverlay();
    }
});
</script>

</body>
</html>