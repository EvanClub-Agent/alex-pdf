$ErrorActionPreference = 'Stop'
$version = (Get-Content src-tauri/tauri.conf.json -Raw | ConvertFrom-Json).version
$installer = Get-Item "src-tauri/target/release/bundle/nsis/AlexPDF_${version}_x64-setup.exe"
$installDir = Join-Path $env:RUNNER_TEMP 'AlexPDF-install'
$setup = Start-Process -FilePath $installer.FullName -ArgumentList "/S /D=$installDir" -PassThru -Wait
if ($setup.ExitCode -ne 0) { throw "Installer exited with $($setup.ExitCode)" }

$app = Start-Process (Join-Path $installDir 'AlexPDF.exe') -PassThru
try {
    $health = $null
    $deadline = (Get-Date).AddSeconds(60)
    while ((Get-Date) -lt $deadline) {
        if ($app.HasExited) { throw "Installed AlexPDF exited with $($app.ExitCode)" }
        try {
            $health = Invoke-RestMethod 'http://127.0.0.1:24568/health' -TimeoutSec 5
            break
        } catch {
            Start-Sleep -Seconds 2
        }
    }
    if ($health.status -ne 'ok' -or $health.version -ne $version) {
        throw "Expected the installed AlexPDF $version to report healthy"
    }
    Write-Host "Installed AlexPDF $($health.version); Word engine available: $($health.wps_available)"
    if ($health.wps_available) {
        $sample = Join-Path $env:RUNNER_TEMP 'AlexPDF-check.docx'
        $word = New-Object -ComObject $health.wps_prog_id
        try {
            $doc = $word.Documents.Add()
            $doc.Content.Text = 'AlexPDF release check'
            $doc.SaveAs([ref]$sample, [ref]16)
            $doc.Close()
        } finally {
            $word.Quit()
        }
        $result = Invoke-RestMethod 'http://127.0.0.1:24568/api/convert' -Method Post -Form @{ file = Get-Item $sample }
        if (-not $result.success -or -not (Test-Path -LiteralPath $result.output_file)) { throw 'Sample conversion failed' }
        $bytes = [IO.File]::ReadAllBytes($result.output_file)
        if ([Text.Encoding]::ASCII.GetString($bytes, 0, 5) -ne '%PDF-') { throw 'Invalid PDF header' }
    }
} finally {
    if (-not $app.HasExited) { Stop-Process -Id $app.Id }
}

$asset = Join-Path $env:RUNNER_TEMP 'AlexPDF-Windows-x64-setup.exe'
Copy-Item $installer.FullName $asset
Get-FileHash -LiteralPath $asset -Algorithm SHA256 | Format-List
