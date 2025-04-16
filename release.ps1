$v = $args[0]

$f = ".\target\release\auto-man.exe"
$z = ".\target\release\auto-man_windows_x64_v$v.zip"

Compress-Archive -Path $f -DestinationPath $z
