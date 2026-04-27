<?php
$dashboardTitle = 'Live TGO Oblique Sounding Dashboard';
$imageGlob = '/var/www/html/iono/latest*.png';
$imageBaseUrl = '/iono';
$maxAgeHours = 48;
$refreshSeconds = 60;

// Configure plot type ordering here. The first matching regex wins.
$plotTypeOrder = [
    'pc status' => '/-pc\.png$/i',
    'digisonde' => '/^latest-digisonde-/i',
    'lfm' => '/^latest-lfm-/i',
    'rti' => '/^latest-rti-/i',
    'other' => '/.*/',
];

function detect_plot_type(string $filename, array $plotTypeOrder): string
{
    foreach ($plotTypeOrder as $label => $pattern) {
        if (preg_match($pattern, $filename)) {
            return $label;
        }
    }
    return 'other';
}

function plot_type_rank(string $plotType, array $plotTypeOrder): int
{
    $labels = array_keys($plotTypeOrder);
    $index = array_search($plotType, $labels, true);
    return $index === false ? count($labels) : $index;
}

function label_from_filename(string $filename): string
{
    $stem = preg_replace('/\.png$/i', '', $filename);
    $stem = preg_replace('/^latest-/', '', $stem);
    $label = str_replace(['_', '-'], ' ', $stem);
    $label = preg_replace('/\s+/', ' ', $label ?? '');
    return ucwords(trim((string)$label));
}

$cutoff = time() - ($maxAgeHours * 3600);
$cards = [];
foreach (glob($imageGlob) ?: [] as $path) {
    if (!is_file($path)) {
        continue;
    }
    $mtime = filemtime($path);
    if ($mtime === false || $mtime < $cutoff) {
        continue;
    }
    $filename = basename($path);
    $plotType = detect_plot_type($filename, $plotTypeOrder);
    $cards[] = [
        'filename' => $filename,
        'path' => $path,
        'plotType' => $plotType,
        'plotTypeRank' => plot_type_rank($plotType, $plotTypeOrder),
        'title' => label_from_filename($filename),
        'mtime' => $mtime,
        'url' => rtrim($imageBaseUrl, '/') . '/' . rawurlencode($filename),
    ];
}

usort($cards, static function (array $a, array $b): int {
    if ($a['plotTypeRank'] !== $b['plotTypeRank']) {
        return $a['plotTypeRank'] <=> $b['plotTypeRank'];
    }
    if ($a['mtime'] !== $b['mtime']) {
        return $b['mtime'] <=> $a['mtime'];
    }
    return strcmp($a['filename'], $b['filename']);
});
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

    h1 {
        text-align: center;
        padding: 20px;
        margin: 0;
    }

    .header {
        display: flex;
        align-items: center;
        justify-content: center;
    }

    .logo {
        margin-right: 12px;
        width: 100px;
    }

    .headertext {
        font-size: 24px;
        font-weight: bold;
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
        transition: transform 0.2s ease;
    }

    .utc-time {
        font-size: 1.6rem;
        font-weight: 600;
        letter-spacing: 0.06em;
        font-family: monospace;
        display: flex;
        justify-content: center;
        align-items: center;
        transition: opacity 0.2s ease, transform 0.2s ease;
    }

    .utc-time.tick {
        opacity: 0.6;
        transform: scale(1.05);
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
        border-radius: 10px;
        object-fit: contain;
        cursor: pointer;
    }

    .overlay.active {
        opacity: 1;
        pointer-events: all;
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
<center><div class="header"><img class="logo" src="https://www.tgo.uit.no/UitLogo.png" alt="UiT logo"><div class="headertext"><?php echo htmlspecialchars($dashboardTitle, ENT_QUOTES, 'UTF-8'); ?></div></div></center>
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
  setTimeout(() => utcEl.classList.remove("tick"), 200);
}
updateUtcTime();
setInterval(updateUtcTime, 1000);
</script>

<?php if (!$cards): ?>
<div class="empty-state">
    No <code>latest*.png</code> files newer than <?php echo (int)$maxAgeHours; ?> hours were found in
    <code><?php echo htmlspecialchars($imageGlob, ENT_QUOTES, 'UTF-8'); ?></code>.
</div>
<?php else: ?>
<div class="dashboard">
<?php foreach ($cards as $card): ?>
    <div class="card" data-plot-type="<?php echo htmlspecialchars($card['plotType'], ENT_QUOTES, 'UTF-8'); ?>">
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

<div class="overlay" id="overlay">
    <img id="overlayImg" src="" alt="Expanded plot">
</div>

<script>
const overlay = document.getElementById('overlay');
const overlayImg = document.getElementById('overlayImg');
document.querySelectorAll('img.dashboard-image').forEach(img => {
    img.addEventListener('click', () => {
        if (overlay.classList.contains('active') && overlayImg.src === img.src) {
            overlay.classList.remove('active');
            overlayImg.src = '';
        } else {
            overlay.classList.add('active');
            overlayImg.src = img.src;
        }
    });
});
overlay.addEventListener('click', () => {
    overlay.classList.remove('active');
    overlayImg.src = '';
});
</script>
</body>
</html>
