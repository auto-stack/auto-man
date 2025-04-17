$v = $args[0]

$f = ".\target\release\auto-up.exe"
$z = ".\target\release\auto-up_windows_x64_v$v.zip"

Compress-Archive -Path $f -DestinationPath $z
