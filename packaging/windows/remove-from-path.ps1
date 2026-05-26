param([Parameter(Mandatory = $true)][string]$InstallDir)

$p = [Environment]::GetEnvironmentVariable('Path', 'User')
$parts = $p -split ';' | Where-Object { $_ -ne $InstallDir -and $_ -ne '' }
[Environment]::SetEnvironmentVariable('Path', ($parts -join ';'), 'User')
