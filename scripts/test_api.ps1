param(
  [string]$File = "C:\\test\\document.docx",
  [string]$OutputFolder = "C:\\output"
)

$Base = "http://127.0.0.1:24568"

Write-Host "Health check..."
Invoke-RestMethod "$Base/health" | ConvertTo-Json

Write-Host "Check WPS COM..."
Invoke-RestMethod "$Base/api/check-wps" | ConvertTo-Json

Write-Host "Single convert..."
$singleForm = @{
  file = Get-Item $File
  output_name = "output.pdf"
}
Invoke-RestMethod -Uri "$Base/api/convert" -Method Post -Form $singleForm | ConvertTo-Json

Write-Host "Batch convert (async)..."
$batchForm = @{
  files = Get-Item $File
  output_folder = $OutputFolder
}
$response = Invoke-RestMethod -Uri "$Base/api/convert-batch?async=true" -Method Post -Form $batchForm
$response | ConvertTo-Json

Write-Host "Task ID: $($response.task_id)"
