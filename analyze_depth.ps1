$lines = Get-Content "c:\Users\ashin\AndroidStudioProjects\CircleToSearch\app\src\main\java\com\akslabs\circletosearch\ui\CircleToSearchScreen.kt"
$depth = 0
for ($i = 0; $i -lt $lines.Length; $i++) {
    $line = $lines[$i]
    $opens = ($line.ToCharArray() | Where-Object { $_ -eq '{' }).Count
    $closes = ($line.ToCharArray() | Where-Object { $_ -eq '}' }).Count
    $depth += ($opens - $closes)
    # Output any line where depth changes significantly or looks suspicious
    Write-Host "Line $($i+1): Depth=$depth | $line"
}
