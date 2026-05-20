<?php
$dashboardTitle = 'Live TGO Oblique Sounding Dashboard';
$imageGlob = '/var/www/html/iono/*.png';
$imageBaseUrl = '/iono';
$maxAgeHours = 48;
$refreshSeconds = 60;

$plotTypeOrder = [
    'ionogram' => '/^latest-(digisonde|lfm)-/i',
    'rti' => '/^latest-rti-/i',
    'summary' => '/^(?:latest-rothr_jorn-|(?:latest-)?rothr_jorn_|latest_)/i',
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
    'KHO' => 'KHO',
];

function detect_plot_type(string $filename, array $plotTypeOrder): string
{
    foreach ($plotTypeOrder as $label => $pattern) {
        if ($label === 'other') continue;
        if (preg_match($pattern, $filename)) return $label;
    }
    return 'other';
}

function detect_receiver_station(string $filename): ?string
{
    if (preg_match('/^latest-(?:digisonde|lfm|rti)-[^-]+-([A-Z0-9]+)\.png$/i', $filename, $m)) {
        return strtoupper($m[1]);
    }

    if (preg_match('/^latest-rothr_jorn-(?:.*-)?([A-Z0-9]+)\.png$/i', $filename, $m)) {
        return strtoupper($m[1]);
    }

    if (preg_match('/^(?:latest-)?rothr_jorn_today(?:-([A-Z0-9]+))?\.png$/i', $filename, $m)) {
        return isset($m[1]) && $m[1] !== '' ? strtoupper($m[1]) : null;
    }

    if (preg_match('/^latest[_-]([A-Z0-9]+)\.png$/i', $filename, $m)) {
        return strtoupper($m[1]);
    }

    if (preg_match('/^latest-([A-Z0-9]+)-pc\.png$/i', $filename, $m)) {
        return strtoupper($m[1]);
    }

    return null;
}

function station_sort_order(string $station): string
{
    $preferred = ['TGO', 'DOB', 'KHO'];
    $index = array_search($station, $preferred, true);
    if ($index !== false) {
        return sprintf('%03d-%s', $index, $station);
    }
    return '999-' . $station;
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

    if (preg_match('/^latest-rti-([^-]+)-([^.]+)\.png$/i', $filename, $m)) {
        return 'Latest RTI ' . station_label($m[1], $stationLabels) . ' -> ' . $m[2];
    }

    if (preg_match('/^latest-rothr_jorn(?:-([^.]+))?\.png$/i', $filename, $m)) {
        $receiver = isset($m[1]) && $m[1] !== '' ? ' -> ' . $m[1] : '';
        return 'ROTHR/JORN Overview Latest' . $receiver;
    }

    if (preg_match('/^(?:latest-)?rothr_jorn_today(?:-([^.]+))?\.png$/i', $filename, $m)) {
        $receiver = isset($m[1]) && $m[1] !== '' ? ' -> ' . $m[1] : '';
        return 'ROTHR/JORN Overview Today' . $receiver;
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
        if (preg_match('/^latest-rothr_jorn(?:-([^.]+))?\.png$/i', $filename, $m)) {
            $receiver = isset($m[1]) ? strtolower($m[1]) : '';
            return 'rothr_jorn ' . $receiver;
        }

        if (preg_match('/^(?:latest-)?rothr_jorn_today(?:-([^.]+))?\.png$/i', $filename, $m)) {
            $receiver = isset($m[1]) ? strtolower($m[1]) : '';
            return 'rothr_jorn ' . $receiver;
        }
    }

    return strtolower($filename);
}

$cutoff = time() - ($maxAgeHours * 3600);
$mapTabId = 'maps';
$imagePaths = glob($imageGlob) ?: [];
$receiverStations = [];

foreach ($imagePaths as $path) {
    if (!is_file($path)) continue;

    $mtime = filemtime($path);
    $filename = basename($path);
    $plotType = detect_plot_type($filename, $plotTypeOrder);

    if ($mtime === false) continue;
    if ($plotType !== 'map' && $mtime < $cutoff) continue;

    $receiver = detect_receiver_station($filename);
    if ($receiver !== null) {
        $receiverStations[$receiver] = true;
    }
}

$receiverStations = array_keys($receiverStations);
usort($receiverStations, static function (string $a, string $b): int {
    return strcmp(station_sort_order($a), station_sort_order($b));
});

$tabs = [];
foreach ($receiverStations as $receiver) {
    $tabs[$receiver] = $receiver;
}
$tabs[$mapTabId] = 'Maps';
$cardsByTab = array_fill_keys(array_keys($tabs), []);

