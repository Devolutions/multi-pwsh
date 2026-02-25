param(
    [Parameter(Mandatory = $true)]
    [string]$Command,

    [string]$PipeName = ("pwsh-host-cli-" + [Guid]::NewGuid().ToString("N")),

    [int]$ConnectTimeoutMs = 10000
)

$pipePath = "\\.\pipe\$PipeName"
$bytes = [System.Text.Encoding]::UTF8.GetBytes($Command)

$pipe = [System.IO.Pipes.NamedPipeServerStream]::new(
    $PipeName,
    [System.IO.Pipes.PipeDirection]::Out,
    1,
    [System.IO.Pipes.PipeTransmissionMode]::Byte,
    [System.IO.Pipes.PipeOptions]::Asynchronous
)

try {
    $async = $pipe.BeginWaitForConnection($null, $null)
    if (-not $async.AsyncWaitHandle.WaitOne($ConnectTimeoutMs)) {
        throw "Timed out waiting for client connection on $pipePath"
    }

    $pipe.EndWaitForConnection($async)
    $pipe.Write($bytes, 0, $bytes.Length)
    $pipe.Flush()

    [pscustomobject]@{
        PipeName     = $PipeName
        PipePath     = $pipePath
        BytesWritten = $bytes.Length
    }
}
finally {
    $pipe.Dispose()
}
