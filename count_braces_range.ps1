$lines = Get-Content "c:\Users\ashin\AndroidStudioProjects\CircleToSearch\app\src\main\java\com\akslabs\circletosearch\ui\CircleToSearchScreen.kt"
$totalOpen = 0
$totalClose = 0
for ($i = 0; $i -lt $lines.Length; $i++) {
    $line = $lines[$i]
    $open = ($line.ToCharArray() | Where-Object { $_ -eq '{' }).Count
    $close = ($line.ToCharArray() | Where-Object { $_ -eq '}' }).Count
    $totalOpen += $open
    $totalClose += $close
    if ($i % 500 -eq 0 -or $i -eq $lines.Length - 1) {
        Write-Host "Line $($i+1): Open=$totalOpen, Close=$totalClose, Diff=$($totalOpen - $totalClose)"
    }
}
