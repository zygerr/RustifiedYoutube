# Render the repository-owned geometric icon using Windows' drawing library.
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
$taskIconDir = Join-Path (Split-Path -Parent $PSScriptRoot) 'src-tauri\icons'
$taskImages = @()
foreach ($taskSize in @(16, 32, 48, 256)) {
    $taskBitmap = [System.Drawing.Bitmap]::new($taskSize, $taskSize)
    $taskGraphics = [System.Drawing.Graphics]::FromImage($taskBitmap)
    $taskGraphics.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
    $taskGraphics.ScaleTransform(($taskSize / 256.0), ($taskSize / 256.0))
    $taskBackground = [System.Drawing.SolidBrush]::new([System.Drawing.ColorTranslator]::FromHtml('#151518'))
    $taskLight = [System.Drawing.SolidBrush]::new([System.Drawing.ColorTranslator]::FromHtml('#fff5ef'))
    $taskRing = [System.Drawing.Pen]::new([System.Drawing.ColorTranslator]::FromHtml('#f49472'), 16)
    $taskPath = [System.Drawing.Drawing2D.GraphicsPath]::new()
    $taskPath.AddArc(0, 0, 112, 112, 180, 90)
    $taskPath.AddArc(144, 0, 112, 112, 270, 90)
    $taskPath.AddArc(144, 144, 112, 112, 0, 90)
    $taskPath.AddArc(0, 144, 112, 112, 90, 90)
    $taskPath.CloseFigure()
    $taskGraphics.FillPath($taskBackground, $taskPath)
    $taskGraphics.DrawEllipse($taskRing, 57, 57, 142, 142)
    $taskPoints = [System.Drawing.PointF[]]@(
        [System.Drawing.PointF]::new(112, 96),
        [System.Drawing.PointF]::new(160, 128),
        [System.Drawing.PointF]::new(112, 160)
    )
    $taskGraphics.FillPolygon($taskLight, $taskPoints)
    $taskStream = [System.IO.MemoryStream]::new()
    $taskBitmap.Save($taskStream, [System.Drawing.Imaging.ImageFormat]::Png)
    $taskImages += @{ Size = $taskSize; Bytes = $taskStream.ToArray() }
    if ($taskSize -eq 256) { [System.IO.File]::WriteAllBytes((Join-Path $taskIconDir 'icon.png'), $taskStream.ToArray()) }
    $taskStream.Dispose()
    $taskGraphics.Dispose()
    $taskBitmap.Dispose()
    $taskPath.Dispose()
    $taskBackground.Dispose()
    $taskLight.Dispose()
    $taskRing.Dispose()
}
$taskIcoStream = [System.IO.MemoryStream]::new()
$taskWriter = [System.IO.BinaryWriter]::new($taskIcoStream)
$taskWriter.Write([uint16]0)
$taskWriter.Write([uint16]1)
$taskWriter.Write([uint16]$taskImages.Count)
$taskOffset = 6 + 16 * $taskImages.Count
foreach ($taskImage in $taskImages) {
    $taskDimension = if ($taskImage.Size -eq 256) { 0 } else { $taskImage.Size }
    $taskWriter.Write([byte]$taskDimension)
    $taskWriter.Write([byte]$taskDimension)
    $taskWriter.Write([byte]0)
    $taskWriter.Write([byte]0)
    $taskWriter.Write([uint16]1)
    $taskWriter.Write([uint16]32)
    $taskWriter.Write([uint32]$taskImage.Bytes.Length)
    $taskWriter.Write([uint32]$taskOffset)
    $taskOffset += $taskImage.Bytes.Length
}
foreach ($taskImage in $taskImages) { $taskWriter.Write([byte[]]$taskImage.Bytes) }
$taskWriter.Flush()
[System.IO.File]::WriteAllBytes((Join-Path $taskIconDir 'icon.ico'), $taskIcoStream.ToArray())
$taskWriter.Dispose()
$taskIcoStream.Dispose()
