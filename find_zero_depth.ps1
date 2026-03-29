$lines = Get-Content "c:\Users\ashin\AndroidStudioProjects\CircleToSearch\app\src\main\java\com\akslabs\circletosearch\ui\CircleToSearchScreen.kt"
$depth = 0
for ($i = 0; $i -lt $lines.Length; $i++) {
    $line = $lines[$i]
    $opens = ($line.ToCharArray() | Where-Object { $_ -eq '{' }).Count
    $closes = ($line.ToCharArray() | Where-Object { $_ -eq '}' }).Count
    $depth += ($opens - $closes)
    if ($depth -le 0 -and $i -gt 200 -and $i -lt ($lines.Length - 10)) {
        Write-Host "CRITICAL: Depth hit 0 at line $($i+1): $line"
        # break
    }
}
