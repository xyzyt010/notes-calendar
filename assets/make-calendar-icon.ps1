Add-Type -AssemblyName System.Drawing

function New-CalendarIcon($size, $path) {
  $bmp = New-Object System.Drawing.Bitmap($size, $size)
  $g = [System.Drawing.Graphics]::FromImage($bmp)
  $g.SmoothingMode = 'AntiAlias'
  $g.Clear([System.Drawing.Color]::Transparent)

  $s = $size / 512.0
  $bg = New-Object System.Drawing.Drawing2D.GraphicsPath
  $r = 112 * $s
  $bg.AddArc(0, 0, 2*$r, 2*$r, 180, 90)
  $bg.AddArc($size - 2*$r, 0, 2*$r, 2*$r, 270, 90)
  $bg.AddArc($size - 2*$r, $size - 2*$r, 2*$r, 2*$r, 0, 90)
  $bg.AddArc(0, $size - 2*$r, 2*$r, 2*$r, 90, 90)
  $bg.CloseFigure()
  $bgBrush = New-Object System.Drawing.Drawing2D.LinearGradientBrush(
      (New-Object System.Drawing.Point(0, 0)), (New-Object System.Drawing.Point([int]$size, [int]$size)),
      [System.Drawing.Color]::FromArgb(255, 27, 28, 30), [System.Drawing.Color]::FromArgb(255, 16, 17, 19))
  $g.FillPath($bgBrush, $bg)

  # Calendar page
  $pageX = 96.0 * $s; $pageY = 116.0 * $s; $pageW = 320.0 * $s; $pageH = 296.0 * $s; $pageR = 34.0 * $s
  $page = New-Object System.Drawing.Drawing2D.GraphicsPath
  $page.AddArc($pageX, $pageY, 2*$pageR, 2*$pageR, 180, 90)
  $page.AddArc($pageX + $pageW - 2*$pageR, $pageY, 2*$pageR, 2*$pageR, 270, 90)
  $page.AddArc($pageX + $pageW - 2*$pageR, $pageY + $pageH - 2*$pageR, 2*$pageR, 2*$pageR, 0, 90)
  $page.AddArc($pageX, $pageY + $pageH - 2*$pageR, 2*$pageR, 2*$pageR, 90, 90)
  $page.CloseFigure()
  $pageBrush = New-Object System.Drawing.Drawing2D.LinearGradientBrush(
      (New-Object System.Drawing.Point(0, [int]$pageY)), (New-Object System.Drawing.Point(0, [int]($pageY + $pageH))),
      [System.Drawing.Color]::FromArgb(255, 173, 206, 253), [System.Drawing.Color]::FromArgb(255, 127, 169, 247))
  $g.FillPath($pageBrush, $page)

  # Header band
  $bandH = 74.0 * $s
  $band = New-Object System.Drawing.Drawing2D.GraphicsPath
  $band.AddArc($pageX, $pageY, 2*$pageR, 2*$pageR, 180, 90)
  $band.AddArc($pageX + $pageW - 2*$pageR, $pageY, 2*$pageR, 2*$pageR, 270, 90)
  $band.AddLine($pageX + $pageW, $pageY + $bandH, $pageX, $pageY + $bandH)
  $band.CloseFigure()
  $bandBrush = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::FromArgb(255, 15, 23, 42))
  $g.FillPath($bandBrush, $band)

  # Two binder rings (rounded bars)
  $ring = [System.Drawing.Color]::FromArgb(255, 226, 232, 240)
  foreach ($rx in @(168.0, 344.0)) {
    $barW = 17.0 * $s
    $top = 82.0 * $s; $bot = 150.0 * $s
    $capR = $barW / 2.0
    $p = New-Object System.Drawing.Drawing2D.GraphicsPath
    $p.AddArc(($rx * $s - $capR), $top, 2*$capR, 2*$capR, 180, 180)
    $p.AddLine(($rx * $s + $capR), $top + $capR, ($rx * $s + $capR), $bot - $capR)
    $p.AddArc(($rx * $s - $capR), ($bot - 2*$capR), 2*$capR, 2*$capR, 0, 180)
    $p.CloseFigure()
    $g.FillPath((New-Object System.Drawing.SolidBrush($ring)), $p)
  }

  # Grid dots
  $dot = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::FromArgb(160, 15, 23, 42))
  for ($rI = 0; $rI -lt 2; $rI++) {
    for ($cI = 0; $cI -lt 4; $cI++) {
      $cx = (140 + $cI * 82) * $s
      $cy = (234 + $rI * 84) * $s
      $rr = 13 * $s
      $g.FillEllipse($dot, $cx - $rr, $cy - $rr, 2*$rr, 2*$rr)
    }
  }

  $g.Dispose()
  $bmp.Save($path, [System.Drawing.Imaging.ImageFormat]::Png)
  $bmp.Dispose()
}

New-Item -ItemType Directory -Force 'assets' | Out-Null
New-CalendarIcon 512 'assets\calendar-icon.png'
New-CalendarIcon 128 'assets\calendar-icon-128.png'
Get-ChildItem assets\calendar-icon*.png | Select-Object Name, Length
