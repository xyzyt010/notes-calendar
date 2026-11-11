$p = 'apps\calendar\ui\calendar.slint'
$c = Get-Content $p -Raw
$c = $c -replace 'x: root\.width - 452px;', 'x: 824px;'
$c = $c -replace 'x: root\.width - 208px;', 'x: 1072px;'
$c = $c -replace 'x: root\.width - 60px;', 'x: 1226px;'
Set-Content $p $c -NoNewline
Select-String -Path $p -Pattern 'x: 824px|x: 1072px|x: 1226px' | ForEach-Object { "$($_.LineNumber): $($_.Line.Trim())" }