foreach ($imagePaths as $path) {
    if (!is_file($path)) continue;

    $mtime = filemtime($path);
    $filename = basename($path);
    $plotType = detect_plot_type($filename, $plotTypeOrder);
    $receiver = detect_receiver_station($filename);

    if ($mtime === false) continue;
    if ($plotType !== 'map' && $mtime < $cutoff) continue;

    if ($plotType === 'map') {
        $tab = $mapTabId;
    } elseif ($receiver !== null) {
        $tab = $receiver;
    } else {
        continue;
    }

    $cardsByTab[$tab][] = [
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

foreach ($cardsByTab as &$cards) {
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
foreach ($cardsByTab as $cards) {
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
<link rel="icon" type="image/png" href="uit-logo.png">
<link rel="apple-touch-icon" href="uit-logo.png">
<style>
    body {
        margin: 0;
        font-family: Arial, sans-serif;
        color: #0f172a;
        background: white;
    }

    .menu-bar {
        display: grid;
        grid-template-columns: minmax(220px, 1fr) auto minmax(220px, 1fr);
        align-items: center;
        gap: 16px;
        min-height: 58px;
        padding: 8px 16px;
        border-bottom: 1px solid #e2e8f0;
        background: #ffffff;
        box-shadow: 0 1px 8px rgba(15, 23, 42, 0.08);
    }

    .menu-left {
        min-width: 0;
    }

    .dashboard-title {
        margin-top: 2px;
        color: #475569;
        font-size: 12px;
        font-weight: bold;
        white-space: nowrap;
        overflow: hidden;
        text-overflow: ellipsis;
    }

    .utc-time {
        font-size: 15px;
        font-weight: 600;
        font-family: monospace;
        white-space: nowrap;
    }

    .tabs {
        display: flex;
        justify-content: center;
        gap: 8px;
        flex-wrap: wrap;
    }

    .tab-button {
        border: 1px solid #cbd5e1;
        background: #f8fafc;
        color: #0f172a;
        padding: 7px 18px;
        border-radius: 999px;
        cursor: pointer;
        font-size: 14px;
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

    .logos {
        display: flex;
        justify-content: flex-end;
        align-items: center;
        gap: 16px;
        min-width: 0;
    }

    .logo {
        display: block;
        width: auto;
        max-width: 150px;
        height: 34px;
        object-fit: contain;
    }

    .logo-uit {
        max-width: 95px;
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

    .page-info {
        max-width: 1100px;
        margin: 10px auto 32px;
        padding: 0 16px;
        color: #334155;
        line-height: 1.5;
    }

    .page-info section {
        border-top: 1px solid #e2e8f0;
        padding: 18px 0 0;
        margin-top: 16px;
    }

    .page-info h2 {
        margin: 0 0 8px;
        font-size: 17px;
        color: #0f172a;
    }

    .page-info p {
        margin: 0 0 10px;
    }

    .page-info kbd {
        display: inline-block;
        padding: 1px 6px;
        border: 1px solid #cbd5e1;
        border-bottom-width: 2px;
        border-radius: 5px;
        background: #f8fafc;
        color: #0f172a;
        font-family: monospace;
        font-size: 13px;
    }

    @media (max-width: 850px) {
        .menu-bar {
            grid-template-columns: 1fr;
            justify-items: center;
        }

        .menu-left {
            text-align: center;
        }

        .logos {
            justify-content: center;
        }
    }
</style>
</head>
<body>

<header class="menu-bar">
    <div class="menu-left">
        <div id="utc-time" class="utc-time">---------- --:--:-- UTC</div>
        <div class="dashboard-title"><?php echo htmlspecialchars($dashboardTitle, ENT_QUOTES, 'UTF-8'); ?></div>
    </div>
    <?php if ($hasCards): ?>
    <nav class="tabs" aria-label="Receiver tabs">
    <?php $i = 0; ?>
    <?php foreach ($tabs as $tabId => $tabLabel): ?>
        <button class="tab-button <?php echo $i === 0 ? 'active' : ''; ?>" data-tab="<?php echo htmlspecialchars($tabId, ENT_QUOTES, 'UTF-8'); ?>">
            <?php echo htmlspecialchars($tabLabel, ENT_QUOTES, 'UTF-8'); ?>
        </button>
        <?php $i++; ?>
    <?php endforeach; ?>
    </nav>
    <?php endif; ?>
    <div class="logos">
        <a href="https://uit.no/" aria-label="UiT The Arctic University of Norway">
            <img class="logo logo-uit" src="https://www.tgo.uit.no/UitLogo.png" alt="UiT logo">
        </a>
        <a href="https://www.unis.no/" aria-label="UNIS">
            <img class="logo logo-unis" src="unis-logo-liggende.svg" alt="UNIS logo">
        </a>
    </div>
</header>

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

<?php $i = 0; ?>
<?php foreach ($tabs as $tabId => $tabLabel): ?>
<section class="tab-panel <?php echo $i === 0 ? 'active' : ''; ?>" id="tab-<?php echo htmlspecialchars($tabId, ENT_QUOTES, 'UTF-8'); ?>">
    <?php if (!$cardsByTab[$tabId]): ?>
        <div class="empty-state">
            No plots found for <strong><?php echo htmlspecialchars($tabLabel, ENT_QUOTES, 'UTF-8'); ?></strong>.
        </div>
    <?php else: ?>
        <div class="dashboard">
        <?php foreach ($cardsByTab[$tabId] as $card): ?>
            <div class="card" data-tab="<?php echo htmlspecialchars($tabId, ENT_QUOTES, 'UTF-8'); ?>" data-plot-type="<?php echo htmlspecialchars($card['plotType'], ENT_QUOTES, 'UTF-8'); ?>">
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
<?php $i++; ?>
<?php endforeach; ?>

<?php endif; ?>

<footer class="page-info">
    <section>
        <h2>Keyboard navigation</h2>
        <p>
            Use <kbd>Left</kbd> and <kbd>Right</kbd> to switch between receiver tabs.
            Press <kbd>Enter</kbd> to open the first plot in the active tab.
            When a plot is open, <kbd>Left</kbd> and <kbd>Right</kbd> move between plots
            in that tab, and <kbd>Esc</kbd> closes the plot.
        </p>
    </section>
    <section>
        <h2>About this system</h2>
        <p>
            This monitor shows quick-look products from the Tromsø Geophysical
            Observatory oblique ionogram system. The network is operated by
            Tromsø Geophysical Observatory and was developed by Juha Vierinen
            and Marieluise Schmitt Gran. 
        </p>
        <p>
            Current observing stations are contributed by UiT The Arctic
            University of Norway and UNIS, with station contributions from
            Mikko Syrjäsuo and Lisa Baddeley. These plots are operational
            monitoring products and should be interpreted as quick-look data.
        </p>
    </section>
</footer>

<div class="overlay" id="overlay">
    <img id="overlayImg" src="" alt="Expanded plot">
</div>

<script>
const activeTabStorageKey = 'chirpsounder-dashboard-active-tab';

function activateTab(tab) {
    const button = Array.from(document.querySelectorAll('.tab-button')).find(b => b.dataset.tab === tab);
    const panel = document.getElementById('tab-' + tab);
    if (!button || !panel) return false;

    document.querySelectorAll('.tab-button').forEach(b => b.classList.remove('active'));
    document.querySelectorAll('.tab-panel').forEach(p => p.classList.remove('active'));

    button.classList.add('active');
    panel.classList.add('active');
    localStorage.setItem(activeTabStorageKey, tab);
    return true;
}

document.querySelectorAll('.tab-button').forEach(button => {
    button.addEventListener('click', () => {
        activateTab(button.dataset.tab);
    });
});

const savedTab = localStorage.getItem(activeTabStorageKey);
if (savedTab !== null) {
    activateTab(savedTab);
}

function tabButtons() {
    return Array.from(document.querySelectorAll('.tab-button'));
}

function activeTabIndex() {
    return tabButtons().findIndex(button => button.classList.contains('active'));
}

function activateAdjacentTab(step) {
    const buttons = tabButtons();
    if (buttons.length === 0) return;

    let index = activeTabIndex();
    if (index < 0) index = 0;

    const nextIndex = (index + step + buttons.length) % buttons.length;
    activateTab(buttons[nextIndex].dataset.tab);
}

const overlay = document.getElementById('overlay');
const overlayImg = document.getElementById('overlayImg');
let currentOverlayImage = null;

function openOverlayFromImage(img) {
    currentOverlayImage = img;
    overlayImg.src = img.src;
    overlayImg.alt = img.alt || 'Expanded plot';
    overlay.classList.add('active');
    document.body.classList.add('overlay-open');
}

function closeOverlay() {
    overlay.classList.remove('active');
    document.body.classList.remove('overlay-open');
    overlayImg.src = '';
    currentOverlayImage = null;
}

function activePanelImages() {
    const activePanel = document.querySelector('.tab-panel.active');
    if (!activePanel) return [];
    return Array.from(activePanel.querySelectorAll('img.dashboard-image'));
}

function navigateOverlay(step) {
    if (!overlay.classList.contains('active')) return;
    const images = activePanelImages();
    if (images.length === 0) return;

    let index = images.indexOf(currentOverlayImage);
    if (index < 0) {
        index = images.findIndex(img => img.src === overlayImg.src);
    }
    if (index < 0) index = 0;

    const nextIndex = (index + step + images.length) % images.length;
    openOverlayFromImage(images[nextIndex]);
}

function openFirstImageInActiveTab() {
    const images = activePanelImages();
    if (images.length > 0) {
        openOverlayFromImage(images[0]);
    }
}

document.querySelectorAll('img.dashboard-image').forEach(img => {
    img.addEventListener('click', event => {
        event.preventDefault();
        openOverlayFromImage(img);
    });
});

overlay.addEventListener('click', event => {
    if (event.target === overlay || event.target === overlayImg) {
        closeOverlay();
    }
});

document.addEventListener('keydown', event => {
    const overlayIsOpen = overlay.classList.contains('active');

    if (event.key === 'Escape') {
        event.preventDefault();
        closeOverlay();
    } else if (event.key === 'ArrowLeft') {
        event.preventDefault();
        if (overlayIsOpen) {
            navigateOverlay(-1);
        } else {
            activateAdjacentTab(-1);
        }
    } else if (event.key === 'ArrowRight') {
        event.preventDefault();
        if (overlayIsOpen) {
            navigateOverlay(1);
        } else {
            activateAdjacentTab(1);
        }
    } else if (event.key === 'Enter' && !overlayIsOpen) {
        event.preventDefault();
        openFirstImageInActiveTab();
    }
});
</script>

</body>
</html>
