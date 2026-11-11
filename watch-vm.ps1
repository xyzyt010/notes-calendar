$key = Join-Path $HOME '.ssh\hs01_windows_key'
$target = 'hs01@192.168.1.12'
$sshOpts = @('-i', $key, '-o', 'ConnectTimeout=6', '-o', 'BatchMode=yes', '-o', 'ServerAliveInterval=3', '-o', 'ServerAliveCountMax=2')
for ($i = 1; $i -le 200; $i++) {
  $r = & ssh @sshOpts $target 'echo VM_UP' 2>&1
  if ($LASTEXITCODE -eq 0) { Write-Output "VM reachable on attempt $i"; exit 0 }
  Start-Sleep -Seconds 15
}
Write-Output 'VM UNREACHABLE'
exit 1
