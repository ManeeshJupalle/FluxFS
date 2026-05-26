param([Parameter(Mandatory = $true)][string]$InstallDir)

$p = [Environment]::GetEnvironmentVariable('Path', 'User')
if ($p -notlike "*$InstallDir*") {
    $updated = ($p.TrimEnd(';') + ';' + $InstallDir).Trim(';')
    [Environment]::SetEnvironmentVariable('Path', $updated, 'User')
}
