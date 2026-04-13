$content = Get-Content "c:\Users\ashin\AndroidStudioProjects\CircleToSearch\app\src\main\java\com\akslabs\circletosearch\ui\CircleToSearchScreen.kt" -Raw
$open = ($content.ToCharArray() | Where-Object { $_ -eq '{' }).Count
$close = ($content.ToCharArray() | Where-Object { $_ -eq '}' }).Count
Write-Host "Open: $open"
Write-Host "Close: $close"
Write-Host "Diff: $($open - $close)"
