Add-Type -AssemblyName System.Drawing

function New-Icon($size, $path) {
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
      (New-Object System.Drawing.Point(0,0)), (New-Object System.Drawing.Point($size,$size)),
      [System.Drawing.Color]::FromArgb(255, 27, 28, 30), [System.Drawing.Color]::FromArgb(255, 16, 17, 19))
  $g.FillPath($bgBrush, $bg)

  $body = New-Object System.Drawing.Drawing2D.GraphicsPath
  $cx = $size / 2.0
  $headY = 138 * $s; $headR = 92 * $s
  $left = $cx - $headR; $right = $cx + $headR
  $bottom = 340 * $s
  $body.AddArc($left, $headY - $headR, 2*$headR, 2*$headR, 180, 180)
  $body.AddLine($right, $headY, $right, $bottom)
  $bw = (2*$headR) / 3.0
  $x = $right
  for ($i = 0; $i -lt 3; $i++) {
    $body.AddArc($x - $bw, $bottom - 16*$s, $bw, 46*$s, 0, 180)
    $x -= $bw
  }
  $body.AddLine($left, $bottom, $left, $headY)
  $body.CloseFigure()
  $bodyBrush = New-Object System.Drawing.Drawing2D.LinearGradientBrush(
      (New-Object System.Drawing.Point(0, ($headY - $headR))), (New-Object System.Drawing.Point(0, $bottom)),
      [System.Drawing.Color]::FromArgb(255, 173, 206, 253), [System.Drawing.Color]::FromArgb(255, 127, 169, 247))
  $g.FillPath($bodyBrush, $body)

  $eye = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::FromArgb(255, 21, 22, 24))
  $ey = $headY - 8 * $s; $er = 11 * $s; $eh = 19 * $s
  $g.FillEllipse($eye, $cx - 44*$s - $er, $ey - $eh, 2*$er, 2*$eh)
  $g.FillEllipse($eye, $cx + 44*$s - $er, $ey - $eh, 2*$er, 2*$eh)

  $g.Dispose()
  $bmp.Save($path, [System.Drawing.Imaging.ImageFormat]::Png)
  $bmp.Dispose()
}

New-Item -ItemType Directory -Force 'assets' | Out-Null
New-Icon 512 'assets\app-icon.png'
New-Icon 128 'assets\app-icon-128.png'
Get-ChildItem assets\app-icon*.png | Select-Object Name, Length
