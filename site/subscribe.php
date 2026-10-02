<?php
/*
 * Art Window sign-up: the endpoint site/index.html POSTs the form to, for an
 * install link on any platform or the iPhone & iPad waitlist.
 *
 * It stores exactly what the page promises we collect —
 * the address, the date and the platform asked for — as one CSV line on this
 * server, and nothing else: no IP, no user agent, no cookie, no third-party
 * service. A repeat (same address, same platform) is accepted and not stored
 * twice, and the reply doesn't say which it was.
 *
 * The list lives OUTSIDE the web root. Set ARTWINDOW_WAITLIST_FILE in the
 * server environment, or edit the fallback path below. See HOSTING.md.
 */

// a notice printed ahead of the JSON would break the reply; log, don't show
ini_set('display_errors', '0');

$file = getenv('ARTWINDOW_WAITLIST_FILE') ?: __DIR__ . '/../art-window-data/waitlist.csv';

header('Content-Type: application/json; charset=utf-8');
header('Cache-Control: no-store');
header('X-Content-Type-Options: nosniff');

function reply($status, $body) {
    http_response_code($status);
    echo json_encode($body);
    exit;
}

if ($_SERVER['REQUEST_METHOD'] !== 'POST') {
    header('Allow: POST');
    reply(405, ['ok' => false, 'error' => 'method']);
}

// the form's hidden "website" field: people leave it empty, form-filling bots don't
if (!empty($_POST['website'])) {
    reply(200, ['ok' => true]);
}

$email = strtolower(trim((string)($_POST['email'] ?? '')));

// A leading = + - @ would turn the line into a formula when the CSV is opened
// in a spreadsheet; no real address needs one.
if ($email === '' || strlen($email) > 254
    || !filter_var($email, FILTER_VALIDATE_EMAIL)
    || preg_match('/^[=+\-@]/', $email)) {
    reply(422, ['ok' => false, 'error' => 'email']);
}

// anything but a platform the page offers is stored as empty, never rejected
$platform = (string)($_POST['platform'] ?? '');
if (!in_array($platform, ['macos', 'windows', 'gnome', 'android', 'ios'], true)) $platform = '';

$dir = dirname($file);
if (!is_dir($dir) && !@mkdir($dir, 0700, true)) {
    error_log('art-window waitlist: cannot create ' . $dir);
    reply(500, ['ok' => false, 'error' => 'storage']);
}

$fh = @fopen($file, 'c+');
if (!$fh || !flock($fh, LOCK_EX)) {
    error_log('art-window waitlist: cannot open ' . $file);
    reply(500, ['ok' => false, 'error' => 'storage']);
}

$known = false;
while (($row = fgetcsv($fh, 0, ',', '"', '')) !== false) {
    // a row from before the platform column has two fields: platform ''
    if (isset($row[0]) && $row[0] === $email && ($row[2] ?? '') === $platform) { $known = true; break; }
}
if (!$known) {
    fseek($fh, 0, SEEK_END);
    if (ftell($fh) === 0) fputcsv($fh, ['email', 'signed_up_utc', 'platform'], ',', '"', '');
    fputcsv($fh, [$email, gmdate('Y-m-d\TH:i:s\Z'), $platform], ',', '"', '');
    fflush($fh);
}
flock($fh, LOCK_UN);
fclose($fh);

reply(200, ['ok' => true]);
