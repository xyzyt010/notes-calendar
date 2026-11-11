param(
    [string]$Image = 'notes-9.png',
    [int]$X = 0, [int]$Y = 0, [int]$Width = 700, [int]$Height = 120,
    [string]$OutFile = 'crop.png',
    [double]$Scale = 1.0
)
Add-Type -AssemblyName System.Drawing
$img = [System.Drawing.Image]::FromFile((Resolve-Path $Image))
$w = [int]($Width * $Scale)
$h = [int]($Height * $Scale)
$bmp = New-Object System.Drawing.Bitmap($w, $h)
$g = [System.Drawing.Graphics]::FromImage($bmp)
$g.InterpolationMode = 'HighQualityBicubic'
$src = New-Object System.Drawing.Rectangle($X, $Y, $Width, $Height)
$dst = New-Object System.Drawing.Rectangle(0, 0, $w, $h)
$g.DrawImage($img, $dst, $src, [System.Drawing.GraphicsUnit]::Pixel)
$bmp.Save($OutFile, [System.Drawing.Imaging.ImageFormat]::Png)
$g.Dispose(); $bmp.Dispose(); $img.Dispose()
Write-Output "saved $OutFile"
